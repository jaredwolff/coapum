//! Tests for manual Block2 control via `Block2Request` + `BlockedRaw`.
//!
//! These verify the load-bearing claims of the manual-Block2 API:
//!
//! * the route handler is invoked for every block (no caching),
//! * the application can cancel an in-flight transfer by returning an error
//!   response on the next block,
//! * `BlockHandler::intercept_response` does not fragment or cache when the
//!   response already carries a Block2 option.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use coap_lite::{
    BlockHandler, BlockHandlerConfig, CoapOption, CoapRequest, MessageClass, Packet, RequestType,
    ResponseType, block_handler::BlockValue,
};
use coapum::ContentFormat;
use coapum::extract::{Block2Request, BlockedRaw, State};
use coapum::observer::memory::MemObserver;
use coapum::router::RouterBuilder;
use tokio::sync::Mutex;
use tower::Service;

const ADDR: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 5683));

#[derive(Clone, Debug)]
struct TestState {
    payload: Arc<Vec<u8>>,
    invocations: Arc<AtomicUsize>,
    cancel_after: Arc<Mutex<Option<u16>>>,
}

impl AsRef<TestState> for TestState {
    fn as_ref(&self) -> &TestState {
        self
    }
}

/// Per-block handler: returns the slice of state.payload for the requested
/// block, optionally cancelling with Forbidden once block_num >= cancel_after.
async fn manual_block_handler(
    Block2Request(req): Block2Request,
    State(state): State<TestState>,
) -> BlockedRaw {
    state.invocations.fetch_add(1, Ordering::SeqCst);

    let size = req.size();

    if let Some(cutoff) = *state.cancel_after.lock().await
        && req.num >= cutoff
    {
        return BlockedRaw::aborted(ResponseType::Forbidden, req.num, size);
    }

    let offset = usize::from(req.num) * size;
    let total = state.payload.len();

    if offset >= total {
        return BlockedRaw::aborted(ResponseType::BadOption, req.num, size);
    }

    let end = (offset + size).min(total);
    let chunk = state.payload[offset..end].to_vec();
    let more = end < total;
    let block = BlockValue::new(usize::from(req.num), more, size).unwrap();

    BlockedRaw::new(chunk, Some(ContentFormat::ApplicationOctetStream), block)
}

fn make_request(num: u16, size: usize, mid: u16) -> CoapRequest<SocketAddr> {
    let mut packet = Packet::new();
    packet.header.code = MessageClass::Request(RequestType::Get);
    packet.header.message_id = mid;
    packet.add_option(CoapOption::UriPath, b"firmware".to_vec());
    let bv = BlockValue::new(usize::from(num), false, size).unwrap();
    packet.add_option_as(CoapOption::Block2, bv);
    CoapRequest::from_packet(packet, ADDR)
}

#[tokio::test]
async fn handler_invoked_per_block() {
    let block_size = 64;
    let total = 200; // 4 blocks: 64, 64, 64, 8
    let payload: Vec<u8> = (0..total).map(|i| (i & 0xff) as u8).collect();

    let state = TestState {
        payload: Arc::new(payload.clone()),
        invocations: Arc::new(AtomicUsize::new(0)),
        cancel_after: Arc::new(Mutex::new(None)),
    };

    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state.clone(), observer)
        .get("/firmware", manual_block_handler)
        .build();

    let mut block_handler: BlockHandler<SocketAddr> = BlockHandler::new(BlockHandlerConfig {
        max_total_message_size: 1152,
        cache_expiry_duration: Duration::from_secs(120),
    });

    let mut reassembled: Vec<u8> = Vec::new();
    let mut num = 0u16;
    let mut mid = 1u16;

    loop {
        let mut coap_req = make_request(num, block_size, mid);
        let intercepted = block_handler.intercept_request(&mut coap_req).unwrap();
        assert!(
            !intercepted,
            "BlockHandler must defer to the application for manual Block2 (num={num})"
        );

        let coapum_req: coapum::router::CoapumRequest<SocketAddr> = coap_req.clone().into();
        let resp = router.call(coapum_req).await.unwrap();

        let mut block_req = coap_req.clone();
        block_req.response = Some(resp);
        let fragmented = block_handler.intercept_response(&mut block_req).unwrap();
        assert!(
            !fragmented,
            "BlockHandler must NOT fragment when Block2 is already set (num={num})"
        );

        let resp = block_req.response.unwrap();
        assert_eq!(*resp.get_status(), ResponseType::Content);
        reassembled.extend(&resp.message.payload);

        let block = resp
            .message
            .get_first_option_as::<BlockValue>(CoapOption::Block2)
            .expect("Block2 must be set on every BlockedRaw response")
            .expect("Block2 must parse");
        assert_eq!(block.num, num);

        if !block.more {
            break;
        }
        num += 1;
        mid += 1;
        assert!(num < 100, "runaway");
    }

    assert_eq!(reassembled, payload);
    // 4 blocks → handler must have been invoked 4 times.
    assert_eq!(state.invocations.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn cancel_via_error_response_aborts_transfer() {
    let block_size = 64;
    let total = 256; // 4 full blocks
    let payload: Vec<u8> = (0..total).map(|i| (i & 0xff) as u8).collect();

    let state = TestState {
        payload: Arc::new(payload),
        invocations: Arc::new(AtomicUsize::new(0)),
        // Cancel once block 2 is requested.
        cancel_after: Arc::new(Mutex::new(Some(2))),
    };

    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state.clone(), observer)
        .get("/firmware", manual_block_handler)
        .build();

    let mut block_handler: BlockHandler<SocketAddr> = BlockHandler::new(BlockHandlerConfig {
        max_total_message_size: 1152,
        cache_expiry_duration: Duration::from_secs(120),
    });

    // Blocks 0 and 1 succeed (handler runs, payload returned).
    for num in 0u16..=1 {
        let mid = num + 1;
        let mut coap_req = make_request(num, block_size, mid);
        let intercepted = block_handler.intercept_request(&mut coap_req).unwrap();
        assert!(!intercepted);

        let coapum_req: coapum::router::CoapumRequest<SocketAddr> = coap_req.clone().into();
        let resp = router.call(coapum_req).await.unwrap();
        let mut block_req = coap_req.clone();
        block_req.response = Some(resp);
        let fragmented = block_handler.intercept_response(&mut block_req).unwrap();
        assert!(!fragmented);

        let resp = block_req.response.unwrap();
        assert_eq!(*resp.get_status(), ResponseType::Content);
    }

    // Block 2: handler returns BlockedRaw::aborted(Forbidden) — transfer cancelled.
    // Block2 is set on the abort response so intercept_response stays a no-op.
    let mut coap_req = make_request(2, block_size, 3);
    let intercepted = block_handler.intercept_request(&mut coap_req).unwrap();
    assert!(!intercepted);

    let coapum_req: coapum::router::CoapumRequest<SocketAddr> = coap_req.clone().into();
    let resp = router.call(coapum_req).await.unwrap();

    let mut block_req = coap_req.clone();
    block_req.response = Some(resp);
    let fragmented = block_handler.intercept_response(&mut block_req).unwrap();
    assert!(
        !fragmented,
        "intercept_response must bypass when abort sets Block2"
    );

    let resp = block_req.response.unwrap();
    assert_eq!(*resp.get_status(), ResponseType::Forbidden);
    let block = resp
        .message
        .get_first_option_as::<BlockValue>(CoapOption::Block2)
        .unwrap()
        .unwrap();
    assert_eq!(block.num, 2);
    assert!(!block.more);

    // Handler ran for blocks 0, 1, 2 — the cancel decision is made inside it.
    assert_eq!(state.invocations.load(Ordering::SeqCst), 3);
}
