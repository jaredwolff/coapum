//! Manual Block2 (RFC 7959) handling for routes that need per-block control.
//!
//! By default, coapum delegates Block2 fragmentation to coap-lite's
//! [`BlockHandler`](coap_lite::block_handler::BlockHandler): the route handler
//! is invoked once with a full response, which is then chopped into blocks and
//! cached on the session. Subsequent block requests are served from that cache
//! and **never reach the handler**.
//!
//! That default is wrong when the application needs to:
//!
//! * re-authorize on every block of a long transfer,
//! * stream a resource that may be invalidated mid-flight,
//! * cancel a transfer in response to external state (quota, revocation, etc.),
//! * or compute each block lazily without buffering the whole resource.
//!
//! [`Block2Request`] + [`BlockedRaw`] together opt a single route out of the
//! cache: the extractor reports which block the client asked for, and the
//! response type sets the `Block2` option explicitly so coap-lite's
//! `intercept_response` no-ops (see coap-lite's `block_handler/mod.rs`: when a
//! response already carries `Block2`, the handler assumes the application is
//! managing fragmentation and skips both fragmentation and caching).
//!
//! ## Cancellation
//!
//! There is no explicit "cancel" message in RFC 7959. Cancellation works in
//! three forms, all of which this API supports without extra plumbing:
//!
//! 1. **Client cancels** — the client simply stops requesting more blocks. The
//!    server holds no per-transfer state (no cache), so there is nothing to
//!    clean up.
//! 2. **Server cancels mid-transfer** — the handler returns a [`BlockedRaw`]
//!    with `status` set to a 4.xx/5.xx code and `block.more = false`. The
//!    client sees the error and aborts. Use `BlockedRaw::aborted(...)` for
//!    this case.
//! 3. **External invalidation** — because the handler runs for every block, it
//!    can re-check authorization, resource validity, rate limits, etc. on each
//!    invocation and abort by returning a `BlockedRaw::aborted(...)`.
//!
//! ### Why cancel via `BlockedRaw` and not bare `StatusCode`
//!
//! coap-lite's `BlockHandler::intercept_response` decides whether to fragment
//! based on the *last requested* Block2 value, not on the response status.
//! Returning a small `Forbidden` response with no Block2 option after the
//! client requested e.g. block 2 makes coap-lite attempt to slice the tiny
//! error payload as if it were the cached body, which produces a malformed
//! response. Setting Block2 on the cancel response (via `BlockedRaw::aborted`)
//! makes coap-lite bypass `intercept_response` entirely.
//!
//! ## Example
//!
//! ```no_run
//! use coapum::{BlockValue, ResponseType};
//! use coapum::extract::{Block2Request, BlockedRaw};
//!
//! async fn serve_firmware(Block2Request(req): Block2Request) -> BlockedRaw {
//!     let firmware: &[u8] = b"...";
//!     let size = req.size();
//!     let offset = usize::from(req.num) * size;
//!
//!     // External cancel: resource has been revoked since the last block.
//!     if false /* check_revoked() */ {
//!         return BlockedRaw::aborted(ResponseType::Forbidden, req.num, size);
//!     }
//!
//!     if offset >= firmware.len() {
//!         return BlockedRaw::aborted(ResponseType::BadOption, req.num, size);
//!     }
//!
//!     let end = (offset + size).min(firmware.len());
//!     let chunk = firmware[offset..end].to_vec();
//!     let more = end < firmware.len();
//!     let block = BlockValue::new(req.num as usize, more, size).unwrap();
//!
//!     BlockedRaw::new(chunk, None, block)
//! }
//! ```

use async_trait::async_trait;
use coap_lite::{CoapOption, ContentFormat, Packet, ResponseType, block_handler::BlockValue};
use std::{convert::Infallible, fmt, net::SocketAddr};

use super::{FromRequest, IntoResponse, ResponseError};
use crate::router::CoapumRequest;

/// Default Block2 size exponent used when the client did not include a Block2
/// option. `6` encodes a 1024-byte block, which fits inside coapum's default
/// `max_message_size` of 1152 bytes (RFC 7252).
const DEFAULT_BLOCK2_SIZE_EXPONENT: u8 = 6;

/// Extracts the client's requested Block2 value.
///
/// If the request omits the Block2 option (RFC 7959 §2.2), this synthesizes
/// `BlockValue { num: 0, more: false, size_exponent: 6 }` so handlers can
/// always treat the request as block-wise. If the option is present but
/// malformed, the same default is used.
pub struct Block2Request(pub BlockValue);

impl fmt::Debug for Block2Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Block2Request").field(&self.0).finish()
    }
}

impl Clone for Block2Request {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl std::ops::Deref for Block2Request {
    type Target = BlockValue;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<S> FromRequest<S> for Block2Request {
    type Rejection = Infallible;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let bv = req
            .message
            .get_first_option_as::<BlockValue>(CoapOption::Block2)
            .and_then(|x| x.ok())
            .unwrap_or(BlockValue {
                num: 0,
                more: false,
                size_exponent: DEFAULT_BLOCK2_SIZE_EXPONENT,
            });
        Ok(Self(bv))
    }
}

/// Response that carries an explicit Block2 option, bypassing coap-lite's
/// auto-fragmentation and per-session response cache.
///
/// Use this when the application needs per-block re-invocation of the handler
/// (see the module-level docs for cancellation and the rationale).
///
/// `payload` should contain exactly the bytes for the block identified by
/// `block.num` at `block.size()`. Set `block.more = true` if more blocks
/// remain, `false` on the final (possibly short) block.
///
/// `status` is `Content` (2.05) for normal blocks. For mid-flight cancel,
/// construct via [`BlockedRaw::aborted`].
pub struct BlockedRaw {
    pub payload: Vec<u8>,
    pub content_format: Option<ContentFormat>,
    pub block: BlockValue,
    pub status: ResponseType,
}

impl BlockedRaw {
    /// Build a normal (2.05 Content) block response.
    pub fn new(payload: Vec<u8>, content_format: Option<ContentFormat>, block: BlockValue) -> Self {
        Self {
            payload,
            content_format,
            block,
            status: ResponseType::Content,
        }
    }

    /// Abort an in-flight Block2 transfer with an error status.
    ///
    /// The returned response carries the requested block number with
    /// `more = false` (so coap-lite's `BlockHandler::intercept_response`
    /// bypasses fragmentation/caching) and an empty payload.
    ///
    /// `block_num` should be the `num` from the [`Block2Request`] the handler
    /// was invoked with; `block_size` should match the negotiated size.
    pub fn aborted(status: ResponseType, block_num: u16, block_size: usize) -> Self {
        let block = BlockValue {
            num: block_num,
            more: false,
            size_exponent: BlockValue::new(0, false, block_size)
                .map(|b| b.size_exponent)
                .unwrap_or(DEFAULT_BLOCK2_SIZE_EXPONENT),
        };
        Self {
            payload: Vec::new(),
            content_format: None,
            block,
            status,
        }
    }
}

impl fmt::Debug for BlockedRaw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockedRaw")
            .field("payload_len", &self.payload.len())
            .field("content_format", &self.content_format)
            .field("block", &self.block)
            .field("status", &self.status)
            .finish()
    }
}

impl IntoResponse for BlockedRaw {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        let packet = Packet::new();
        let mut response = crate::CoapResponse::new(&packet).ok_or_else(|| {
            ResponseError::InvalidResponse("Failed to create response".to_string())
        })?;

        response.message.payload = self.payload;
        if let Some(cf) = self.content_format {
            response.message.set_content_format(cf);
        }
        // Setting Block2 on the outgoing response is what disables coap-lite's
        // auto-fragmentation: BlockHandler::intercept_response no-ops when the
        // response already has a Block2 option.
        response
            .message
            .add_option_as(CoapOption::Block2, self.block);
        response.set_status(self.status);
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coap_lite::{
        BlockHandler, BlockHandlerConfig, CoapRequest, MessageClass, RequestType,
        block_handler::BlockValue,
    };
    use std::time::Duration;

    fn make_request(block2: Option<BlockValue>) -> CoapumRequest<SocketAddr> {
        let mut packet = Packet::new();
        packet.header.code = MessageClass::Request(RequestType::Get);
        if let Some(bv) = block2 {
            packet.add_option_as(CoapOption::Block2, bv);
        }
        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();
        let coap_req = CoapRequest::from_packet(packet, addr);
        let mut req: CoapumRequest<SocketAddr> = coap_req.into();
        req.identity = "test-identity".to_string();
        req
    }

    #[tokio::test]
    async fn extracts_default_when_block2_absent() {
        let req = make_request(None);
        let Block2Request(bv) = Block2Request::from_request(&req, &()).await.unwrap();
        assert_eq!(bv.num, 0);
        assert!(!bv.more);
        assert_eq!(bv.size_exponent, DEFAULT_BLOCK2_SIZE_EXPONENT);
        assert_eq!(bv.size(), 1024);
    }

    #[tokio::test]
    async fn extracts_present_block2() {
        let bv = BlockValue::new(2, true, 64).unwrap();
        let req = make_request(Some(bv.clone()));
        let Block2Request(got) = Block2Request::from_request(&req, &()).await.unwrap();
        assert_eq!(got, bv);
    }

    #[test]
    fn blocked_raw_sets_block2_option() {
        let block = BlockValue::new(3, true, 128).unwrap();
        let resp = BlockedRaw::new(
            vec![0xab; 128],
            Some(ContentFormat::ApplicationOctetStream),
            block.clone(),
        )
        .into_response()
        .unwrap();

        assert_eq!(*resp.get_status(), ResponseType::Content);
        assert_eq!(resp.message.payload.len(), 128);
        assert_eq!(
            resp.message.get_content_format(),
            Some(ContentFormat::ApplicationOctetStream)
        );
        let got = resp
            .message
            .get_first_option_as::<BlockValue>(CoapOption::Block2)
            .unwrap()
            .unwrap();
        assert_eq!(got, block);
    }

    #[test]
    fn aborted_carries_status_and_block2() {
        let resp = BlockedRaw::aborted(ResponseType::Forbidden, 5, 64)
            .into_response()
            .unwrap();
        assert_eq!(*resp.get_status(), ResponseType::Forbidden);
        assert!(resp.message.payload.is_empty());
        let block = resp
            .message
            .get_first_option_as::<BlockValue>(CoapOption::Block2)
            .unwrap()
            .unwrap();
        assert_eq!(block.num, 5);
        assert!(!block.more);
        assert_eq!(block.size(), 64);
    }

    /// The load-bearing claim: when the response already has Block2 set,
    /// coap-lite's BlockHandler.intercept_response returns Ok(false) and does
    /// not populate its cache. This means subsequent block requests will
    /// reach the handler again instead of being served from cache.
    #[test]
    fn intercept_response_bypassed_when_block2_set() {
        let mut handler: BlockHandler<SocketAddr> = BlockHandler::new(BlockHandlerConfig {
            max_total_message_size: 1152,
            cache_expiry_duration: Duration::from_secs(120),
        });

        let addr: SocketAddr = "127.0.0.1:5683".parse().unwrap();

        // First exchange: client requests block 0.
        let mut req_packet = Packet::new();
        req_packet.header.code = MessageClass::Request(RequestType::Get);
        req_packet.add_option_as(CoapOption::Block2, BlockValue::new(0, false, 64).unwrap());
        let mut coap_req = CoapRequest::from_packet(req_packet, addr);

        // No cache yet — intercept_request must let it through.
        let intercepted = handler.intercept_request(&mut coap_req).unwrap();
        assert!(!intercepted);

        // Handler returns BlockedRaw-style response (Block2 set explicitly).
        let response = BlockedRaw::new(vec![0u8; 64], None, BlockValue::new(0, true, 64).unwrap())
            .into_response()
            .unwrap();
        coap_req.response = Some(response);

        // intercept_response must report no fragmentation and not cache.
        let fragmented = handler.intercept_response(&mut coap_req).unwrap();
        assert!(!fragmented);

        // Second exchange: client requests block 1. Because nothing was
        // cached, intercept_request must again return false so the handler
        // can produce block 1.
        let mut req_packet = Packet::new();
        req_packet.header.code = MessageClass::Request(RequestType::Get);
        req_packet.add_option_as(CoapOption::Block2, BlockValue::new(1, false, 64).unwrap());
        let mut coap_req = CoapRequest::from_packet(req_packet, addr);

        let intercepted = handler.intercept_request(&mut coap_req).unwrap();
        assert!(
            !intercepted,
            "with no cached response, BlockHandler must defer to the application"
        );
    }
}
