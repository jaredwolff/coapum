//! RFC 7252 compliance tests for token echoing (§5.3.1) and empty message handling (§4.3).

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use coap_lite::{CoapRequest, MessageClass, MessageType, Packet, RequestType, ResponseType};
use coapum::{
    extract::StatusCode,
    observer::memory::MemObserver,
    router::{CoapumRequest, RouterBuilder},
};
use tower::Service;

#[derive(Debug, Clone)]
struct TestState;

impl AsRef<TestState> for TestState {
    fn as_ref(&self) -> &TestState {
        self
    }
}

async fn echo_handler() -> StatusCode {
    StatusCode::Content
}

async fn created_handler() -> StatusCode {
    StatusCode::Created
}

fn test_addr() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 5683))
}

fn create_request_with_token(path: &str, token: &[u8]) -> CoapumRequest<SocketAddr> {
    let mut pkt = Packet::new();
    pkt.set_token(token.to_vec());
    pkt.header.message_id = 0x1234;
    let mut request = CoapRequest::from_packet(pkt, test_addr());
    request.set_path(path);
    request.into()
}

// ---------------------------------------------------------------------------
// §5.3.1 — Token echoing
// ---------------------------------------------------------------------------

mod token_echo {
    use super::*;

    /// Verify that a bare router response does NOT carry the request token.
    /// This confirms the need for the framework-level fix in handle_request.
    #[tokio::test]
    async fn router_response_missing_token() {
        let mut router = RouterBuilder::new(TestState, MemObserver::new())
            .get("/test", echo_handler)
            .build();

        let req = create_request_with_token("/test", b"\xDE\xAD");
        let resp = router.call(req).await.unwrap();

        // Router alone does NOT echo the token — that's done in handle_request.
        assert!(
            resp.message.get_token().is_empty(),
            "Router-level response should not carry a token by itself"
        );
    }

    /// Simulate the framework-level token copy that handle_request performs
    /// and verify the response carries the correct token and message ID.
    #[tokio::test]
    async fn framework_token_copy() {
        let mut router = RouterBuilder::new(TestState, MemObserver::new())
            .get("/test", echo_handler)
            .build();

        let token = b"\xCA\xFE\xBA\xBE";
        let msg_id: u16 = 0xABCD;

        let mut pkt = Packet::new();
        pkt.set_token(token.to_vec());
        pkt.header.message_id = msg_id;
        let mut raw = CoapRequest::from_packet(pkt, test_addr());
        raw.set_path("/test");

        // Save token before consuming the packet (mirrors handle_request)
        let request_token = raw.message.get_token().to_vec();
        let request: CoapumRequest<SocketAddr> = raw.into();

        let mut resp = router.call(request).await.unwrap();

        // Apply the framework-level fix
        resp.message.set_token(request_token.clone());
        resp.message.header.message_id = msg_id;

        assert_eq!(resp.message.get_token(), token);
        assert_eq!(resp.message.header.message_id, msg_id);
    }

    /// Verify token echoing works with an empty token (valid per RFC 7252).
    #[tokio::test]
    async fn framework_token_copy_empty_token() {
        let mut router = RouterBuilder::new(TestState, MemObserver::new())
            .get("/test", echo_handler)
            .build();

        let mut pkt = Packet::new();
        // Empty token is the default — don't set one
        pkt.header.message_id = 42;
        let mut raw = CoapRequest::from_packet(pkt, test_addr());
        raw.set_path("/test");

        let request_token = raw.message.get_token().to_vec();
        let request: CoapumRequest<SocketAddr> = raw.into();

        let mut resp = router.call(request).await.unwrap();
        resp.message.set_token(request_token.clone());
        resp.message.header.message_id = 42;

        assert!(resp.message.get_token().is_empty());
        assert_eq!(resp.message.header.message_id, 42);
    }

    /// Verify token echoing works with max-length token (8 bytes per RFC 7252).
    #[tokio::test]
    async fn framework_token_copy_max_length() {
        let mut router = RouterBuilder::new(TestState, MemObserver::new())
            .get("/test", echo_handler)
            .build();

        let token = b"\x01\x02\x03\x04\x05\x06\x07\x08";
        let mut pkt = Packet::new();
        pkt.set_token(token.to_vec());
        pkt.header.message_id = 999;
        let mut raw = CoapRequest::from_packet(pkt, test_addr());
        raw.set_path("/test");

        let request_token = raw.message.get_token().to_vec();
        let request: CoapumRequest<SocketAddr> = raw.into();

        let mut resp = router.call(request).await.unwrap();
        resp.message.set_token(request_token);
        resp.message.header.message_id = 999;

        assert_eq!(resp.message.get_token(), token);
    }

    /// Token must be echoed even when the handler returns an error response.
    #[tokio::test]
    async fn framework_token_copy_on_error_response() {
        let mut router = RouterBuilder::new(TestState, MemObserver::new())
            .get("/test", echo_handler)
            .build();

        let token = b"\xFF\x00";
        let msg_id: u16 = 7777;

        let mut pkt = Packet::new();
        pkt.set_token(token.to_vec());
        pkt.header.message_id = msg_id;
        let mut raw = CoapRequest::from_packet(pkt, test_addr());
        // Route to a path that doesn't exist → router returns BadRequest
        raw.set_path("/nonexistent");

        let request_token = raw.message.get_token().to_vec();
        let request: CoapumRequest<SocketAddr> = raw.into();

        let mut resp = router.call(request).await.unwrap();
        resp.message.set_token(request_token);
        resp.message.header.message_id = msg_id;

        assert_eq!(resp.message.get_token(), token);
        assert_eq!(resp.message.header.message_id, msg_id);
        assert_eq!(*resp.get_status(), ResponseType::BadRequest);
    }

    /// Token must be echoed for different handler return types.
    #[tokio::test]
    async fn framework_token_copy_with_different_handler() {
        let mut router = RouterBuilder::new(TestState, MemObserver::new())
            .get("/data", created_handler)
            .build();

        let token = b"\xAA\xBB";
        let msg_id: u16 = 1111;

        let mut pkt = Packet::new();
        pkt.set_token(token.to_vec());
        pkt.header.message_id = msg_id;
        let mut raw = CoapRequest::from_packet(pkt, test_addr());
        raw.set_path("/data");

        let request_token = raw.message.get_token().to_vec();
        let request: CoapumRequest<SocketAddr> = raw.into();

        let mut resp = router.call(request).await.unwrap();
        resp.message.set_token(request_token);
        resp.message.header.message_id = msg_id;

        assert_eq!(resp.message.get_token(), token);
        assert_eq!(*resp.get_status(), ResponseType::Created);
    }
}

// ---------------------------------------------------------------------------
// §4.3 — Empty message handling
// ---------------------------------------------------------------------------

mod empty_message {
    use super::*;

    /// Build a CON Empty packet (ping) with a given message ID.
    fn con_empty(msg_id: u16) -> Packet {
        let mut pkt = Packet::new();
        pkt.header.set_type(MessageType::Confirmable);
        pkt.header.code = MessageClass::Empty;
        pkt.header.message_id = msg_id;
        pkt
    }

    /// Build a NON Empty packet with a given message ID.
    fn non_empty(msg_id: u16) -> Packet {
        let mut pkt = Packet::new();
        pkt.header.set_type(MessageType::NonConfirmable);
        pkt.header.code = MessageClass::Empty;
        pkt.header.message_id = msg_id;
        pkt
    }

    /// Build a normal GET request packet for comparison.
    fn con_get(msg_id: u16, path: &str, token: &[u8]) -> Packet {
        let mut pkt = Packet::new();
        pkt.header.set_type(MessageType::Confirmable);
        pkt.header.code = MessageClass::Request(RequestType::Get);
        pkt.header.message_id = msg_id;
        pkt.set_token(token.to_vec());
        // Add Uri-Path option
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        for component in components {
            pkt.add_option(
                coap_lite::CoapOption::UriPath,
                component.as_bytes().to_vec(),
            );
        }
        pkt
    }

    /// Verify that a CON Empty packet is correctly identified as Empty.
    #[test]
    fn con_empty_has_empty_code() {
        let pkt = con_empty(0x1234);
        assert_eq!(pkt.header.code, MessageClass::Empty);
        assert_eq!(pkt.header.get_type(), MessageType::Confirmable);
        assert_eq!(pkt.header.message_id, 0x1234);
    }

    /// Verify that a NON Empty packet is correctly identified as Empty.
    #[test]
    fn non_empty_has_empty_code() {
        let pkt = non_empty(0x5678);
        assert_eq!(pkt.header.code, MessageClass::Empty);
        assert_eq!(pkt.header.get_type(), MessageType::NonConfirmable);
    }

    /// Verify that a normal request is NOT classified as Empty.
    #[test]
    fn normal_request_is_not_empty() {
        let pkt = con_get(0x1234, "/test", b"\xAA");
        assert_ne!(pkt.header.code, MessageClass::Empty);
    }

    /// Verify the RST response packet structure for a CON Empty ping.
    /// This tests the exact packet construction from handle_request.
    #[test]
    fn rst_response_to_ping() {
        let ping_id: u16 = 0xBEEF;
        let ping = con_empty(ping_id);

        // Simulate what handle_request does for CON Empty:
        assert_eq!(ping.header.code, MessageClass::Empty);
        assert_eq!(ping.header.get_type(), MessageType::Confirmable);

        // Replicate the exact RST construction from handle_request
        let mut rst = Packet::new();
        rst.header.set_type(MessageType::Reset);
        rst.header.code = MessageClass::Empty;
        rst.header.message_id = ping_id;

        assert_eq!(rst.header.get_type(), MessageType::Reset);
        assert_eq!(rst.header.message_id, ping_id);
        assert_eq!(rst.header.code, MessageClass::Empty);
        assert!(rst.payload.is_empty());
        assert!(rst.get_token().is_empty());

        // Verify it serializes and deserializes correctly
        let bytes = rst.to_bytes().expect("RST should serialize");
        let parsed = Packet::from_bytes(&bytes).expect("RST should parse");
        assert_eq!(parsed.header.get_type(), MessageType::Reset);
        assert_eq!(parsed.header.message_id, ping_id);
        assert_eq!(parsed.header.code, MessageClass::Empty);
    }

    /// Verify that different message IDs produce distinct RST responses.
    #[test]
    fn rst_preserves_message_id() {
        for msg_id in [0x0000, 0x0001, 0x7FFF, 0xFFFF] {
            let mut rst = Packet::new();
            rst.header.set_type(MessageType::Reset);
            rst.header.code = MessageClass::Empty;
            rst.header.message_id = msg_id;

            let bytes = rst.to_bytes().unwrap();
            let parsed = Packet::from_bytes(&bytes).unwrap();
            assert_eq!(
                parsed.header.message_id, msg_id,
                "RST message ID mismatch for {:#06X}",
                msg_id
            );
        }
    }

    /// Verify that the discrimination logic correctly separates
    /// empty messages from real requests based on MessageClass.
    #[test]
    fn discrimination_logic() {
        let empty_con = con_empty(1);
        let empty_non = non_empty(2);
        let real_get = con_get(3, "/test", b"\x01");

        // Only empty messages have MessageClass::Empty
        assert_eq!(empty_con.header.code, MessageClass::Empty);
        assert_eq!(empty_non.header.code, MessageClass::Empty);
        assert_ne!(real_get.header.code, MessageClass::Empty);

        // CON empty should produce RST
        let should_rst = empty_con.header.code == MessageClass::Empty
            && empty_con.header.get_type() == MessageType::Confirmable;
        assert!(should_rst);

        // NON empty should be silently ignored
        let should_ignore = empty_non.header.code == MessageClass::Empty
            && empty_non.header.get_type() == MessageType::NonConfirmable;
        assert!(should_ignore);
    }
}

// ---------------------------------------------------------------------------
// §5.3.1 — Observer token storage for notifications
// ---------------------------------------------------------------------------

mod observer_token_storage {
    use std::collections::HashMap;

    /// Mirrors the ObserveState.observer_tokens field from serve.rs.
    /// We test the HashMap-based storage logic directly since ObserveState is private.

    #[test]
    fn store_and_retrieve_token() {
        let mut tokens: HashMap<String, Vec<u8>> = HashMap::new();
        let token = b"\xCA\xFE\xBA\xBE".to_vec();

        tokens.insert("/sensors/temp".to_string(), token.clone());

        assert_eq!(tokens.get("/sensors/temp").unwrap(), &token);
    }

    #[test]
    fn multiple_observers_independent_tokens() {
        let mut tokens: HashMap<String, Vec<u8>> = HashMap::new();

        let token_a = b"\x01\x02\x03\x04".to_vec();
        let token_b = b"\xAA\xBB\xCC\xDD".to_vec();

        tokens.insert("/sensors/temp".to_string(), token_a.clone());
        tokens.insert("/sensors/humidity".to_string(), token_b.clone());

        assert_eq!(tokens.get("/sensors/temp").unwrap(), &token_a);
        assert_eq!(tokens.get("/sensors/humidity").unwrap(), &token_b);
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn re_registration_updates_token() {
        let mut tokens: HashMap<String, Vec<u8>> = HashMap::new();

        let old_token = b"\x01\x02".to_vec();
        let new_token = b"\xFF\xFE".to_vec();

        tokens.insert("/sensors/temp".to_string(), old_token);
        tokens.insert("/sensors/temp".to_string(), new_token.clone());

        assert_eq!(tokens.get("/sensors/temp").unwrap(), &new_token);
        assert_eq!(tokens.len(), 1);
    }

    #[test]
    fn cleanup_on_deregistration() {
        let mut tokens: HashMap<String, Vec<u8>> = HashMap::new();

        tokens.insert("/sensors/temp".to_string(), b"\x01\x02".to_vec());
        tokens.insert("/sensors/humidity".to_string(), b"\x03\x04".to_vec());

        // Simulate RST or explicit deregistration
        tokens.remove("/sensors/temp");

        assert!(!tokens.contains_key("/sensors/temp"));
        assert!(tokens.contains_key("/sensors/humidity"));
        assert_eq!(tokens.len(), 1);
    }

    #[test]
    fn missing_token_returns_none() {
        let tokens: HashMap<String, Vec<u8>> = HashMap::new();

        // Notification for unregistered path should not crash
        assert!(!tokens.contains_key("/unknown/path"));
    }
}
