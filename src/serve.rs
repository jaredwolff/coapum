use std::{
    collections::HashMap,
    fmt::Debug,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::sync::{
    Mutex,
    mpsc::{self, Sender, channel},
};
use tower::Service;
use webrtc_dtls::{Error, conn::DTLSConn, listener};
use webrtc_util::conn::{Conn, Listener};

use coap_lite::{
    BlockHandler, BlockHandlerConfig, CoapRequest, MessageType, ObserveOption, Packet, RequestType,
    ResponseType,
};

use crate::{
    config::Config,
    credential::{CredentialStore, memory::MemoryCredentialStore},
    observer::{Observer, ObserverValue, validate_observer_path},
    router::{ClientCommand, ClientManager, CoapRouter, CoapumRequest},
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

/// Security constants for connection management
const MIN_RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Per-connection RFC 7641 observe state.
struct ObserveState {
    sequence: u32,
    next_msg_id: u16,
    /// Maps message IDs to observer paths for RST-based deregistration.
    notification_msg_ids: HashMap<u16, String>,
}

impl ObserveState {
    fn new() -> Self {
        Self {
            sequence: 0,
            next_msg_id: 1,
            notification_msg_ids: HashMap::new(),
        }
    }
}

/// Extract and validate PSK identity from a DTLS identity hint.
///
/// Validates length, UTF-8 encoding, and sanitizes to safe characters only.
fn extract_identity(identity_hint: Vec<u8>) -> Option<String> {
    const MAX_IDENTITY_LENGTH: usize = 256;

    if identity_hint.len() > MAX_IDENTITY_LENGTH {
        tracing::error!(
            "Identity hint too long: {} bytes (max: {})",
            identity_hint.len(),
            MAX_IDENTITY_LENGTH
        );
        return None;
    }

    match String::from_utf8(identity_hint) {
        Ok(s) => {
            let sanitized: String = s
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                .take(MAX_IDENTITY_LENGTH)
                .collect();

            if sanitized.is_empty() {
                tracing::error!("Identity hint contains no valid characters");
                None
            } else {
                Some(sanitized)
            }
        }
        Err(e) => {
            tracing::error!("Invalid UTF-8 in identity hint: {}", e);
            None
        }
    }
}

/// Validate connection and implement rate limiting for reconnections.
///
/// Returns `true` if the connection is allowed, `false` if rate-limited or blocked.
async fn manage_connection(
    identity: &str,
    socket_addr: SocketAddr,
    tx: Sender<()>,
    connections: &Mutex<HashMap<String, ConnectionInfo>>,
) -> bool {
    let mut guard = connections.lock().await;

    if let Some(old_conn) = guard.get(identity) {
        if old_conn.established_at.elapsed() < MIN_RECONNECT_INTERVAL {
            tracing::warn!(
                "Rate limited: Rapid reconnection attempt from {} for identity '{}' (interval: {:?})",
                socket_addr,
                identity,
                old_conn.established_at.elapsed()
            );
            return false;
        }

        if old_conn.reconnect_count > MAX_RECONNECT_ATTEMPTS {
            tracing::error!(
                "Security: Too many reconnection attempts from {} for identity '{}' (count: {})",
                socket_addr,
                identity,
                old_conn.reconnect_count
            );
            return false;
        }

        let _ = old_conn.sender.send(()).await;
    }

    let conn_info = ConnectionInfo {
        sender: tx,
        established_at: Instant::now(),
        source_addr: socket_addr,
        reconnect_count: guard
            .get(identity)
            .map(|c| c.reconnect_count + 1)
            .unwrap_or(0),
    };

    guard.insert(identity.to_string(), conn_info);
    tracing::info!(
        "Connection established for identity '{}' from {}",
        identity,
        socket_addr
    );
    true
}

/// Handle an observer notification: route, set RFC 7641 headers, and send.
async fn handle_notification<O, S>(
    value: ObserverValue,
    router: &mut CoapRouter<O, S>,
    conn: &(dyn Conn + Send + Sync),
    socket_addr: SocketAddr,
    obs: &mut ObserveState,
) where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    tracing::info!("Got notification: {:?}", value);

    let notification_path = value.path.clone();
    let req = value.to_request(socket_addr);

    match router.call(req).await {
        Ok(mut resp) => {
            if *resp.get_status() == ResponseType::BadRequest {
                tracing::error!("Error: {:?}", resp.message);
                return;
            }

            // RFC 7641 §3.3: Set observe sequence number
            obs.sequence = obs.sequence.wrapping_add(1);
            resp.message.set_observe_value(obs.sequence);

            // Set message type to Non-Confirmable for notifications
            resp.message.header.set_type(MessageType::NonConfirmable);

            // Assign unique message ID for RST tracking
            let msg_id = obs.next_msg_id;
            obs.next_msg_id = obs.next_msg_id.wrapping_add(1);
            resp.message.header.message_id = msg_id;

            obs.notification_msg_ids.insert(msg_id, notification_path);

            // Bound tracking map to prevent unbounded growth
            if obs.notification_msg_ids.len() > 256 {
                let cutoff = msg_id.wrapping_sub(128);
                obs.notification_msg_ids
                    .retain(|&id, _| id.wrapping_sub(cutoff) < 256);
            }

            tracing::info!(
                "Sending notification (seq={}) to: {}",
                obs.sequence,
                socket_addr
            );
            match resp.message.to_bytes() {
                Ok(bytes) => match conn.send(&bytes).await {
                    Ok(n) => tracing::debug!("Wrote {} notification bytes", n),
                    Err(e) => tracing::error!("Error: {}", e),
                },
                Err(e) => tracing::error!("Failed to serialize response: {}", e),
            }
        }
        Err(e) => tracing::error!("Error: {}", e),
    }
}

/// Send a CoAP response over a connection.
async fn send_response(conn: &(dyn Conn + Send + Sync), resp: &crate::CoapResponse) {
    match resp.message.to_bytes() {
        Ok(bytes) => match conn.send(&bytes).await {
            Ok(n) => tracing::debug!("Wrote {} bytes", n),
            Err(e) => tracing::error!("Error sending response: {}", e),
        },
        Err(e) => tracing::error!("Failed to serialize response: {}", e),
    }
}

/// Handle an incoming CoAP request: block-wise transfer, observe management, routing, and response.
#[allow(clippy::too_many_arguments)]
async fn handle_request<O, S>(
    packet: Packet,
    socket_addr: SocketAddr,
    identity: &str,
    router: &mut CoapRouter<O, S>,
    conn: &(dyn Conn + Send + Sync),
    obs_tx: &Arc<Sender<ObserverValue>>,
    obs: &mut ObserveState,
    block_handler: &mut BlockHandler<SocketAddr>,
    max_observers_per_device: usize,
) where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    // RFC 7641 §3.2: RST deregisters observer
    if packet.header.get_type() == MessageType::Reset {
        if let Some(path) = obs.notification_msg_ids.remove(&packet.header.message_id) {
            tracing::info!("RST deregistration for '{}' path '{}'", identity, path);
            let _ = router.unregister_observer(identity, &path).await;
        }
        return;
    }

    let mut coap_request = CoapRequest::from_packet(packet, socket_addr);

    // RFC 7959: Block1 reassembly / Block2 cache serving
    match block_handler.intercept_request(&mut coap_request) {
        Ok(true) => {
            // Block handler handled it (intermediate Block1 or Block2 cache hit)
            if let Some(ref resp) = coap_request.response {
                send_response(conn, resp).await;
            }
            return;
        }
        Err(e) => {
            tracing::error!("Block transfer error: {}", e.message);
            if let Some(ref resp) = coap_request.response {
                send_response(conn, resp).await;
            }
            return;
        }
        Ok(false) => {} // Not a block request, or Block1 fully reassembled — proceed
    }

    // Save packet for Block2 intercept_response later
    let packet_for_block2 = coap_request.message.clone();

    let mut request: CoapumRequest<SocketAddr> = coap_request.into();
    request.identity = identity.to_string();

    let path = request.get_path();
    let observe_flag = *request.get_observe_flag();
    let method = *request.get_method();

    // Validate observe request and prepare for deferred registration.
    // Registration is deferred until after handler succeeds (RFC 7641 §3.1:
    // the observe option in the response confirms registration).
    let pending_observe = match (observe_flag, method) {
        (Some(ObserveOption::Register), RequestType::Get) => match validate_observer_path(path) {
            Ok(normalized_path) => {
                if !router.has_observe_route(&normalized_path) {
                    tracing::warn!(
                        "Observer registration rejected for '{}' on '{}': no observe route",
                        identity,
                        normalized_path
                    );
                    None
                } else if router.observer_count(identity).await >= max_observers_per_device {
                    tracing::warn!(
                        "Observer registration rejected for '{}' on '{}': limit of {} exceeded",
                        identity,
                        normalized_path,
                        max_observers_per_device
                    );
                    None
                } else {
                    Some(normalized_path)
                }
            }
            Err(e) => {
                tracing::error!(
                    "Invalid observer path '{}' from {}: {}",
                    path,
                    socket_addr,
                    e
                );
                return;
            }
        },
        (Some(ObserveOption::Deregister), RequestType::Delete) => {
            match validate_observer_path(path) {
                Ok(normalized_path) => {
                    if let Err(e) = router.unregister_observer(identity, &normalized_path).await {
                        tracing::error!("Failed to unregister observer: {:?}", e);
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Invalid observer path '{}' from {}: {}",
                        path,
                        socket_addr,
                        e
                    );
                    return;
                }
            }
            None
        }
        _ => None,
    };

    // Route the request
    match router.call(request).await {
        Ok(mut resp) => {
            // RFC 7641 §3.1: Register observer only after handler succeeds
            if let Some(ref normalized_path) = pending_observe
                && !resp.get_status().is_error()
            {
                if let Err(e) = router
                    .register_observer(identity, normalized_path, obs_tx.clone())
                    .await
                {
                    tracing::error!("Failed to register observer: {:?}", e);
                } else {
                    obs.sequence = obs.sequence.wrapping_add(1);
                    resp.message.set_observe_value(obs.sequence);
                }
            }

            // RFC 7959: Fragment large responses using Block2
            let mut block_req = CoapRequest::from_packet(packet_for_block2, socket_addr);
            block_req.response = Some(resp);
            if let Err(e) = block_handler.intercept_response(&mut block_req) {
                tracing::error!("Block transfer response error: {}", e.message);
            }

            if let Some(ref resp) = block_req.response {
                tracing::debug!("Got response: {:?}", resp.message);
                send_response(conn, resp).await;
            }
        }
        Err(e) => tracing::error!("Error: {}", e),
    }
}

/// Start basic CoAP server without client management
pub async fn serve_basic<O, S>(
    addr: String,
    config: Config,
    router: CoapRouter<O, S>,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    let dtls_config = config.dtls_cfg.clone();
    let listener = Arc::new(
        listener::listen(addr.clone(), dtls_config)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
    );
    let connections: Arc<Mutex<HashMap<String, ConnectionInfo>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let active_connections = Arc::new(AtomicUsize::new(0));
    let max_connections = config.max_connections;
    let mut shutdown_rx = config.shutdown.clone();

    loop {
        // Check for shutdown signal alongside accepting connections
        let accept_result = tokio::select! {
            _ = async {
                match &mut shutdown_rx {
                    Some(rx) => { let _ = rx.changed().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                tracing::info!("Shutdown signal received, stopping server");
                return Ok(());
            }
            result = listener.accept() => result,
        };

        if let Ok((conn, socket_addr)) = accept_result {
            tracing::info!("Got a connection from: {}", socket_addr);

            if active_connections.load(Ordering::Relaxed) >= max_connections {
                tracing::warn!(
                    "Connection rejected from {}: limit of {} reached",
                    socket_addr,
                    max_connections
                );
                continue;
            }

            let mut router = router.clone();
            let config_clone = config.clone();
            let timeout = config_clone.timeout;

            let state = if let Some(dtls) = conn.as_any().downcast_ref::<DTLSConn>() {
                dtls.connection_state().await
            } else {
                tracing::error!("Unable to get state!");
                continue;
            };

            let identity = match extract_identity(state.identity_hint) {
                Some(id) => id,
                None => continue,
            };

            tracing::info!("PSK Identity: {}", identity);

            let cons = connections.clone();
            let conn_count = active_connections.clone();
            conn_count.fetch_add(1, Ordering::Relaxed);

            tokio::spawn(async move {
                let (tx, mut rx) = channel::<()>(1);

                if !manage_connection(&identity, socket_addr, tx, &cons).await {
                    conn_count.fetch_sub(1, Ordering::Relaxed);
                    return;
                }

                let (obs_tx, mut obs_rx) = channel::<ObserverValue>(10);
                let obs_tx = Arc::new(obs_tx);

                let mut obs = ObserveState::new();
                let mut block_handler = BlockHandler::new(BlockHandlerConfig {
                    max_total_message_size: config_clone.max_message_size,
                    cache_expiry_duration: config_clone.block_cache_expiry,
                });
                let mut b = vec![0u8; config_clone.buffer_size()];

                loop {
                    tokio::select! {
                        Some(value) = obs_rx.recv() => {
                            handle_notification(
                                value, &mut router, conn.as_ref(), socket_addr,
                                &mut obs,
                            ).await;
                        }
                        recv = tokio::time::timeout(Duration::from_secs(timeout), conn.recv(&mut b)) => {
                            let recv = match recv {
                                Ok(r) => r,
                                Err(e) => {
                                    tracing::error!("Timeout! Err: {}", e);
                                    let _ = cons.lock().await.remove(&identity);
                                    break;
                                }
                            };

                            if let Ok(n) = recv {
                                let packet = match Packet::from_bytes(&b[..n]) {
                                    Ok(p) => p,
                                    Err(e) => {
                                        tracing::error!("Failed to parse packet: {}", e);
                                        continue;
                                    }
                                };
                                handle_request(
                                    packet, socket_addr, &identity, &mut router,
                                    conn.as_ref(), &obs_tx, &mut obs,
                                    &mut block_handler,
                                    config_clone.max_observers_per_device,
                                ).await;
                            }
                        }
                        _ = rx.recv() => {
                            tracing::info!("Terminating connection with: {}", socket_addr);
                            break;
                        }
                    }
                }

                conn_count.fetch_sub(1, Ordering::Relaxed);
                cons.lock().await.remove(&identity);
                let _ = router.unregister_all(&identity).await;
                tracing::info!(
                    "Terminated connection for identity: {} ({})",
                    &identity,
                    &socket_addr
                );
            });
        }
    }
}

/// Start a basic CoAP server without client management
///
/// This function runs a CoAP server that blocks forever, handling incoming requests
/// using the provided router. For client management capabilities, use
/// `serve_with_client_management()` instead.
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
    serve_basic(addr, config, router).await
}

/// Start a CoAP server with dynamic client management capability
///
/// This function sets up client management and returns both a ClientManager for real-time
/// client operations and a Future that runs the server. The user controls when and how
/// to execute the server future.
///
/// # Returns
///
/// Returns a tuple of:
/// - A ClientManager handle for managing clients
/// - A Future that runs the server (user must execute it)
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
/// // Configure initial clients
/// let mut initial_clients = HashMap::new();
/// initial_clients.insert("device_001".to_string(), b"secret_key_123".to_vec());
///
/// let config = Config::default().with_client_management(initial_clients);
///
/// // Setup client management and get server future
/// let (client_manager, server_future) = serve_with_client_management(
///     "0.0.0.0:5683".to_string(),
///     config,
///     router
/// ).await?;
///
/// // Add a new client before starting server
/// client_manager.add_client("device_002", b"new_secret").await?;
///
/// // User controls how to run the server
/// tokio::spawn(async move {
///     if let Err(e) = server_future.await {
///         tracing::error!("Server error: {}", e);
///     }
/// });
///
/// // Continue using client manager
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
    // Check if client management is enabled
    let initial_clients = config
        .initial_clients
        .as_ref()
        .ok_or("Client management not enabled. Use Config::with_client_management() to enable.")?;

    // Build MemoryCredentialStore from initial clients
    let credential_store = MemoryCredentialStore::from_clients(initial_clients);

    // Create client management channel
    let (cmd_sender, mut cmd_receiver) = mpsc::channel(config.client_command_buffer);
    let client_manager = ClientManager::new(cmd_sender);

    // Spawn client command processor
    let store_for_processor = credential_store.clone();
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            process_client_command(cmd, &store_for_processor).await;
        }
    });

    // Return client manager and server future
    let server_future = serve_with_credential_store(addr, config, router, credential_store);

    Ok((client_manager, server_future))
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
    // Create DTLS config with PSK callback wired to the credential store
    let mut dtls_cfg = config.dtls_cfg.clone();

    dtls_cfg.psk = Some(Arc::new(move |hint: &[u8]| -> Result<Vec<u8>, Error> {
        let hint_str = String::from_utf8(hint.to_vec()).map_err(|_| Error::ErrIdentityNoPsk)?;

        tracing::debug!("PSK callback for identity: {}", hint_str);

        match credential_store.lookup_psk(&hint_str) {
            Ok(Some(entry)) if entry.enabled => {
                tracing::info!("PSK found for identity: {}", hint_str);
                Ok(entry.key)
            }
            Ok(Some(_)) => {
                tracing::warn!("Client {} is disabled", hint_str);
                Err(Error::ErrIdentityNoPsk)
            }
            Ok(None) => {
                tracing::warn!("PSK not found for identity: {}", hint_str);
                Err(Error::ErrIdentityNoPsk)
            }
            Err(e) => {
                tracing::error!("Credential store error for {}: {:?}", hint_str, e);
                Err(Error::ErrIdentityNoPsk)
            }
        }
    }));

    let mut final_config = config;
    final_config.dtls_cfg = dtls_cfg;

    serve_basic(addr, final_config, router).await
}

/// Process a client command by delegating to a credential store.
async fn process_client_command<C: CredentialStore>(cmd: ClientCommand, store: &C) {
    match cmd {
        ClientCommand::AddClient {
            identity,
            key,
            metadata,
        } => {
            if let Err(e) = store.add_client(&identity, key, metadata).await {
                tracing::error!("Failed to add client {}: {:?}", identity, e);
            }
        }
        ClientCommand::RemoveClient { identity } => {
            if let Err(e) = store.remove_client(&identity).await {
                tracing::error!("Failed to remove client {}: {:?}", identity, e);
            }
        }
        ClientCommand::UpdateKey { identity, key } => {
            if let Err(e) = store.update_key(&identity, key).await {
                tracing::error!("Failed to update key for {}: {:?}", identity, e);
            }
        }
        ClientCommand::UpdateMetadata { identity, metadata } => {
            if let Err(e) = store.update_metadata(&identity, metadata).await {
                tracing::error!("Failed to update metadata for {}: {:?}", identity, e);
            }
        }
        ClientCommand::SetClientEnabled { identity, enabled } => {
            if let Err(e) = store.set_enabled(&identity, enabled).await {
                tracing::error!("Failed to set enabled for {}: {:?}", identity, e);
            }
        }
        ClientCommand::ListClients { response } => match store.list_clients().await {
            Ok(clients) => {
                let _ = response.send(clients);
            }
            Err(e) => {
                tracing::error!("Failed to list clients: {:?}", e);
                let _ = response.send(vec![]);
            }
        },
    }
}

/// Create a client manager connected to a credential store.
///
/// This is useful when you want to manage clients from multiple places
/// or integrate with existing authentication systems.
pub fn create_client_manager<C: CredentialStore>(
    credential_store: C,
    buffer_size: usize,
) -> ClientManager {
    let (cmd_sender, mut cmd_receiver) = mpsc::channel(buffer_size);

    // Spawn command processor
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            process_client_command(cmd, &credential_store).await;
        }
    });

    ClientManager::new(cmd_sender)
}
