use std::{
    collections::HashMap,
    fmt::Debug,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{
    Mutex, RwLock,
    mpsc::{self, Sender, channel},
};
use tower::Service;
use webrtc_dtls::{Error, conn::DTLSConn, listener};
use webrtc_util::conn::{Conn, Listener};

use coap_lite::{CoapRequest, MessageType, ObserveOption, Packet, RequestType, ResponseType};

use crate::{
    config::Config,
    observer::{Observer, ObserverValue},
    router::{
        ClientCommand, ClientEntry, ClientManager, ClientMetadata, ClientStore, CoapRouter,
        CoapumRequest,
    },
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

/// Path validation errors
#[derive(Debug)]
enum PathValidationError {
    TraversalAttempt,
    PathTooDeep,
    InvalidCharacters,
    EmptyPath,
}

impl std::fmt::Display for PathValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathValidationError::TraversalAttempt => write!(f, "Path traversal attempt detected"),
            PathValidationError::PathTooDeep => write!(f, "Path too deep (max 10 components)"),
            PathValidationError::InvalidCharacters => write!(f, "Path contains invalid characters"),
            PathValidationError::EmptyPath => write!(f, "Path is empty"),
        }
    }
}

impl std::error::Error for PathValidationError {}

/// Security: Validate and normalize observer path to prevent injection attacks
fn validate_observer_path(path: &str) -> Result<String, PathValidationError> {
    if path.is_empty() {
        return Err(PathValidationError::EmptyPath);
    }

    // Security: Reject paths containing dangerous patterns
    if path.contains("..") || path.contains("./") || path.contains("\\") {
        return Err(PathValidationError::TraversalAttempt);
    }

    // Normalize and validate path components
    let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // Security: Limit path depth to prevent resource exhaustion
    const MAX_PATH_DEPTH: usize = 10;
    if components.len() > MAX_PATH_DEPTH {
        return Err(PathValidationError::PathTooDeep);
    }

    // Security: Validate each path component for safe characters only
    for component in &components {
        if !component
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(PathValidationError::InvalidCharacters);
        }
    }

    // Return normalized path
    if components.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", components.join("/")))
    }
}

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
        log::error!(
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
                log::error!("Identity hint contains no valid characters");
                None
            } else {
                Some(sanitized)
            }
        }
        Err(e) => {
            log::error!("Invalid UTF-8 in identity hint: {}", e);
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
            log::warn!(
                "Rate limited: Rapid reconnection attempt from {} for identity '{}' (interval: {:?})",
                socket_addr,
                identity,
                old_conn.established_at.elapsed()
            );
            return false;
        }

        if old_conn.reconnect_count > MAX_RECONNECT_ATTEMPTS {
            log::error!(
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
    log::info!(
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
    log::info!("Got notification: {:?}", value);

    let notification_path = value.path.clone();
    let req = value.to_request(socket_addr);

    match router.call(req).await {
        Ok(mut resp) => {
            if *resp.get_status() == ResponseType::BadRequest {
                log::error!("Error: {:?}", resp.message);
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

            log::info!(
                "Sending notification (seq={}) to: {}",
                obs.sequence,
                socket_addr
            );
            match resp.message.to_bytes() {
                Ok(bytes) => match conn.send(&bytes).await {
                    Ok(n) => log::debug!("Wrote {} notification bytes", n),
                    Err(e) => log::error!("Error: {}", e),
                },
                Err(e) => log::error!("Failed to serialize response: {}", e),
            }
        }
        Err(e) => log::error!("Error: {}", e),
    }
}

/// Handle an incoming CoAP request: observe management, routing, and response.
async fn handle_request<O, S>(
    packet: Packet,
    socket_addr: SocketAddr,
    identity: &str,
    router: &mut CoapRouter<O, S>,
    conn: &(dyn Conn + Send + Sync),
    obs_tx: &Arc<Sender<ObserverValue>>,
    obs: &mut ObserveState,
) where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    // RFC 7641 §3.2: RST deregisters observer
    if packet.header.get_type() == MessageType::Reset {
        if let Some(path) = obs.notification_msg_ids.remove(&packet.header.message_id) {
            log::info!("RST deregistration for '{}' path '{}'", identity, path);
            let _ = router.unregister_observer(identity, &path).await;
        }
        return;
    }

    let request = CoapRequest::from_packet(packet, socket_addr);
    let mut request: CoapumRequest<SocketAddr> = request.into();
    request.identity = identity.to_string();

    let path = request.get_path();
    let observe_flag = *request.get_observe_flag();
    let method = *request.get_method();

    // Handle observations
    match (observe_flag, method) {
        (Some(ObserveOption::Register), RequestType::Get) => match validate_observer_path(path) {
            Ok(normalized_path) => {
                if router.has_observe_route(&normalized_path) {
                    if let Err(e) = router
                        .register_observer(identity, &normalized_path, obs_tx.clone())
                        .await
                    {
                        log::error!("Failed to register observer: {:?}", e);
                    }
                } else {
                    log::warn!(
                        "Observer registration rejected for '{}' on '{}': no observe route",
                        identity,
                        normalized_path
                    );
                }
            }
            Err(e) => {
                log::error!(
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
                        log::error!("Failed to unregister observer: {:?}", e);
                    }
                }
                Err(e) => {
                    log::error!(
                        "Invalid observer path '{}' from {}: {}",
                        path,
                        socket_addr,
                        e
                    );
                    return;
                }
            }
        }
        _ => {}
    }

    // Route the request
    match router.call(request).await {
        Ok(mut resp) => {
            // RFC 7641 §3.1: Include observe option in registration response
            if observe_flag == Some(ObserveOption::Register)
                && method == RequestType::Get
                && !resp.get_status().is_error()
            {
                obs.sequence = obs.sequence.wrapping_add(1);
                resp.message.set_observe_value(obs.sequence);
            }

            let bytes = match resp.message.to_bytes() {
                Ok(b) => b,
                Err(e) => {
                    log::error!("Failed to serialize response: {}", e);
                    return;
                }
            };
            log::debug!("Got response: {:?}", resp.message);

            match conn.send(&bytes).await {
                Ok(n) => log::debug!("Wrote {} bytes", n),
                Err(e) => log::error!("Error: {}", e),
            }
        }
        Err(e) => log::error!("Error: {}", e),
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

    loop {
        if let Ok((conn, socket_addr)) = listener.accept().await {
            log::info!("Got a connection from: {}", socket_addr);

            let mut router = router.clone();
            let config_clone = config.clone();
            let timeout = config_clone.timeout;

            let state = if let Some(dtls) = conn.as_any().downcast_ref::<DTLSConn>() {
                dtls.connection_state().await
            } else {
                log::error!("Unable to get state!");
                continue;
            };

            let identity = match extract_identity(state.identity_hint) {
                Some(id) => id,
                None => continue,
            };

            log::info!("PSK Identity: {}", identity);

            let cons = connections.clone();

            tokio::spawn(async move {
                let (tx, mut rx) = channel::<()>(1);

                if !manage_connection(&identity, socket_addr, tx, &cons).await {
                    return;
                }

                let (obs_tx, mut obs_rx) = channel::<ObserverValue>(10);
                let obs_tx = Arc::new(obs_tx);

                let mut obs = ObserveState::new();
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
                                    log::error!("Timeout! Err: {}", e);
                                    let _ = cons.lock().await.remove(&identity);
                                    break;
                                }
                            };

                            if let Ok(n) = recv {
                                let packet = match Packet::from_bytes(&b[..n]) {
                                    Ok(p) => p,
                                    Err(e) => {
                                        log::error!("Failed to parse packet: {}", e);
                                        continue;
                                    }
                                };
                                handle_request(
                                    packet, socket_addr, &identity, &mut router,
                                    conn.as_ref(), &obs_tx, &mut obs,
                                ).await;
                            }
                        }
                        _ = rx.recv() => {
                            log::info!("Terminating connection with: {}", socket_addr);
                            break;
                        }
                    }
                }

                cons.lock().await.remove(&identity);
                let _ = router.unregister_all(&identity).await;
                log::info!(
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
///         log::error!("Server error: {}", e);
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

    // Initialize client store with initial clients
    let mut store_map = HashMap::new();
    for (identity, key) in initial_clients.iter() {
        store_map.insert(
            identity.clone(),
            ClientEntry {
                key: key.clone(),
                metadata: ClientMetadata {
                    enabled: true,
                    ..Default::default()
                },
            },
        );
    }
    let client_store: ClientStore = Arc::new(RwLock::new(store_map));

    // Create client management channel
    let (cmd_sender, mut cmd_receiver) = mpsc::channel(config.client_command_buffer);
    let client_manager = ClientManager::new(cmd_sender);

    // Clone for the command processor
    let store_for_processor = Arc::clone(&client_store);

    // Spawn client command processor
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            process_client_command(cmd, &store_for_processor).await;
        }
    });

    // Create DTLS config with dynamic PSK callback
    let store_for_psk = Arc::clone(&client_store);
    let mut dtls_cfg = config.dtls_cfg.clone();

    // Set up PSK callback that uses our dynamic client store
    dtls_cfg.psk = Some(Arc::new(move |hint: &[u8]| -> Result<Vec<u8>, Error> {
        let hint_str = String::from_utf8(hint.to_vec()).map_err(|_| Error::ErrIdentityNoPsk)?;

        log::debug!("PSK callback for identity: {}", hint_str);

        // Use blocking read since we're in a sync context
        let store = store_for_psk.blocking_read();

        if let Some(entry) = store.get(&hint_str) {
            if entry.metadata.enabled {
                log::info!("PSK found for identity: {}", hint_str);
                Ok(entry.key.clone())
            } else {
                log::warn!("Client {} is disabled", hint_str);
                Err(Error::ErrIdentityNoPsk)
            }
        } else {
            log::warn!("PSK not found for identity: {}", hint_str);
            Err(Error::ErrIdentityNoPsk)
        }
    }));

    // Update the config with our enhanced DTLS config
    let mut final_config = config;
    final_config.dtls_cfg = dtls_cfg;

    // Return client manager and server future (don't spawn)
    let server_future = serve_basic(addr, final_config, router);

    Ok((client_manager, server_future))
}

/// Process a client command
async fn process_client_command(cmd: ClientCommand, store: &ClientStore) {
    match cmd {
        ClientCommand::AddClient {
            identity,
            key,
            metadata,
        } => {
            let mut store = store.write().await;
            let entry = ClientEntry {
                key,
                metadata: metadata.unwrap_or_else(|| ClientMetadata {
                    enabled: true,
                    ..Default::default()
                }),
            };
            store.insert(identity.clone(), entry);
            log::info!("Added client: {}", identity);
        }
        ClientCommand::RemoveClient { identity } => {
            let mut store = store.write().await;
            if store.remove(&identity).is_some() {
                log::info!("Removed client: {}", identity);
            } else {
                log::warn!("Client not found for removal: {}", identity);
            }
        }
        ClientCommand::UpdateKey { identity, key } => {
            let mut store = store.write().await;
            if let Some(entry) = store.get_mut(&identity) {
                entry.key = key;
                log::info!("Updated key for client: {}", identity);
            } else {
                log::warn!("Client not found for key update: {}", identity);
            }
        }
        ClientCommand::UpdateMetadata { identity, metadata } => {
            let mut store = store.write().await;
            if let Some(entry) = store.get_mut(&identity) {
                entry.metadata = metadata;
                log::info!("Updated metadata for client: {}", identity);
            } else {
                log::warn!("Client not found for metadata update: {}", identity);
            }
        }
        ClientCommand::SetClientEnabled { identity, enabled } => {
            let mut store = store.write().await;
            if let Some(entry) = store.get_mut(&identity) {
                entry.metadata.enabled = enabled;
                log::info!("Set client {} enabled: {}", identity, enabled);
            } else {
                log::warn!("Client not found for enable/disable: {}", identity);
            }
        }
        ClientCommand::ListClients { response } => {
            let store = store.read().await;
            let clients: Vec<String> = store.keys().cloned().collect();
            let _ = response.send(clients);
        }
    }
}

/// Create a client manager connected to an existing client store
///
/// This is useful when you want to manage clients from multiple places
/// or integrate with existing authentication systems.
pub fn create_client_manager(client_store: ClientStore, buffer_size: usize) -> ClientManager {
    let (cmd_sender, mut cmd_receiver) = mpsc::channel(buffer_size);

    // Spawn command processor
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            process_client_command(cmd, &client_store).await;
        }
    });

    ClientManager::new(cmd_sender)
}
