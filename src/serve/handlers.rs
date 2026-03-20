use std::{fmt::Debug, net::SocketAddr};

use tower::Service;

use coap_lite::{
    CoapOption, CoapRequest, ContentFormat, MessageClass, MessageType, ObserveOption, Packet,
    RequestType, ResponseType,
};

use super::connection::SessionState;
use super::helpers::{DtlsIo, drain_packets, send_block_intercept_response, send_response};
use crate::{
    config::Config,
    observer::{Observer, ObserverValue, validate_observer_path},
    reliability::DedupResult,
    router::{CoapRouter, CoapumRequest, DeviceEvent},
};

/// Handle an observer notification: route, set RFC 7641 headers, and send.
pub(super) async fn handle_notification<O, S>(
    value: ObserverValue,
    router: &mut CoapRouter<O, S>,
    io: &mut DtlsIo,
    session: &mut SessionState,
) where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    tracing::trace!("Got notification: {:?}", value);

    let notification_path = value.path.clone();
    let notification_value = value.value.clone();
    let req = value.to_request(io.remote);

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
            if let Some(token) = session.obs.observer_tokens.get(&notification_path) {
                resp.message.set_token(token.clone());
            }

            // RFC 7641 §3.3: Set observe sequence number (24-bit per §3.4)
            session.obs.sequence = session.obs.sequence.wrapping_add(1) & 0x00FF_FFFF;
            resp.message.set_observe_value(session.obs.sequence);

            // Assign unique message ID for RST tracking
            let msg_id = session.obs.next_msg_id;
            session.obs.next_msg_id = session.obs.next_msg_id.wrapping_add(1);
            resp.message.header.message_id = msg_id;

            // RFC 7252 §4.2 / RFC 7641 §4.5: Use CON or NON based on route config
            let confirmable = router.is_confirmable_notify(&notification_path);
            if confirmable {
                resp.message.header.set_type(MessageType::Confirmable);
            } else {
                resp.message.header.set_type(MessageType::NonConfirmable);
            }

            session
                .obs
                .notification_msg_ids
                .insert(msg_id, notification_path);

            // Bound tracking map to prevent unbounded growth
            if session.obs.notification_msg_ids.len() > 256 {
                let cutoff = msg_id.wrapping_sub(128);
                session
                    .obs
                    .notification_msg_ids
                    .retain(|&id, _| id.wrapping_sub(cutoff) < 256);
            }

            tracing::trace!(
                "Sending notification (seq={}, con={}) to: {}",
                session.obs.sequence,
                confirmable,
                io.remote
            );

            // RFC 7959: Fragment large notification payloads using Block2
            let mut block_req = CoapRequest::from_packet(resp.message.clone(), io.remote);
            block_req.response = Some(resp);
            if let Err(e) = session.block_handler.intercept_response(&mut block_req) {
                tracing::error!("Block notification error: {}", e.message);
            }
            if let Some(ref resp) = block_req.response {
                send_response(io, resp).await;

                // Track for retransmission if CON
                if confirmable && let Ok(bytes) = resp.message.to_bytes() {
                    session.reliability.track_outgoing_con(msg_id, bytes);
                }
            }
        }
        Err(e) => tracing::error!("Error: {}", e),
    }
}

/// Handle an incoming CoAP request: block-wise transfer, observe management, routing, and response.
pub(super) async fn handle_request<O, S>(
    packet: Packet,
    io: &mut DtlsIo,
    session: &mut SessionState,
    router: &mut CoapRouter<O, S>,
    config: &Config,
) where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    let identity = session
        .identity
        .as_deref()
        .expect("identity guaranteed by caller");
    let socket_addr = io.remote;
    let msg_type = packet.header.get_type();
    let msg_id = packet.header.message_id;

    // RFC 7641 §3.2: RST deregisters observer + stops CON retransmission
    if msg_type == MessageType::Reset {
        if let Some(path) = session.obs.notification_msg_ids.remove(&msg_id) {
            tracing::info!("RST deregistration for '{}' path '{}'", identity, path);
            session.obs.observer_tokens.remove(&path);
            let _ = router.unregister_observer(identity, &path).await;
        }
        session.reliability.handle_rst(msg_id);
        return;
    }

    // RFC 7252 §4.2: ACK for a CON we sent — stop retransmitting
    if msg_type == MessageType::Acknowledgement {
        if session.reliability.handle_ack(msg_id) {
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
                if let Err(e) = io.dtls.send_application_data(&bytes) {
                    tracing::error!(error = %e, "dtls.send_failed");
                }
                drain_packets(io).await;
            }
            router.emit_device_event(DeviceEvent::Ping {
                device_id: identity.to_string(),
                addr: socket_addr,
            });
        } else {
            tracing::debug!(msg_id, "ignoring NON empty message");
        }
        return;
    }

    // RFC 7252 §4.5: Deduplication for incoming CON requests
    let is_confirmable = msg_type == MessageType::Confirmable;
    if is_confirmable {
        match session.reliability.check_dedup(msg_id) {
            DedupResult::Duplicate(cached_bytes) => {
                tracing::debug!(msg_id, "reliability.dedup_hit");
                if let Err(e) = io.dtls.send_application_data(&cached_bytes) {
                    tracing::error!(error = %e, "dtls.send_failed");
                }
                drain_packets(io).await;
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
                    session.reliability.record_response(msg_id, bytes.clone());
                }
                if let Err(e) = io.dtls.send_application_data(&bytes) {
                    tracing::error!(error = %e, "dtls.send_failed");
                }
                drain_packets(io).await;
            }
            return;
        }
    }

    // RFC 7252 §5.3.1: Save request token for echoing into the response
    let request_token = packet.get_token().to_vec();

    let mut coap_request = CoapRequest::from_packet(packet, socket_addr);

    // RFC 7959: Block1 reassembly / Block2 cache serving
    match session.block_handler.intercept_request(&mut coap_request) {
        Ok(true) => {
            // Block handler handled it (intermediate Block1 or Block2 cache hit)
            if let Some(ref mut resp) = coap_request.response {
                send_block_intercept_response(
                    resp,
                    &request_token,
                    msg_id,
                    is_confirmable,
                    config.max_message_size,
                    io,
                    &mut session.reliability,
                )
                .await;
            }
            return;
        }
        Err(e) => {
            tracing::error!("Block transfer error: {}", e.message);
            if let Some(ref mut resp) = coap_request.response {
                send_block_intercept_response(
                    resp,
                    &request_token,
                    msg_id,
                    is_confirmable,
                    config.max_message_size,
                    io,
                    &mut session.reliability,
                )
                .await;
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
                } else if router.observer_count(identity).await >= config.max_observers_per_device {
                    tracing::warn!(
                        "Observer registration rejected for '{}' on '{}': limit of {} exceeded",
                        identity,
                        normalized_path,
                        config.max_observers_per_device
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
                    session.obs.observer_tokens.remove(&normalized_path);
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
                    .register_observer(identity, normalized_path, session.obs_tx.clone())
                    .await
                {
                    tracing::error!(identity = %identity, path = %normalized_path, error = ?e, "observer.register.failed");
                } else {
                    tracing::info!(identity = %identity, path = %normalized_path, "observer.registered");
                    // RFC 7252 §5.3.1: Store token for future notifications
                    session
                        .obs
                        .observer_tokens
                        .insert(normalized_path.clone(), request_token);
                    session.obs.sequence = session.obs.sequence.wrapping_add(1) & 0x00FF_FFFF;
                    resp.message.set_observe_value(session.obs.sequence);
                }
            }

            // RFC 7959: Fragment large responses using Block2
            let mut block_req = CoapRequest::from_packet(packet_for_block2, socket_addr);
            block_req.response = Some(resp);
            if let Err(e) = session.block_handler.intercept_response(&mut block_req) {
                tracing::error!("Block transfer response error: {}", e.message);
            }

            if let Some(ref mut resp) = block_req.response {
                // RFC 7252 §5.2.1: Piggybacked ACK for Confirmable requests
                if is_confirmable {
                    resp.message.header.set_type(MessageType::Acknowledgement);
                }

                tracing::debug!("Got response: {:?}", resp.message);
                send_response(io, resp).await;

                // Cache serialized response for deduplication
                if is_confirmable && let Ok(bytes) = resp.message.to_bytes() {
                    session.reliability.record_response(msg_id, bytes);
                }
            }
        }
        Err(e) => tracing::error!("Error: {}", e),
    }
}
