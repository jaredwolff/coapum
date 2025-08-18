use std::{collections::HashMap, fmt::Debug, net::SocketAddr, sync::Arc, time::{Duration, Instant}};

use tokio::sync::{
    mpsc::{channel, Sender},
    Mutex,
};
use tower::Service;
use webrtc_dtls::{conn::DTLSConn, listener};
use webrtc_util::conn::Listener;

use coap_lite::{CoapRequest, ObserveOption, Packet, RequestType, ResponseType};

use crate::{
    config::Config,
    observer::{Observer, ObserverValue},
    router::{CoapRouter, CoapumRequest},
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
    let components: Vec<&str> = path.split('/')
        .filter(|s| !s.is_empty())
        .collect();

    // Security: Limit path depth to prevent resource exhaustion
    const MAX_PATH_DEPTH: usize = 10;
    if components.len() > MAX_PATH_DEPTH {
        return Err(PathValidationError::PathTooDeep);
    }

    // Security: Validate each path component for safe characters only
    for component in &components {
        if !component.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
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

pub async fn serve<O, S>(
    addr: String,
    config: Config,
    router: CoapRouter<O, S>,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Debug + Clone + Send + Sync + 'static, // The shared state needs to be Send and Sync to be shared across threads
    O: Observer + Send + Sync + 'static,
{
    let dtls_config = config.dtls_cfg.clone();
    let listener = Arc::new(
        listener::listen(addr.clone(), dtls_config)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
    );
    let connections: Arc<Mutex<HashMap<String, ConnectionInfo>>> = Arc::new(Mutex::new(HashMap::new()));

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
                log::error!("Identity hint too long: {} bytes (max: {})", state.identity_hint.len(), MAX_IDENTITY_LENGTH);
                continue;
            } else {
                match String::from_utf8(state.identity_hint) {
                    Ok(s) => {
                        // Sanitize identity to prevent injection attacks
                        let sanitized: String = s.chars()
                            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
                            .take(MAX_IDENTITY_LENGTH)
                            .collect();
                        
                        if sanitized.is_empty() {
                            log::error!("Identity hint contains no valid characters");
                            continue;
                        }
                        
                        sanitized
                    },
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
                                socket_addr, identity, old_conn.established_at.elapsed()
                            );
                            return; // Skip this connection attempt
                        }

                        // Security: Detect suspicious reconnection patterns
                        if old_conn.reconnect_count > MAX_RECONNECT_ATTEMPTS {
                            log::error!(
                                "Security: Too many reconnection attempts from {} for identity '{}' (count: {})",
                                socket_addr, identity, old_conn.reconnect_count
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
                    log::info!("Connection established for identity '{}' from {}", identity, socket_addr);
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

// #[cfg(test)]
// mod tests {
//     use crate::router::wrapper::get;

//     use super::*;
//     use coap_lite::{CoapRequest, CoapResponse, Packet, RequestType, ResponseType};
//     use std::net::SocketAddr;

//     #[tokio::test]
//     async fn test_serve() {
//         // Set up test data
//         let addr = "127.0.0.1:5683".to_string();
//         let config = Config::default();

//         let mut router = CoapRouter::new((), ());
//         router.add(
//             "test",
//             get(|_, _| async { Ok(CoapResponse::new(&Packet::new()).unwrap()) }),
//         );

//         let mut request = CoapRequest::new();
//         request.set_method(RequestType::Get);
//         request.set_path("/test");

//         let identity = vec![0x01, 0x02, 0x03];

//         let mut request: CoapumRequest<SocketAddr> = request.into();
//         request.identity = identity.clone();

//         // Call the serve function
//         let result = serve(addr, config, router.clone()).await;

//         // Check that the serve function returns Ok
//         assert!(result.is_ok());

//         // Call the router with a GET request
//         let response = router.call(request).await.unwrap();

//         // Check that the response has a Valid status
//         assert_eq!(*response.get_status(), ResponseType::Content);

//         // Check that the response message is empty
//         assert!(response.message.payload.is_empty());

//         // Call the router with a DELETE request
//         let mut request = CoapRequest::new();
//         request.set_method(RequestType::Delete);
//         request.set_path("/test");

//         let mut request: CoapumRequest<SocketAddr> = request.into();
//         request.identity = identity.clone();

//         let response = router.call(request).await.unwrap();

//         // Check that the response has a Valid status
//         assert_eq!(*response.get_status(), ResponseType::BadRequest);
//     }
// }
