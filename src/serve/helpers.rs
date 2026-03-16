use std::net::SocketAddr;

use dimpl::{Dtls, Output};
use tokio::net::UdpSocket;

use coap_lite::{CoapOption, MessageClass, Packet, ResponseType};

use crate::reliability::ReliabilityState;

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

/// DTLS 1.2 CID content type (RFC 9146 §3).
pub(super) const TLS12_CID_CONTENT_TYPE: u8 = 25;

/// Generate a random Connection ID of the given length.
pub(super) fn generate_cid(len: usize) -> Vec<u8> {
    use rand::Rng;
    let mut cid = vec![0u8; len];
    rand::rng().fill_bytes(&mut cid);
    cid
}

/// Extract a Connection ID from a raw DTLS record header.
///
/// CID records (RFC 9146) use content type 25 with the CID placed at byte
/// offset 11 (after type + version + epoch + sequence number). Returns
/// `None` for non-CID records or packets too short to contain a full header.
pub(super) fn extract_cid(packet: &[u8], cid_len: usize) -> Option<&[u8]> {
    // Header layout for CID records:
    //   type(1) + version(2) + epoch(2) + seq(6) = 11, then CID, then length(2)
    let min_len = 11 + cid_len + 2;
    if packet.len() >= min_len && packet[0] == TLS12_CID_CONTENT_TYPE {
        Some(&packet[11..11 + cid_len])
    } else {
        None
    }
}

/// Drain all pending DTLS output packets and send them over the socket.
pub(super) async fn drain_packets(
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
pub(super) async fn send_response(
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
pub(super) fn add_size1_option(message: &mut Packet, max_message_size: usize) {
    let bytes = (max_message_size as u32).to_be_bytes();
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(3);
    message.add_option(CoapOption::Size1, bytes[start..].to_vec());
}

/// Send a block-transfer intercept response: add Size1 for 4.13, echo the
/// request token, set message ID, piggybacked ACK for CON, send, and cache
/// for deduplication.
#[allow(clippy::too_many_arguments)]
pub(super) async fn send_block_intercept_response(
    resp: &mut crate::CoapResponse,
    request_token: &[u8],
    msg_id: u16,
    is_confirmable: bool,
    max_message_size: usize,
    dtls: &mut Dtls,
    out_buf: &mut [u8],
    socket: &UdpSocket,
    socket_addr: SocketAddr,
    reliability: &mut ReliabilityState,
) {
    // RFC 7959 §2.9.1: Include Size1 in 4.13 to indicate max acceptable size
    if resp.message.header.code == MessageClass::Response(ResponseType::RequestEntityTooLarge) {
        add_size1_option(&mut resp.message, max_message_size);
    }
    // RFC 7252 §5.3.1: Echo request token in block transfer responses
    resp.message.set_token(request_token.to_vec());
    resp.message.header.message_id = msg_id;
    // RFC 7252 §5.2.1: Piggybacked ACK for CON block transfer responses
    if is_confirmable {
        resp.message
            .header
            .set_type(coap_lite::MessageType::Acknowledgement);
    }
    send_response(dtls, out_buf, socket, socket_addr, resp).await;
    // RFC 7252 §4.5: Cache response for deduplication
    if is_confirmable && let Ok(bytes) = resp.message.to_bytes() {
        reliability.record_response(msg_id, bytes);
    }
}
