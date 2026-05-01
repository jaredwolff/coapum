//! Service trait alias for tower-layer composition.
//!
//! [`CoapService`] is the trait every value passed to [`bind_and_spawn`](crate::bind_and_spawn)
//! must implement. It bundles the two `tower::Service` impls coapum needs (the
//! request path and the observer-notification path) together with the standard
//! `Clone + Send + 'static` plumbing required by the serve loop.
//!
//! Consumers rarely name this trait directly: [`CoapRouter`](crate::router::CoapRouter)
//! and any [`tower::Layer`]-wrapped router that preserves the request/response
//! contract satisfy it via a blanket impl.
//!
//! ## The `Error = Infallible` discipline
//!
//! Both `Service` impls require `Error = Infallible`. The dispatch loop sends
//! whatever `CoapResponse` the service returns straight back over DTLS, so
//! there is no transport-level error channel separate from the response.
//! Layers that want to reject a request MUST encode the rejection as a
//! `CoapResponse` with a non-success [`ResponseType`](coap_lite::ResponseType)
//! (e.g., `BadRequest`, `ServiceUnavailable`), not as `Err(_)`.
//!
//! When wrapping a tower-http layer whose error type is not `Infallible`,
//! adapt it with `.map_err(...)` (or a small adapter layer) that converts the
//! error into an appropriate `CoapResponse`.

use std::convert::Infallible;
use std::net::SocketAddr;

use coap_lite::CoapResponse;
use tower::Service;

use crate::observer::ObserverRequest;
use crate::router::CoapumRequest;

mod sealed {
    pub trait Sealed {}
}

/// Marker trait for services that can be passed to
/// [`bind_and_spawn`](crate::bind_and_spawn).
///
/// Implemented automatically for any type that implements both
/// `Service<CoapumRequest<SocketAddr>>` and `Service<ObserverRequest<SocketAddr>>`
/// with `Response = CoapResponse`, `Error = Infallible`, plus
/// `Clone + Send + 'static` and `Send + 'static` futures.
pub trait CoapService:
    sealed::Sealed
    + Service<
        CoapumRequest<SocketAddr>,
        Response = CoapResponse,
        Error = Infallible,
        Future = <Self as CoapService>::RequestFuture,
    > + Service<
        ObserverRequest<SocketAddr>,
        Response = CoapResponse,
        Error = Infallible,
        Future = <Self as CoapService>::NotificationFuture,
    > + Clone
    + Send
    + 'static
{
    /// Future returned by the request-path `Service` impl.
    type RequestFuture: std::future::Future<Output = Result<CoapResponse, Infallible>>
        + Send
        + 'static;
    /// Future returned by the observer-notification `Service` impl.
    type NotificationFuture: std::future::Future<Output = Result<CoapResponse, Infallible>>
        + Send
        + 'static;
}

impl<T, ReqFut, NotFut> sealed::Sealed for T
where
    T: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = Infallible,
            Future = ReqFut,
        > + Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = Infallible,
            Future = NotFut,
        > + Clone
        + Send
        + 'static,
    ReqFut: std::future::Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    NotFut: std::future::Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
{
}

impl<T, ReqFut, NotFut> CoapService for T
where
    T: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = Infallible,
            Future = ReqFut,
        > + Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = Infallible,
            Future = NotFut,
        > + Clone
        + Send
        + 'static,
    ReqFut: std::future::Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    NotFut: std::future::Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
{
    type RequestFuture = ReqFut;
    type NotificationFuture = NotFut;
}
