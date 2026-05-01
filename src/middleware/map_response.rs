use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use coap_lite::CoapResponse;
use tower::{Layer, Service};

/// Tower layer that applies a function to every response before returning it.
///
/// The closure receives an immutable reference to the original request and a
/// mutable reference to the outbound response. Because the closure is typed to a
/// specific request type, `MapResponseLayer` should be applied via
/// [`RouterBuilder::layer_request_only`](crate::RouterBuilder::layer_request_only),
/// [`RouterBuilder::layer_notification_only`](crate::RouterBuilder::layer_notification_only),
/// or the equivalent methods on
/// [`LayeredCoapRouter`](crate::LayeredCoapRouter).
///
/// # `Error = Infallible` discipline
///
/// The mapping function is synchronous and infallible. To reject a request, set
/// a non-success `ResponseType` on the response — do not return `Err(_)`.
///
/// # Example
///
/// ```rust,no_run
/// use std::net::SocketAddr;
/// use coapum::middleware::MapResponseLayer;
/// use coapum::{CoapumRequest, router::RouterBuilder, observer::memory::MemObserver, extract::StatusCode};
///
/// let router = RouterBuilder::new((), MemObserver::new())
///     .get("/data", || async { StatusCode::Content })
///     .layer_request_only(MapResponseLayer::new(
///         |_req: &CoapumRequest<SocketAddr>, resp: &mut coapum::CoapResponse| {
///             resp.message.set_content_format(coapum::ContentFormat::ApplicationJSON);
///         },
///     ));
/// ```
pub struct MapResponseLayer<F> {
    f: Arc<F>,
}

impl<F> MapResponseLayer<F> {
    /// Create a new layer applying `f` to every response.
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f) }
    }
}

impl<F> Clone for MapResponseLayer<F> {
    fn clone(&self) -> Self {
        Self {
            f: Arc::clone(&self.f),
        }
    }
}

impl<S, F> Layer<S> for MapResponseLayer<F> {
    type Service = MapResponse<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        MapResponse {
            inner,
            f: Arc::clone(&self.f),
        }
    }
}

/// Service produced by [`MapResponseLayer`].
pub struct MapResponse<S, F> {
    inner: S,
    f: Arc<F>,
}

impl<S, F> Clone for MapResponse<S, F>
where
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            f: Arc::clone(&self.f),
        }
    }
}

impl<S, F, Req> Service<Req> for MapResponse<S, F>
where
    S: Service<Req, Response = CoapResponse, Error = Infallible>,
    S::Future: Send + 'static,
    F: Fn(&Req, &mut CoapResponse) + Send + Sync + 'static,
    Req: Clone + Send + 'static,
{
    type Response = CoapResponse;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let f = Arc::clone(&self.f);
        let req_clone = req.clone();
        let fut = self.inner.call(req);
        Box::pin(async move {
            let mut resp = fut.await.unwrap();
            f(&req_clone, &mut resp);
            Ok(resp)
        })
    }
}
