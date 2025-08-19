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
use webrtc_util::conn::Listener;

use coap_lite::{CoapRequest, ObserveOption, Packet, RequestType, ResponseType};

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

            // Get PSK Identity and use it as the Client's ID
            // Security: Validate identity hint size and content to prevent buffer overflow
            const MAX_IDENTITY_LENGTH: usize = 256;

            let identity: String = if state.identity_hint.len() > MAX_IDENTITY_LENGTH {
                log::error!(
                    "Identity hint too long: {} bytes (max: {})",
                    state.identity_hint.len(),
                    MAX_IDENTITY_LENGTH
                );
                continue;
            } else {
                match String::from_utf8(state.identity_hint) {
                    Ok(s) => {
                        // Sanitize identity to prevent injection attacks
                        let sanitized: String = s
                            .chars()
                            .filter(|c| {
                                c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.'
                            })
                            .take(MAX_IDENTITY_LENGTH)
                            .collect();

                        if sanitized.is_empty() {
                            log::error!("Identity hint contains no valid characters");
                            continue;
                        }

                        sanitized
                    }
                    Err(e) => {
                        log::error!("Invalid UTF-8 in identity hint: {}", e);
                        continue;
                    }
                }
            };

            log::info!("PSK Identity: {}", identity);

            let cons = connections.clone();

            tokio::spawn(async move {
                let (tx, mut rx) = channel::<()>(1);

                // Observers
                let (obs_tx, mut obs_rx) = channel::<ObserverValue>(10);
                let obs_tx = Arc::new(obs_tx);

                // Security: Validate connection and implement rate limiting
                {
                    let mut connections_guard = cons.lock().await;

                    // Check for existing connection and implement security measures
                    if let Some(old_conn) = connections_guard.get(&identity) {
                        // Security: Rate limit reconnections to prevent abuse
                        if old_conn.established_at.elapsed() < MIN_RECONNECT_INTERVAL {
                            log::warn!(
                                "Rate limited: Rapid reconnection attempt from {} for identity '{}' (interval: {:?})",
                                socket_addr,
                                identity,
                                old_conn.established_at.elapsed()
                            );
                            return; // Skip this connection attempt
                        }

                        // Security: Detect suspicious reconnection patterns
                        if old_conn.reconnect_count > MAX_RECONNECT_ATTEMPTS {
                            log::error!(
                                "Security: Too many reconnection attempts from {} for identity '{}' (count: {})",
                                socket_addr,
                                identity,
                                old_conn.reconnect_count
                            );
                            return; // Block this identity
                        }

                        // Signal the old connection to terminate
                        let _ = old_conn.sender.send(()).await;
                    }

                    // Insert the new connection with tracking info
                    let conn_info = ConnectionInfo {
                        sender: tx,
                        established_at: Instant::now(),
                        source_addr: socket_addr,
                        reconnect_count: connections_guard
                            .get(&identity)
                            .map(|c| c.reconnect_count + 1)
                            .unwrap_or(0),
                    };

                    connections_guard.insert(identity.clone(), conn_info);
                    log::info!(
                        "Connection established for identity '{}' from {}",
                        identity,
                        socket_addr
                    );
                }

                // Buffer
                let mut b = vec![0u8; config_clone.buffer_size()];

                loop {
                    tokio::select! {
                        // Handling observer..
                        notify = obs_rx.recv() => {
                            if let Some(value) = notify {

                                log::info!("Got notification: {:?}", value);

                                // Convert to request
                                let req = value.to_request(socket_addr);

                                // Formulate request from Value (i.e. create JSON payload)
                                match router.call(req).await{
                                    Ok(resp)=> {

                                        // Check to make sure we don't send error messages since this is server internal
                                        if *resp.get_status() == ResponseType::BadRequest {
                                            log::error!("Error: {:?}", resp.message);
                                            continue;
                                        }

                                        // Then send..
                                        log::info!("Sending data to: {}", socket_addr);
                                        match resp.message.to_bytes() {
                                            Ok(bytes) => match conn.send(&bytes).await {
                                            Ok(n) => log::debug!("Wrote {} notification bytes", n),
                                            Err(e) => {
                                                log::error!("Error: {}", e);
                                            }
                                        },
                                            Err(e) => log::error!("Failed to serialize response: {}", e),
                                        }
                                    }
                                    Err(e) => log::error!("Error: {}", e)
                                };
                            }
                        }
                        recv = tokio::time::timeout(Duration::from_secs(timeout), conn.recv(&mut b)) => {

                            // Check for timeout
                            let recv = match recv {
                                Ok(r) => r,
                                Err(e) => {
                                    log::error!("Timeout! Err: {}", e);

                                    // Since we timed out remove:
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
                                let request = CoapRequest::from_packet(packet, socket_addr);

                                // Convert to wrapper
                                let mut request: CoapumRequest<SocketAddr> = request.into();
                                request.identity = identity.clone();

                                // Get path
                                let path = request.get_path();
                                let observe_flag = *request.get_observe_flag();
                                let method = *request.get_method();

                                // Handle observations
                                match (observe_flag, method) {
                                    (Some(ObserveOption::Register), RequestType::Get) => {
                                        // register - ensure path starts with / for consistency with routing
                                        // Security: Validate path to prevent injection attacks
                                        match validate_observer_path(path) {
                                            Ok(normalized_path) => {
                                                if let Err(e) = router.register_observer(&identity, &normalized_path, obs_tx.clone()).await {
                                                    log::error!("Failed to register observer: {:?}", e);
                                                }
                                            }
                                            Err(e) => {
                                                log::error!("Invalid observer path '{}' from {}: {}", path, socket_addr, e);
                                                // Send error response for invalid path
                                                continue;
                                            }
                                        }
                                    }
                                    (Some(ObserveOption::Deregister), RequestType::Delete) => {
                                        // Security: Validate path to prevent injection attacks
                                        match validate_observer_path(path) {
                                            Ok(normalized_path) => {
                                                if let Err(e) = router.unregister_observer(&identity, &normalized_path).await {
                                                    log::error!("Failed to unregister observer: {:?}", e);
                                                }
                                            }
                                            Err(e) => {
                                                log::error!("Invalid observer path '{}' from {}: {}", path, socket_addr, e);
                                                continue;
                                            }
                                        }
                                    }
                                    _ => {}
                                };

                                // Call the service
                                match router.call(request).await
                                {
                                    Ok(resp) => {
                                      // Get the response
                                      let bytes = match resp.message.to_bytes() {
                                          Ok(b) => b,
                                          Err(e) => {
                                              log::error!("Failed to serialize response: {}", e);
                                              continue;
                                          }
                                      };
                                      log::debug!("Got response: {:?}", resp.message);

                                      // Write it back
                                      match conn.send(&bytes).await {
                                          Ok(n) => log::debug!("Wrote {} bytes", n),
                                          Err(e) => {
                                              log::error!("Error: {}", e);
                                          }
                                      };
                                    }
                                    Err(e)=> {
                                        log::error!("Error: {}", e);
                                    }
                                }
                            }
                        }
                        _ = rx.recv() => {
                            log::info!("Terminating connection with: {}", socket_addr);
                            break;
                        }
                    }
                }

                // Clean up connection from the map
                {
                    cons.lock().await.remove(&identity);
                }

                // Clean up all observer subscriptions for this device
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
