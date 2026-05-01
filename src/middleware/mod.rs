//! Built-in tower middleware layers for coapum routers.
//!
//! All layers in this module satisfy the [`CoapService`](crate::CoapService) bound, meaning
//! they can be passed directly to [`RouterBuilder::layer`](crate::RouterBuilder::layer).
//!
//! ## `Error = Infallible` discipline
//!
//! Layers must never return `Err(_)`. Encode errors as a `CoapResponse` with a
//! non-success [`ResponseType`](crate::ResponseType):
//!
//! ```rust,no_run
//! use coapum::{CoapResponse, ResponseType};
//! use coap_lite::MessageClass;
//!
//! fn bad_request_response() -> CoapResponse {
//!     let mut resp = CoapResponse::new(&coap_lite::Packet::new()).unwrap();
//!     resp.message.header.code = MessageClass::Response(ResponseType::BadRequest);
//!     resp
//! }
//! ```
//!
//! When adapting a layer whose error type is not `Infallible`, wrap it with
//! `.map_err(...)` or a small adapter that converts the error to a `CoapResponse`.
//!
//! ## Which path classes each layer applies to
//!
//! | Layer | Request path | Notification path | Notes |
//! |---|---|---|---|
//! | [`MapResponseLayer`] | via `layer_request_only` | via `layer_notification_only` | closure is typed to one `Req` |
//! | [`TraceLayer`] | ✓ | ✓ | use `.layer()` for both |
//! | [`TimeoutLayer`] | ✓ | ✓ | use `.layer()` for both |
//!
//! `MapResponseLayer`'s closure takes `&Req` alongside `&mut CoapResponse`, so it
//! must be applied via
//! [`layer_request_only`](crate::RouterBuilder::layer_request_only) or
//! [`layer_notification_only`](crate::RouterBuilder::layer_notification_only) —
//! a single concrete closure cannot satisfy both request and notification `Req`
//! types simultaneously. `TraceLayer` and `TimeoutLayer` are generic over `Req`
//! and can be applied with `.layer()` to cover both paths.

pub mod map_response;
pub mod timeout;
pub mod trace;

pub use map_response::MapResponseLayer;
pub use timeout::TimeoutLayer;
pub use trace::TraceLayer;
