//! Tests for RFC 7959 Block-wise Transfer integration
//!
//! Tests BlockHandler integration at the router level, verifying that
//! large responses are fragmented (Block2) and large requests are
//! reassembled (Block1) correctly.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use coap_lite::block_handler::BlockValue;
use coap_lite::{
    BlockHandler, BlockHandlerConfig, CoapOption, CoapRequest, MessageClass, Packet, RequestType,
    ResponseType,
};
use coapum::config::Config;
use coapum::extract::{Bytes, State};
use coapum::observer::memory::MemObserver;
use coapum::router::RouterBuilder;
use tower::Service;

fn test_addr() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 5683))
}

fn create_coap_request(method: RequestType, path: &str, mid: u16) -> CoapRequest<SocketAddr> {
    let mut packet = Packet::new();
    packet.header.code = MessageClass::Request(method);
    packet.header.message_id = mid;

    for segment in path.split('/').filter(|s| !s.is_empty()) {
        packet.add_option(CoapOption::UriPath, segment.as_bytes().to_vec());
    }

    CoapRequest::from_packet(packet, test_addr())
}

// -- Config tests --

#[test]
fn test_block_config_defaults() {
    let config = Config::default();
    assert_eq!(config.max_message_size, 1152);
    assert_eq!(config.block_cache_expiry, Duration::from_secs(120));
}

#[test]
fn test_block_config_setters() {
    let mut config = Config::default();
    config.set_max_message_size(2048);
    config.set_block_cache_expiry(Duration::from_secs(60));
    assert_eq!(config.max_message_size, 2048);
    assert_eq!(config.block_cache_expiry, Duration::from_secs(60));
}

// -- Block2 response fragmentation tests --

#[derive(Debug, Clone)]
struct TestState {
    large_payload: Vec<u8>,
}

impl AsRef<TestState> for TestState {
    fn as_ref(&self) -> &TestState {
        self
    }
}

async fn large_response_handler(State(state): State<TestState>) -> Bytes {
    Bytes(state.large_payload.clone())
}

async fn echo_handler(body: Bytes) -> Bytes {
    body
}

/// Test that BlockHandler fragments a large response into Block2 chunks,
/// and subsequent Block2 requests retrieve remaining fragments from cache.
#[tokio::test]
async fn test_block2_response_fragmentation() {
    let max_msg_size = 64;
    let payload = vec![0xABu8; 200]; // Larger than max_msg_size

    let state = TestState {
        large_payload: payload.clone(),
    };
    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state, observer)
        .get("/large", large_response_handler)
        .build();

    let mut block_handler = BlockHandler::new(BlockHandlerConfig {
        max_total_message_size: max_msg_size,
        cache_expiry_duration: Duration::from_secs(120),
    });

    // First request — no block option, triggers fragmentation via intercept_response
    let mut coap_req = create_coap_request(RequestType::Get, "/large", 1);

    // intercept_request should return false (no block option on initial request)
    assert!(!block_handler.intercept_request(&mut coap_req).unwrap());

    // Route through coapum router
    let coapum_req: coapum::router::CoapumRequest<SocketAddr> = coap_req.clone().into();
    let resp = router.call(coapum_req).await.unwrap();
    assert_eq!(*resp.get_status(), coapum::ResponseType::Content);
    assert_eq!(resp.message.payload.len(), 200);

    // Feed response through intercept_response for fragmentation
    let mut block_req = coap_req.clone();
    block_req.response = Some(resp);
    let intercepted = block_handler.intercept_response(&mut block_req).unwrap();
    assert!(intercepted, "Should fragment large response");

    let first_resp = block_req.response.unwrap();
    assert!(
        first_resp.message.payload.len() < 200,
        "First chunk should be smaller than full payload"
    );

    let first_block = first_resp
        .message
        .get_first_option_as::<BlockValue>(CoapOption::Block2)
        .expect("Should have Block2 option")
        .expect("Should parse Block2");
    assert!(first_block.more, "Should indicate more blocks");
    assert_eq!(first_block.num, 0);
    let block_size = first_block.size();

    // Collect all fragments
    let mut reassembled = first_resp.message.payload.clone();
    let mut block_num = 1u16;
    let mut last_mid = 1u16;

    loop {
        last_mid += 1;
        let next_block = BlockValue::new(block_num as usize, false, block_size).unwrap();
        let mut next_req = create_coap_request(RequestType::Get, "/large", last_mid);
        next_req
            .message
            .add_option_as::<BlockValue>(CoapOption::Block2, next_block);

        // intercept_request should serve from cache
        let handled = block_handler.intercept_request(&mut next_req).unwrap();
        assert!(handled, "Block2 follow-up should be served from cache");

        let resp = next_req.response.unwrap();
        reassembled.extend(&resp.message.payload);

        let block_val = resp
            .message
            .get_first_option_as::<BlockValue>(CoapOption::Block2)
            .unwrap()
            .unwrap();

        if !block_val.more {
            break;
        }
        block_num += 1;
        assert!(block_num < 100, "Too many blocks");
    }

    assert_eq!(
        reassembled, payload,
        "Reassembled payload should match original"
    );
}

/// Test that a small response (below max_message_size) is NOT fragmented.
#[tokio::test]
async fn test_block2_small_response_not_fragmented() {
    let max_msg_size = 1152;
    let payload = vec![0x42u8; 64]; // Well under threshold

    let state = TestState {
        large_payload: payload.clone(),
    };
    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state, observer)
        .get("/small", large_response_handler)
        .build();

    let mut block_handler = BlockHandler::new(BlockHandlerConfig {
        max_total_message_size: max_msg_size,
        cache_expiry_duration: Duration::from_secs(120),
    });

    let mut coap_req = create_coap_request(RequestType::Get, "/small", 1);
    assert!(!block_handler.intercept_request(&mut coap_req).unwrap());

    let coapum_req: coapum::router::CoapumRequest<SocketAddr> = coap_req.clone().into();
    let resp = router.call(coapum_req).await.unwrap();

    let mut block_req = coap_req.clone();
    block_req.response = Some(resp);
    let intercepted = block_handler.intercept_response(&mut block_req).unwrap();
    assert!(!intercepted, "Small response should not be fragmented");

    let resp = block_req.response.unwrap();
    assert_eq!(resp.message.payload, payload);
}

// -- Block1 request reassembly tests --

/// Test that BlockHandler rejects a large payload without Block1 option.
#[tokio::test]
async fn test_block1_rejects_oversized_without_block_option() {
    let max_msg_size = 64;

    let mut block_handler = BlockHandler::new(BlockHandlerConfig {
        max_total_message_size: max_msg_size,
        cache_expiry_duration: Duration::from_secs(120),
    });

    let mut coap_req = create_coap_request(RequestType::Put, "/upload", 1);
    coap_req.message.payload = vec![0xFFu8; 200];

    // intercept_request should handle it and set an error response
    let handled = block_handler.intercept_request(&mut coap_req).unwrap();
    assert!(handled, "Should reject oversized payload without Block1");

    let mut resp = coap_req.response.unwrap();
    assert_eq!(
        resp.message.header.code,
        MessageClass::Response(ResponseType::RequestEntityTooLarge)
    );

    // Should include Block1 option indicating expected block size
    let block1 = resp
        .message
        .get_first_option_as::<BlockValue>(CoapOption::Block1)
        .expect("Should have Block1 option")
        .expect("Should parse Block1");
    assert!(block1.more, "Should indicate more blocks expected");

    // RFC 7959 §2.9.1: serve.rs adds Size1 to 4.13 responses — verify the encoding
    // mirrors add_size1_option() in serve.rs
    let size_bytes = (max_msg_size as u32).to_be_bytes();
    let start = size_bytes.iter().position(|&b| b != 0).unwrap_or(3);
    resp.message
        .add_option(CoapOption::Size1, size_bytes[start..].to_vec());

    let size1_raw = resp
        .message
        .get_first_option(CoapOption::Size1)
        .expect("Should have Size1 option");
    let mut buf = [0u8; 4];
    buf[4 - size1_raw.len()..].copy_from_slice(size1_raw);
    let size1_val = u32::from_be_bytes(buf) as usize;
    assert_eq!(
        size1_val, max_msg_size,
        "Size1 should indicate max acceptable size"
    );
}

/// Test Block1 upload reassembly: send a large payload in chunks,
/// verify intermediate Continue responses, and final reassembled payload.
#[tokio::test]
async fn test_block1_upload_reassembly() {
    let block_size = 32;
    let full_payload = vec![0xCDu8; 100];

    let state = TestState {
        large_payload: vec![],
    };
    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state, observer)
        .post("/upload", echo_handler)
        .build();

    let mut block_handler = BlockHandler::new(BlockHandlerConfig {
        max_total_message_size: 256, // Allow reassembly up to 256 bytes
        cache_expiry_duration: Duration::from_secs(120),
    });

    let chunks: Vec<&[u8]> = full_payload.chunks(block_size).collect();
    let total_chunks = chunks.len();

    for (num, chunk) in chunks.iter().enumerate() {
        let has_more = num + 1 < total_chunks;
        let block = BlockValue::new(num, has_more, block_size).unwrap();

        let mut coap_req = create_coap_request(RequestType::Post, "/upload", (num + 1) as u16);
        coap_req.message.payload = chunk.to_vec();
        coap_req
            .message
            .add_option_as::<BlockValue>(CoapOption::Block1, block);

        let handled = block_handler.intercept_request(&mut coap_req).unwrap();

        if has_more {
            assert!(
                handled,
                "Intermediate Block1 should be handled by BlockHandler"
            );
            let resp = coap_req.response.unwrap();
            assert_eq!(
                resp.message.header.code,
                MessageClass::Response(ResponseType::Continue)
            );
        } else {
            assert!(!handled, "Final Block1 should pass through to handler");
            assert_eq!(
                coap_req.message.payload, full_payload,
                "Payload should be fully reassembled"
            );

            // Route the reassembled request through the router
            let coapum_req: coapum::router::CoapumRequest<SocketAddr> = coap_req.into();
            let resp = router.call(coapum_req).await.unwrap();
            assert_eq!(*resp.get_status(), coapum::ResponseType::Content);
            assert_eq!(resp.message.payload, full_payload);
        }
    }
}

/// Test that BlockHandler config maps correctly from coapum Config.
#[test]
fn test_config_to_block_handler_config() {
    let mut config = Config::default();
    config.set_max_message_size(2048);
    config.set_block_cache_expiry(Duration::from_secs(300));

    let block_config = BlockHandlerConfig {
        max_total_message_size: config.max_message_size,
        cache_expiry_duration: config.block_cache_expiry,
    };

    assert_eq!(block_config.max_total_message_size, 2048);
    assert_eq!(block_config.cache_expiry_duration, Duration::from_secs(300));
}

/// Test that multiple Block2 transfers for different paths don't interfere.
#[tokio::test]
async fn test_block2_independent_paths() {
    let max_msg_size = 64;

    let state = TestState {
        large_payload: vec![0xAAu8; 150],
    };
    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state, observer)
        .get("/path_a", large_response_handler)
        .get("/path_b", large_response_handler)
        .build();

    let mut block_handler = BlockHandler::new(BlockHandlerConfig {
        max_total_message_size: max_msg_size,
        cache_expiry_duration: Duration::from_secs(120),
    });

    // Start Block2 transfer for /path_a
    let mut req_a = create_coap_request(RequestType::Get, "/path_a", 1);
    block_handler.intercept_request(&mut req_a).unwrap();
    let coapum_req_a: coapum::router::CoapumRequest<SocketAddr> = req_a.clone().into();
    let resp_a = router.call(coapum_req_a).await.unwrap();
    let mut block_req_a = req_a.clone();
    block_req_a.response = Some(resp_a);
    block_handler.intercept_response(&mut block_req_a).unwrap();

    let first_a = block_req_a.response.unwrap();
    let block_a = first_a
        .message
        .get_first_option_as::<BlockValue>(CoapOption::Block2)
        .unwrap()
        .unwrap();
    assert!(block_a.more);

    // Start Block2 transfer for /path_b
    let mut req_b = create_coap_request(RequestType::Get, "/path_b", 2);
    block_handler.intercept_request(&mut req_b).unwrap();
    let coapum_req_b: coapum::router::CoapumRequest<SocketAddr> = req_b.clone().into();
    let resp_b = router.call(coapum_req_b).await.unwrap();
    let mut block_req_b = req_b.clone();
    block_req_b.response = Some(resp_b);
    block_handler.intercept_response(&mut block_req_b).unwrap();

    let first_b = block_req_b.response.unwrap();
    let block_b = first_b
        .message
        .get_first_option_as::<BlockValue>(CoapOption::Block2)
        .unwrap()
        .unwrap();
    assert!(block_b.more);

    // Fetch block 1 of /path_a — should still work (independent cache entries)
    let next_block = BlockValue::new(1, false, block_a.size()).unwrap();
    let mut next_req = create_coap_request(RequestType::Get, "/path_a", 3);
    next_req
        .message
        .add_option_as::<BlockValue>(CoapOption::Block2, next_block);
    let handled = block_handler.intercept_request(&mut next_req).unwrap();
    assert!(handled, "Should serve /path_a block 1 from cache");
    assert!(next_req.response.is_some());
}

// -- Integration test: Size1 in 4.13 through full DTLS server --

/// RFC 7959 §2.9.1: Server SHOULD include Size1 in 4.13 responses.
/// This test sends an oversized payload through a real DTLS connection
/// and verifies the 4.13 response contains both Block1 and Size1 options.
#[tokio::test]
async fn test_413_response_includes_size1_via_dtls() {
    use coapum::credential::resolver::MapResolver;
    use coapum::{MemoryCredentialStore, client::DtlsClient, serve};

    const PSK: &[u8] = b"block_test_key_456";
    const IDENTITY: &str = "block_test_client";
    const MAX_MSG_SIZE: usize = 64;

    // Start server with small max_message_size
    let listener = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let mut clients = HashMap::new();
    clients.insert(IDENTITY.to_string(), PSK.to_vec());

    let state = TestState {
        large_payload: vec![],
    };
    let observer = MemObserver::new();
    let router = RouterBuilder::new(state, observer)
        .put("/upload", echo_handler)
        .build();

    let server_config = Config {
        psk_identity_hint: Some(b"block_test_server".to_vec()),
        max_message_size: MAX_MSG_SIZE,
        timeout: 10,
        ..Default::default()
    };

    let credential_store = MemoryCredentialStore::from_clients(&clients);
    tokio::spawn(async move {
        let _ = serve::serve_with_credential_store(
            addr.to_string(),
            server_config,
            router,
            credential_store,
        )
        .await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect DTLS client
    let mut keys = HashMap::new();
    keys.insert(IDENTITY.to_string(), PSK.to_vec());
    let resolver = Arc::new(MapResolver::new(keys));
    let dtls_config = dimpl::Config::builder()
        .with_psk_resolver(resolver as Arc<dyn dimpl::PskResolver>)
        .with_psk_identity(IDENTITY.as_bytes().to_vec())
        .build()
        .expect("valid DTLS config");

    let mut client = DtlsClient::connect(&addr.to_string(), Arc::new(dtls_config))
        .await
        .expect("Failed to connect DTLS client");

    // Send oversized payload without Block1
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.message.header.message_id = 42;
    request.set_method(RequestType::Put);
    request.set_path("/upload");
    request.message.payload = vec![0xFFu8; 200]; // Larger than MAX_MSG_SIZE

    let request_bytes = request.message.to_bytes().unwrap();
    client.send(&request_bytes).await.unwrap();

    // Receive 4.13 response
    let data = tokio::time::timeout(Duration::from_secs(5), client.recv(Duration::from_secs(5)))
        .await
        .expect("Timed out waiting for response")
        .expect("Failed to receive response");

    let packet = Packet::from_bytes(&data).unwrap();
    assert_eq!(
        packet.header.code,
        MessageClass::Response(ResponseType::RequestEntityTooLarge),
        "Expected 4.13 for oversized payload"
    );

    // Verify Size1 option is present with the configured max size
    let size1_raw = packet
        .get_first_option(CoapOption::Size1)
        .expect("4.13 response should include Size1 option (RFC 7959 §2.9.1)");
    let mut buf = [0u8; 4];
    buf[4 - size1_raw.len()..].copy_from_slice(size1_raw);
    let size1_val = u32::from_be_bytes(buf) as usize;
    assert_eq!(
        size1_val, MAX_MSG_SIZE,
        "Size1 should indicate server's max acceptable payload size"
    );

    // Verify Block1 option is also present (from coap-lite BlockHandler)
    let block1 = packet
        .get_first_option_as::<BlockValue>(CoapOption::Block1)
        .expect("Should have Block1 option")
        .expect("Should parse Block1");
    assert!(block1.more, "Block1 should indicate more blocks expected");
}
