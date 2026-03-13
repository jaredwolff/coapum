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

use dimpl::{Dtls, Output};
use tokio::{
    net::UdpSocket,
    sync::{
        Mutex,
        mpsc::{self, Sender, channel},
    },
};
use tower::Service;

use coap_lite::{
    BlockHandler, BlockHandlerConfig, CoapOption, CoapRequest, ContentFormat, MessageClass,
    MessageType, ObserveOption, Packet, RequestType, ResponseType,
};

use crate::{
    config::Config,
    credential::{CredentialStore, memory::MemoryCredentialStore, resolver::CapturingResolver},
    observer::{Observer, ObserverValue, validate_observer_path},
    reliability::{DedupResult, ReliabilityState, RetransmitAction, RetransmitParams},
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

/// Extract and validate PSK identity from raw bytes.
///
/// Validates length, UTF-8 encoding, and sanitizes to safe characters only.
pub(crate) fn extract_identity(identity_hint: &[u8]) -> Option<String> {
    const MAX_IDENTITY_LENGTH: usize = 256;

    if identity_hint.len() > MAX_IDENTITY_LENGTH {
        tracing::error!(
            "Identity hint too long: {} bytes (max: {})",
            identity_hint.len(),
            MAX_IDENTITY_LENGTH
        );
        return None;
    }

    match std::str::from_utf8(identity_hint) {
        Ok(s) => {
            if s.is_empty() {
                tracing::error!("Identity hint is empty");
                return None;
            }

            // Allow all printable ASCII (0x21–0x7E) except path separators
            // that could cause issues if identities appear in paths or logs.
            if !s
                .chars()
                .all(|c| c.is_ascii_graphic() && c != '/' && c != '\\')
            {
                tracing::error!("Identity hint contains invalid characters");
                return None;
            }

            Some(s.to_string())
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
    min_reconnect_interval: Duration,
    max_reconnect_attempts: usize,
) -> bool {
    let mut guard = connections.lock().await;

    if let Some(old_conn) = guard.get(identity) {
        if old_conn.established_at.elapsed() < min_reconnect_interval {
            tracing::warn!(
                identity = %identity,
                addr = %socket_addr,
                interval_ms = old_conn.established_at.elapsed().as_millis() as u64,
                "connection.rejected.rate_limit"
            );
            return false;
        }

        if old_conn.reconnect_count as usize > max_reconnect_attempts {
            tracing::error!(
                identity = %identity,
                addr = %socket_addr,
                count = old_conn.reconnect_count,
                "connection.rejected.max_attempts"
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
        identity = %identity,
        addr = %socket_addr,
        "connection.established"
    );
    true
}

/// Drain all pending DTLS output packets and send them over the socket.
async fn drain_packets(
    dtls: &mut Dtls,
    out_buf: &mut [u8],
    socket: &UdpSocket,
    remote: SocketAddr,
) {
    loop {
        match dtls.poll_output(out_buf) {
            Output::Packet(p) => {
                if let Err(e) = socket.send_to(p, remote).await {
                    tracing::error!(addr = %remote, error = %e, "udp.send_failed");
                }
            }
            Output::Timeout(_) => break,
            _ => {} // Connected, PeerCert, KeyingMaterial, ApplicationData handled elsewhere
        }
    }
}

/// Send a CoAP response over a DTLS connection.
async fn send_response(
    dtls: &mut Dtls,
    out_buf: &mut [u8],
    socket: &UdpSocket,
    remote: SocketAddr,
    resp: &crate::CoapResponse,
) {
    match resp.message.to_bytes() {
        Ok(bytes) => {
            if let Err(e) = dtls.send_application_data(&bytes) {
                tracing::error!(error = %e, "dtls.send_failed");
                return;
            }
            drain_packets(dtls, out_buf, socket, remote).await;
        }
        Err(e) => tracing::error!("Failed to serialize response: {}", e),
    }
}

/// RFC 7959 §2.9.1: Add Size1 option to indicate max acceptable payload size.
fn add_size1_option(message: &mut Packet, max_message_size: usize) {
    let bytes = (max_message_size as u32).to_be_bytes();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(3);
    message.add_option(CoapOption::Size1, bytes[start..].to_vec());
}

/// Handle an observer notification: route, set RFC 7641 headers, and send.
#[allow(clippy::too_many_arguments)]
async fn handle_notification<O, S>(
    value: ObserverValue,
    router: &mut CoapRouter<O, S>,
    dtls: &mut Dtls,
    out_buf: &mut [u8],
    socket: &UdpSocket,
    remote: SocketAddr,
    obs: &mut ObserveState,
    block_handler: &mut BlockHandler<SocketAddr>,
    reliability: &mut ReliabilityState,
) where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    tracing::trace!("Got notification: {:?}", value);

    let notification_path = value.path.clone();
    let notification_value = value.value.clone();
    let req = value.to_request(remote);

    match router.call(req).await {
        Ok(mut resp) => {
            if *resp.get_status() == ResponseType::BadRequest {
                tracing::error!("Error: {:?}", resp.message);
                return;
            }

            resp.message.payload =
                if resp.message.get_content_format() == Some(ContentFormat::ApplicationCBOR) {
                    let mut buf = Vec::new();
                    ciborium::into_writer(&notification_value, &mut buf).ok();
                    buf
                } else {
                    serde_json::to_vec(&notification_value).unwrap_or_default()
                };

            // RFC 7252 §5.3.1: Echo the token from the original OBSERVE GET
            if let Some(token) = obs.observer_tokens.get(&notification_path) {
                resp.message.set_token(token.clone());
            }

            // RFC 7641 §3.3: Set observe sequence number (24-bit per §3.4)
            obs.sequence = obs.sequence.wrapping_add(1) & 0x00FF_FFFF;
            resp.message.set_observe_value(obs.sequence);

            // Assign unique message ID for RST tracking
            let msg_id = obs.next_msg_id;
            obs.next_msg_id = obs.next_msg_id.wrapping_add(1);
            resp.message.header.message_id = msg_id;

            // RFC 7252 §4.2 / RFC 7641 §4.5: Use CON or NON based on route config
            let confirmable = router.is_confirmable_notify(&notification_path);
            if confirmable {
                resp.message.header.set_type(MessageType::Confirmable);
            } else {
                resp.message.header.set_type(MessageType::NonConfirmable);
            }

            obs.notification_msg_ids.insert(msg_id, notification_path);

            // Bound tracking map to prevent unbounded growth
            if obs.notification_msg_ids.len() > 256 {
                let cutoff = msg_id.wrapping_sub(128);
                obs.notification_msg_ids
                    .retain(|&id, _| id.wrapping_sub(cutoff) < 256);
            }

            tracing::trace!(
                "Sending notification (seq={}, con={}) to: {}",
                obs.sequence,
                confirmable,
                remote
            );

            // RFC 7959: Fragment large notification payloads using Block2
            let mut block_req = CoapRequest::from_packet(resp.message.clone(), remote);
            block_req.response = Some(resp);
            if let Err(e) = block_handler.intercept_response(&mut block_req) {
                tracing::error!("Block notification error: {}", e.message);
            }
            if let Some(ref resp) = block_req.response {
                send_response(dtls, out_buf, socket, remote, resp).await;

                // Track for retransmission if CON
                if confirmable && let Ok(bytes) = resp.message.to_bytes() {
                    reliability.track_outgoing_con(msg_id, bytes);
                }
            }
        }
        Err(e) => tracing::error!("Error: {}", e),
    }
}

/// Handle an incoming CoAP request: block-wise transfer, observe management, routing, and response.
#[allow(clippy::too_many_arguments)]
async fn handle_request<O, S>(
    packet: Packet,
    socket_addr: SocketAddr,
    identity: &str,
    router: &mut CoapRouter<O, S>,
    dtls: &mut Dtls,
    out_buf: &mut [u8],
    socket: &UdpSocket,
    obs_tx: &Arc<Sender<ObserverValue>>,
    obs: &mut ObserveState,
    block_handler: &mut BlockHandler<SocketAddr>,
    max_message_size: usize,
    max_observers_per_device: usize,
    reliability: &mut ReliabilityState,
) where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    let msg_type = packet.header.get_type();
    let msg_id = packet.header.message_id;

    // RFC 7641 §3.2: RST deregisters observer + stops CON retransmission
    if msg_type == MessageType::Reset {
        if let Some(path) = obs.notification_msg_ids.remove(&msg_id) {
            tracing::info!("RST deregistration for '{}' path '{}'", identity, path);
            obs.observer_tokens.remove(&path);
            let _ = router.unregister_observer(identity, &path).await;
        }
        reliability.handle_rst(msg_id);
        return;
    }

    // RFC 7252 §4.2: ACK for a CON we sent — stop retransmitting
    if msg_type == MessageType::Acknowledgement {
        if reliability.handle_ack(msg_id) {
            tracing::debug!(msg_id, "reliability.ack_received");
        }
        return;
    }

    // RFC 7252 §4.3: Empty message handling (code 0.00)
    // CON Empty = ping → respond with RST; NON Empty = silently ignore
    if packet.header.code == MessageClass::Empty {
        if msg_type == MessageType::Confirmable {
            tracing::debug!(msg_id, "ping received, responding with RST");
            let mut rst = Packet::new();
            rst.header.set_type(MessageType::Reset);
            rst.header.code = MessageClass::Empty;
            rst.header.message_id = msg_id;
            if let Ok(bytes) = rst.to_bytes() {
                if let Err(e) = dtls.send_application_data(&bytes) {
                    tracing::error!(error = %e, "dtls.send_failed");
                }
                drain_packets(dtls, out_buf, socket, socket_addr).await;
            }
        } else {
            tracing::debug!(msg_id, "ignoring NON empty message");
        }
        return;
    }

    // RFC 7252 §4.5: Deduplication for incoming CON requests
    let is_confirmable = msg_type == MessageType::Confirmable;
    if is_confirmable {
        match reliability.check_dedup(msg_id) {
            DedupResult::Duplicate(cached_bytes) => {
                tracing::debug!(msg_id, "reliability.dedup_hit");
                if let Err(e) = dtls.send_application_data(&cached_bytes) {
                    tracing::error!(error = %e, "dtls.send_failed");
                }
                drain_packets(dtls, out_buf, socket, socket_addr).await;
                return;
            }
            DedupResult::NewMessage => {}
        }
    }

    // RFC 7252 §5.4.1: Reject requests with unrecognized critical options (4.02 Bad Option).
    // Critical options have odd option numbers. Options known to coap-lite are accepted;
    // only truly unknown critical options trigger rejection.
    for (&option_num, _) in packet.options() {
        if let CoapOption::Unknown(_) = CoapOption::from(option_num)
            && option_num % 2 == 1
        {
            tracing::warn!(
                option_num,
                "Rejecting request with unrecognized critical option"
            );
            let mut rst = Packet::new();
            rst.header.message_id = msg_id;
            rst.set_token(packet.get_token().to_vec());
            rst.header.code = MessageClass::Response(ResponseType::BadOption);
            if is_confirmable {
                rst.header.set_type(MessageType::Acknowledgement);
            }
            if let Ok(bytes) = rst.to_bytes() {
                if is_confirmable {
                    reliability.record_response(msg_id, bytes.clone());
                }
                if let Err(e) = dtls.send_application_data(&bytes) {
                    tracing::error!(error = %e, "dtls.send_failed");
                }
                drain_packets(dtls, out_buf, socket, socket_addr).await;
            }
            return;
        }
    }

    // RFC 7252 §5.3.1: Save request token for echoing into the response
    let request_token = packet.get_token().to_vec();

    let mut coap_request = CoapRequest::from_packet(packet, socket_addr);

    // RFC 7959: Block1 reassembly / Block2 cache serving
    match block_handler.intercept_request(&mut coap_request) {
        Ok(true) => {
            // Block handler handled it (intermediate Block1 or Block2 cache hit)
            if let Some(ref mut resp) = coap_request.response {
                // RFC 7959 §2.9.1: Include Size1 in 4.13 to indicate max acceptable size
                if resp.message.header.code
                    == MessageClass::Response(ResponseType::RequestEntityTooLarge)
                {
                    add_size1_option(&mut resp.message, max_message_size);
                }
                // RFC 7252 §5.3.1: Echo request token in block transfer responses
                resp.message.set_token(request_token.clone());
                resp.message.header.message_id = msg_id;
                // RFC 7252 §5.2.1: Piggybacked ACK for CON block transfer responses
                if is_confirmable {
                    resp.message.header.set_type(MessageType::Acknowledgement);
                }
                send_response(dtls, out_buf, socket, socket_addr, resp).await;
                // RFC 7252 §4.5: Cache response for deduplication
                if is_confirmable && let Ok(bytes) = resp.message.to_bytes() {
                    reliability.record_response(msg_id, bytes);
                }
            }
            return;
        }
        Err(e) => {
            tracing::error!("Block transfer error: {}", e.message);
            if let Some(ref mut resp) = coap_request.response {
                // RFC 7959 §2.9.1: Include Size1 in 4.13 to indicate max acceptable size
                if resp.message.header.code
                    == MessageClass::Response(ResponseType::RequestEntityTooLarge)
                {
                    add_size1_option(&mut resp.message, max_message_size);
                }
                resp.message.set_token(request_token.clone());
                resp.message.header.message_id = msg_id;
                // RFC 7252 §5.2.1: Piggybacked ACK for CON block transfer responses
                if is_confirmable {
                    resp.message.header.set_type(MessageType::Acknowledgement);
                }
                send_response(dtls, out_buf, socket, socket_addr, resp).await;
                // RFC 7252 §4.5: Cache response for deduplication
                if is_confirmable && let Ok(bytes) = resp.message.to_bytes() {
                    reliability.record_response(msg_id, bytes);
                }
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
        (Some(ObserveOption::Deregister), RequestType::Get) => {
            match validate_observer_path(path) {
                Ok(normalized_path) => {
                    obs.observer_tokens.remove(&normalized_path);
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
            // RFC 7252 §5.3.1: Echo the request token in the response
            resp.message.set_token(request_token.clone());
            resp.message.header.message_id = msg_id;

            // RFC 7641 §3.1: Register observer only after handler succeeds
            if let Some(ref normalized_path) = pending_observe
                && !resp.get_status().is_error()
            {
                if let Err(e) = router
                    .register_observer(identity, normalized_path, obs_tx.clone())
                    .await
                {
                    tracing::error!(identity = %identity, path = %normalized_path, error = ?e, "observer.register.failed");
                } else {
                    tracing::info!(identity = %identity, path = %normalized_path, "observer.registered");
                    // RFC 7252 §5.3.1: Store token for future notifications
                    obs.observer_tokens
                        .insert(normalized_path.clone(), request_token);
                    obs.sequence = obs.sequence.wrapping_add(1) & 0x00FF_FFFF;
                    resp.message.set_observe_value(obs.sequence);
                }
            }

            // RFC 7959: Fragment large responses using Block2
            let mut block_req = CoapRequest::from_packet(packet_for_block2, socket_addr);
            block_req.response = Some(resp);
            if let Err(e) = block_handler.intercept_response(&mut block_req) {
                tracing::error!("Block transfer response error: {}", e.message);
            }

            if let Some(ref mut resp) = block_req.response {
                // RFC 7252 §5.2.1: Piggybacked ACK for Confirmable requests
                if is_confirmable {
                    resp.message.header.set_type(MessageType::Acknowledgement);
                }

                tracing::debug!("Got response: {:?}", resp.message);
                send_response(dtls, out_buf, socket, socket_addr, resp).await;

                // Cache serialized response for deduplication
                if is_confirmable && let Ok(bytes) = resp.message.to_bytes() {
                    reliability.record_response(msg_id, bytes);
                }
            }
        }
        Err(e) => tracing::error!("Error: {}", e),
    }
}

/// Process DTLS outputs after handle_packet(), handling Connected and ApplicationData events.
///
/// Returns `false` if the connection should be terminated.
#[allow(clippy::too_many_arguments)]
async fn process_outputs<O, S>(
    dtls: &mut Dtls,
    out_buf: &mut [u8],
    socket: &UdpSocket,
    remote: SocketAddr,
    resolver: &CapturingResolver<impl CredentialStore>,
    connected: &mut bool,
    identity: &mut Option<String>,
    router: &mut CoapRouter<O, S>,
    obs_tx: &Arc<Sender<ObserverValue>>,
    obs: &mut ObserveState,
    block_handler: &mut BlockHandler<SocketAddr>,
    max_observers_per_device: usize,
    connections: &Mutex<HashMap<String, ConnectionInfo>>,
    disconnect_tx: Sender<()>,
    config: &Config,
    reliability: &mut ReliabilityState,
) -> bool
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    loop {
        match dtls.poll_output(out_buf) {
            Output::Packet(p) => {
                if let Err(e) = socket.send_to(p, remote).await {
                    tracing::error!(addr = %remote, error = %e, "udp.send_failed");
                }
            }
            Output::Connected => {
                tracing::debug!(addr = %remote, "dtls.connected");

                let raw_identity = match resolver.take_last_identity() {
                    Some(id) => id,
                    None => {
                        tracing::error!(addr = %remote, "dtls.no_identity");
                        return false;
                    }
                };

                let validated = match extract_identity(raw_identity.as_bytes()) {
                    Some(id) => id,
                    None => return false,
                };

                if !manage_connection(
                    &validated,
                    remote,
                    disconnect_tx.clone(),
                    connections,
                    config.min_reconnect_interval,
                    config.max_reconnect_attempts,
                )
                .await
                {
                    return false;
                }

                tracing::info!(identity = %validated, addr = %remote, "connection.accepted");
                *identity = Some(validated);
                *connected = true;
            }
            Output::ApplicationData(data) => {
                if let Some(id) = identity.as_ref() {
                    let packet = match Packet::from_bytes(data) {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::error!("Failed to parse packet: {}", e);
                            continue;
                        }
                    };
                    handle_request(
                        packet,
                        remote,
                        id,
                        router,
                        dtls,
                        out_buf,
                        socket,
                        obs_tx,
                        obs,
                        block_handler,
                        config.max_message_size,
                        max_observers_per_device,
                        reliability,
                    )
                    .await;
                }
            }
            Output::Timeout(_) => break,
            _ => {} // PeerCert, KeyingMaterial — not used for PSK
        }
    }
    true
}

/// Per-connection task. Each spawned task owns its own Dtls instance and
/// its own `CapturingResolver`, so identity capture is race-free.
#[allow(clippy::too_many_arguments)]
async fn connection_task<O, S, C>(
    remote: SocketAddr,
    mut packet_rx: mpsc::Receiver<Vec<u8>>,
    socket: Arc<UdpSocket>,
    credential_store: C,
    psk_identity_hint: Option<Vec<u8>>,
    mut router: CoapRouter<O, S>,
    config: Config,
    connections: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
    conn_count: Arc<AtomicUsize>,
    cleanup_tx: mpsc::Sender<SocketAddr>,
) where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
    C: CredentialStore,
{
    // Build per-connection resolver + dimpl config so identity capture is race-free
    let resolver = Arc::new(CapturingResolver::new(credential_store));
    let dimpl_config = Arc::new(
        dimpl::Config::builder()
            .with_psk_server(
                psk_identity_hint.clone(),
                resolver.clone() as Arc<dyn dimpl::PskResolver>,
            )
            .build()
            .expect("valid DTLS config"),
    );

    let mut dtls = Dtls::new_12_psk(dimpl_config, Instant::now());
    let mut out_buf = vec![0u8; 2048];
    let mut connected = false;
    let mut identity: Option<String> = None;

    let (obs_tx, mut obs_rx) = channel::<ObserverValue>(10);
    let obs_tx = Arc::new(obs_tx);
    let mut obs = ObserveState::new();
    let mut reliability = ReliabilityState::new(RetransmitParams::from_config(&config));
    let mut block_handler = BlockHandler::new(BlockHandlerConfig {
        max_total_message_size: config.max_message_size,
        cache_expiry_duration: config.block_cache_expiry,
    });

    let (disconnect_tx, mut disconnect_rx) = channel::<()>(1);
    let timeout_duration = Duration::from_secs(config.timeout);

    // One-shot session lifetime timer (DTLS 1.2 key wear-out mitigation).
    // Created once before the loop so it is NOT reset on activity.
    let session_deadline = config.max_session_lifetime.map(tokio::time::sleep);
    tokio::pin!(session_deadline);

    loop {
        // Compute next DTLS retransmit deadline
        let dtls_timeout = tokio::time::sleep(timeout_duration);
        tokio::pin!(dtls_timeout);

        tokio::select! {
            // Incoming DTLS packet from dispatch
            packet = packet_rx.recv() => {
                let Some(raw) = packet else {
                    // Channel closed — dispatch removed us
                    tracing::debug!(addr = %remote, "connection.channel_closed");
                    break;
                };

                if let Err(e) = dtls.handle_packet(&raw) {
                    tracing::error!(addr = %remote, error = %e, "dtls.packet_error");
                    break;
                }

                if !process_outputs(
                    &mut dtls, &mut out_buf, &socket, remote,
                    &resolver, &mut connected, &mut identity,
                    &mut router, &obs_tx, &mut obs, &mut block_handler,
                    config.max_observers_per_device,
                    &connections, disconnect_tx.clone(), &config,
                    &mut reliability,
                ).await {
                    break;
                }
            }

            // Observer notification
            Some(value) = obs_rx.recv(), if connected => {
                handle_notification(
                    value, &mut router, &mut dtls, &mut out_buf,
                    &socket, remote, &mut obs, &mut block_handler,
                    &mut reliability,
                ).await;
            }

            // Disconnect signal
            _ = disconnect_rx.recv() => {
                tracing::info!(addr = %remote, identity = ?identity, "connection.terminating");
                break;
            }

            // Idle timeout
            () = &mut dtls_timeout => {
                tracing::info!(addr = %remote, "connection.timeout");
                break;
            }

            // Session lifetime limit (DTLS 1.2 key wear-out mitigation)
            Some(()) = async {
                match session_deadline.as_mut().as_pin_mut() {
                    Some(f) => { f.await; Some(()) }
                    None => std::future::pending().await,
                }
            } => {
                tracing::info!(
                    addr = %remote,
                    identity = ?identity,
                    "connection.session_lifetime_exceeded"
                );
                break;
            }

            // RFC 7252 §4.2: CON retransmission timer
            () = async {
                match reliability.next_retransmit_deadline() {
                    Some(d) => tokio::time::sleep_until(d).await,
                    None => std::future::pending::<()>().await,
                }
            } => {
                for action in reliability.process_retransmits() {
                    match action {
                        RetransmitAction::Resend { msg_id, ref bytes } => {
                            tracing::debug!(msg_id, "reliability.retransmit");
                            if let Err(e) = dtls.send_application_data(bytes) {
                                tracing::error!(error = %e, "reliability.retransmit.send_failed");
                                continue;
                            }
                            drain_packets(&mut dtls, &mut out_buf, &socket, remote).await;
                        }
                        RetransmitAction::GiveUp { msg_id } => {
                            tracing::warn!(msg_id, "reliability.give_up");
                            if let Some(path) = obs.notification_msg_ids.remove(&msg_id)
                                && let Some(ref id) = identity
                            {
                                obs.observer_tokens.remove(&path);
                                let _ = router.unregister_observer(id, &path).await;
                                tracing::info!(identity = %id, path = %path, "reliability.observer_deregistered");
                            }
                        }
                    }
                }
            }
        }

        // Drive DTLS retransmit timers after every event
        if let Err(e) = dtls.handle_timeout(Instant::now()) {
            tracing::error!(addr = %remote, error = %e, "dtls.timeout_error");
            break;
        }
        drain_packets(&mut dtls, &mut out_buf, &socket, remote).await;
    }

    // Cleanup
    conn_count.fetch_sub(1, Ordering::Relaxed);
    if let Some(ref id) = identity {
        connections.lock().await.remove(id);
        let _ = router.unregister_device(id).await;
        tracing::info!(identity = %id, addr = %remote, "connection.terminated");
    }
    let _ = cleanup_tx.send(remote).await;
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
    let mut shutdown_rx = config.shutdown.clone();

    // Dispatch table: SocketAddr → per-connection packet sender
    let mut dispatch: HashMap<SocketAddr, mpsc::Sender<Vec<u8>>> = HashMap::new();

    // Cleanup channel: connection tasks notify dispatch when they exit
    let (cleanup_tx, mut cleanup_rx) = mpsc::channel::<SocketAddr>(64);

    let mut recv_buf = vec![0u8; config.buffer_size()];

    loop {
        // Drain completed connections
        while let Ok(remote) = cleanup_rx.try_recv() {
            dispatch.remove(&remote);
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

                if let Some(tx) = dispatch.get(&remote) {
                    // Fast path: known connection
                    let _ = tx.try_send(recv_buf[..n].to_vec());
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
                    let _ = tx.try_send(recv_buf[..n].to_vec());
                    dispatch.insert(remote, tx);

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
                        connection_task(
                            remote, rx, socket, store,
                            hint, router, config, connections,
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
            process_client_command(cmd, &store_for_processor, &disconnect_tx).await;
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
            process_client_command(cmd, &store_for_processor, &disconnect_tx).await;
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

/// Process a client command by delegating to a credential store.
async fn process_client_command<C: CredentialStore>(
    cmd: ClientCommand,
    store: &C,
    disconnect_tx: &mpsc::Sender<String>,
) {
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
        ClientCommand::DisconnectClient { identity } => {
            if let Err(e) = disconnect_tx.send(identity.clone()).await {
                tracing::error!("Failed to send disconnect for {}: {}", identity, e);
            }
        }
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

    // Create a no-op disconnect channel (standalone managers aren't wired to a server)
    let (disconnect_tx, _disconnect_rx) = mpsc::channel::<String>(1);

    // Spawn command processor
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            process_client_command(cmd, &credential_store, &disconnect_tx).await;
        }
    });

    ClientManager::new(cmd_sender)
}
