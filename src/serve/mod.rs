mod client_mgmt;
mod connection;
mod handlers;
mod helpers;

pub use client_mgmt::create_client_manager;
pub(crate) use helpers::extract_identity;
use helpers::{extract_cid, generate_cid};

use std::{
    collections::HashMap,
    fmt::Debug,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use tokio::{
    net::UdpSocket,
    sync::{
        Mutex,
        mpsc::{self, Sender},
        watch,
    },
};

use crate::{
    config::Config,
    credential::{CredentialStore, memory::MemoryCredentialStore},
    observer::Observer,
    router::{ClientManager, CoapRouter},
};

/// Connection information for security tracking and rate limiting
#[derive(Debug, Clone)]
struct ConnectionInfo {
    sender: Sender<()>,
    established_at: Instant,
    #[allow(dead_code)] // Reserved for future security features
    source_addr: SocketAddr,
    reconnect_count: u32,
}

/// Per-connection RFC 7641 observe state.
struct ObserveState {
    sequence: u32,
    next_msg_id: u16,
    /// Maps message IDs to observer paths for RST-based deregistration.
    notification_msg_ids: HashMap<u16, String>,
    /// RFC 7252 §5.3.1: Maps observer paths to the token from the original
    /// OBSERVE GET so notifications echo the correct token.
    observer_tokens: HashMap<String, Vec<u8>>,
}

impl ObserveState {
    fn new() -> Self {
        Self {
            sequence: 0,
            next_msg_id: 1,
            notification_msg_ids: HashMap::new(),
            observer_tokens: HashMap::new(),
        }
    }
}

/// Start basic CoAP server with quinn-style dispatch + per-connection tasks.
///
/// Each connection gets its own `CapturingResolver` wrapping the shared
/// `credential_store`, so PSK identity capture is race-free under concurrency.
pub async fn serve_basic<O, S, C>(
    addr: String,
    config: Config,
    router: CoapRouter<O, S>,
    credential_store: C,
    psk_identity_hint: Option<Vec<u8>>,
    mut disconnect_rx: Option<mpsc::Receiver<String>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
    C: CredentialStore,
{
    let socket = Arc::new(UdpSocket::bind(&addr).await?);
    tracing::info!(addr = %addr, "server.started");

    let connections: Arc<Mutex<HashMap<String, ConnectionInfo>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let active_connections = Arc::new(AtomicUsize::new(0));
    let max_connections = config.max_connections;
    let cid_length = config.cid_length;
    let mut shutdown_rx = config.shutdown.clone();

    // Dispatch table: SocketAddr → per-connection packet sender
    let mut addr_dispatch: HashMap<SocketAddr, mpsc::Sender<Vec<u8>>> = HashMap::new();

    // CID dispatch tables (RFC 9146): route packets by Connection ID when
    // the client's address changes. Only populated when cid_length is set.
    let mut cid_dispatch: HashMap<Vec<u8>, mpsc::Sender<Vec<u8>>> = HashMap::new();
    let mut cid_to_addr: HashMap<Vec<u8>, SocketAddr> = HashMap::new();
    let mut cid_addr_tx: HashMap<Vec<u8>, watch::Sender<SocketAddr>> = HashMap::new();

    // Cleanup channel: connection tasks notify dispatch when they exit
    let (cleanup_tx, mut cleanup_rx) = mpsc::channel::<(SocketAddr, Option<Vec<u8>>)>(64);

    let mut recv_buf = vec![0u8; config.buffer_size()];

    loop {
        // Drain completed connections
        while let Ok((addr, maybe_cid)) = cleanup_rx.try_recv() {
            addr_dispatch.remove(&addr);
            if let Some(cid) = maybe_cid {
                cid_dispatch.remove(&cid);
                cid_to_addr.remove(&cid);
                cid_addr_tx.remove(&cid);
            }
        }

        // Drain disconnect commands
        if let Some(ref mut rx) = disconnect_rx {
            while let Ok(identity) = rx.try_recv() {
                let cons = connections.lock().await;
                if let Some(info) = cons.get(&identity) {
                    let _ = info.sender.send(()).await;
                    tracing::info!(identity = %identity, "client.disconnected");
                }
            }
        }

        tokio::select! {
            // Shutdown signal
            _ = async {
                match &mut shutdown_rx {
                    Some(rx) => { let _ = rx.changed().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                tracing::info!("Shutdown signal received, stopping server");
                return Ok(());
            }

            // Incoming UDP packet
            result = socket.recv_from(&mut recv_buf) => {
                let (n, remote) = result?;
                let raw = &recv_buf[..n];

                // CID dispatch: route by Connection ID when available (RFC 9146)
                if let Some(cid_len) = cid_length
                    && let Some(cid) = extract_cid(raw, cid_len)
                {
                    if let Some(tx) = cid_dispatch.get(cid) {
                        if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(raw.to_vec()) {
                            tracing::trace!(addr = %remote, "dispatch.cid.backpressure");
                        }

                        // Address migration: update mappings if source changed
                        if cid_to_addr.get(cid) != Some(&remote) {
                            if let Some(old_addr) = cid_to_addr.insert(cid.to_vec(), remote) {
                                addr_dispatch.remove(&old_addr);
                            }
                            addr_dispatch.insert(remote, tx.clone());
                            if let Some(addr_tx) = cid_addr_tx.get(cid) {
                                let _ = addr_tx.send(remote);
                            }
                        }

                        continue;
                    }
                    // Unknown CID — drop (stale or spoofed)
                    tracing::trace!(addr = %remote, "cid.unknown_dropped");
                    continue;
                }

                // Address dispatch: standard path for non-CID packets and handshakes
                if let Some(tx) = addr_dispatch.get(&remote) {
                    // Fast path: known connection
                    if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(raw.to_vec()) {
                        tracing::trace!(addr = %remote, "dispatch.addr.backpressure");
                    }
                } else {
                    // New connection
                    if active_connections.load(Ordering::Relaxed) >= max_connections {
                        tracing::warn!(
                            addr = %remote,
                            limit = max_connections,
                            "connection.rejected.limit"
                        );
                        continue;
                    }

                    tracing::debug!(addr = %remote, "connection.incoming");

                    let (tx, rx) = mpsc::channel(256);
                    let _ = tx.try_send(raw.to_vec());
                    addr_dispatch.insert(remote, tx.clone());

                    // Generate CID and pre-register in dispatch tables
                    let cid = cid_length.map(|len| {
                        let cid = generate_cid(len);
                        cid_dispatch.insert(cid.clone(), tx);
                        cid_to_addr.insert(cid.clone(), remote);
                        cid
                    });

                    let (addr_tx, addr_rx) = watch::channel(remote);
                    if let Some(ref cid) = cid {
                        cid_addr_tx.insert(cid.clone(), addr_tx);
                    }

                    active_connections.fetch_add(1, Ordering::Relaxed);

                    let socket = socket.clone();
                    let store = credential_store.clone();
                    let hint = psk_identity_hint.clone();
                    let router = router.clone();
                    let config = config.clone();
                    let connections = connections.clone();
                    let conn_count = active_connections.clone();
                    let cleanup_tx = cleanup_tx.clone();

                    tokio::spawn(async move {
                        connection::connection_task(
                            remote, rx, socket, store,
                            hint, cid, addr_rx,
                            router, config, connections,
                            conn_count, cleanup_tx,
                        ).await;
                    });
                }
            }
        }
    }
}

/// Start a basic CoAP server without client management.
///
/// Requires `config.dimpl_cfg` to be set with a valid dimpl configuration
/// including a PSK resolver.
///
/// # Example
///
/// ```rust,no_run
/// # use coapum::{RouterBuilder, observer::memory::MemObserver, config::Config};
/// # use coapum::serve::serve;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # #[derive(Clone, Debug)]
/// # struct AppState {}
/// # let state = AppState {};
/// # let observer = MemObserver::new();
/// # let router = RouterBuilder::new(state, observer).build();
///
/// let config = Config::default();
/// serve("0.0.0.0:5683".to_string(), config, router).await?;
/// # Ok(())
/// # }
/// ```
pub async fn serve<O, S>(
    addr: String,
    config: Config,
    router: CoapRouter<O, S>,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    if config.dimpl_cfg.is_none() {
        return Err(
            "DTLS config not set. Set config.dimpl_cfg or use serve_with_credential_store()."
                .into(),
        );
    }

    // Create a no-op store for the basic serve case (identity captured by user's resolver)
    let store = MemoryCredentialStore::new();
    let hint = config.psk_identity_hint.clone();

    serve_basic(addr, config, router, store, hint, None).await
}

/// Start a CoAP server with a custom credential store for PSK authentication.
///
/// This is the primary API for plugging in custom credential backends (e.g.,
/// PostgreSQL, Redis). The credential store handles PSK lookup during DTLS
/// handshakes and can be managed directly by the caller.
///
/// # Example
///
/// ```rust,no_run
/// # use coapum::{RouterBuilder, observer::memory::MemObserver, config::Config};
/// # use coapum::credential::memory::MemoryCredentialStore;
/// # use coapum::serve::serve_with_credential_store;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # #[derive(Clone, Debug)]
/// # struct AppState {}
/// # let state = AppState {};
/// # let observer = MemObserver::new();
/// # let router = RouterBuilder::new(state, observer).build();
/// let config = Config::default();
/// let credentials = MemoryCredentialStore::new();
///
/// serve_with_credential_store("0.0.0.0:5683".to_string(), config, router, credentials).await?;
/// # Ok(())
/// # }
/// ```
pub async fn serve_with_credential_store<O, S, C>(
    addr: String,
    config: Config,
    router: CoapRouter<O, S>,
    credential_store: C,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
    C: CredentialStore,
{
    let hint = config.psk_identity_hint.clone();
    serve_basic(addr, config, router, credential_store, hint, None).await
}

/// Start a CoAP server with dynamic client management capability.
///
/// # Example
///
/// ```rust,no_run
/// # use coapum::{RouterBuilder, observer::memory::MemObserver, config::Config};
/// # use coapum::serve::serve_with_client_management;
/// # use std::collections::HashMap;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # #[derive(Clone, Debug)]
/// # struct AppState {}
/// # let state = AppState {};
/// # let observer = MemObserver::new();
/// # let router = RouterBuilder::new(state, observer).build();
///
/// let mut initial_clients = HashMap::new();
/// initial_clients.insert("device_001".to_string(), b"secret_key_123".to_vec());
///
/// let config = Config::default().with_client_management(initial_clients);
///
/// let (client_manager, server_future) = serve_with_client_management(
///     "0.0.0.0:5683".to_string(),
///     config,
///     router
/// ).await?;
///
/// client_manager.add_client("device_002", b"new_secret").await?;
///
/// tokio::spawn(async move {
///     if let Err(e) = server_future.await {
///         tracing::error!("Server error: {}", e);
///     }
/// });
///
/// client_manager.update_key("device_001", b"rotated_key").await?;
/// # Ok(())
/// # }
/// ```
pub async fn serve_with_client_management<O, S>(
    addr: String,
    config: Config,
    router: CoapRouter<O, S>,
) -> Result<
    (
        ClientManager,
        impl std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
    ),
    Box<dyn std::error::Error>,
>
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    let initial_clients = config
        .initial_clients
        .as_ref()
        .ok_or("Client management not enabled. Use Config::with_client_management() to enable.")?;

    let credential_store = MemoryCredentialStore::from_clients(initial_clients);

    let (cmd_sender, mut cmd_receiver) = mpsc::channel(config.client_command_buffer);
    let client_manager = ClientManager::new(cmd_sender);

    let (disconnect_tx, disconnect_rx) = mpsc::channel::<String>(32);

    let store_for_processor = credential_store.clone();
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            client_mgmt::process_client_command(cmd, &store_for_processor, &disconnect_tx).await;
        }
    });

    let hint = config.psk_identity_hint.clone();
    let server_future = serve_basic(
        addr,
        config,
        router,
        credential_store,
        hint,
        Some(disconnect_rx),
    );

    Ok((client_manager, server_future))
}

/// Start a CoAP server with a custom credential store and client management.
///
/// # Example
///
/// ```rust,no_run
/// # use coapum::{RouterBuilder, observer::memory::MemObserver, config::Config};
/// # use coapum::credential::memory::MemoryCredentialStore;
/// # use coapum::serve::serve_with_credential_store_and_management;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # #[derive(Clone, Debug)]
/// # struct AppState {}
/// # let state = AppState {};
/// # let observer = MemObserver::new();
/// # let router = RouterBuilder::new(state, observer).build();
/// let config = Config::default();
/// let credentials = MemoryCredentialStore::new();
///
/// let (client_manager, server_future) =
///     serve_with_credential_store_and_management(
///         "0.0.0.0:5683".to_string(), config, router, credentials,
///     ).await?;
///
/// tokio::spawn(async move {
///     if let Err(e) = server_future.await {
///         tracing::error!("Server error: {}", e);
///     }
/// });
///
/// client_manager.disconnect_client("revoked_device").await?;
/// # Ok(())
/// # }
/// ```
pub async fn serve_with_credential_store_and_management<O, S, C>(
    addr: String,
    config: Config,
    router: CoapRouter<O, S>,
    credential_store: C,
) -> Result<
    (
        ClientManager,
        impl std::future::Future<Output = Result<(), Box<dyn std::error::Error>>>,
    ),
    Box<dyn std::error::Error>,
>
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
    C: CredentialStore,
{
    let (cmd_sender, mut cmd_receiver) = mpsc::channel(config.client_command_buffer);
    let client_manager = ClientManager::new(cmd_sender);

    let (disconnect_tx, disconnect_rx) = mpsc::channel::<String>(32);

    let store_for_processor = credential_store.clone();
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            client_mgmt::process_client_command(cmd, &store_for_processor, &disconnect_tx).await;
        }
    });

    let hint = config.psk_identity_hint.clone();
    let server_future = serve_basic(
        addr,
        config,
        router,
        credential_store,
        hint,
        Some(disconnect_rx),
    );

    Ok((client_manager, server_future))
}
