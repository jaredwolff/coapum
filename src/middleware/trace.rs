use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

use coap_lite::CoapResponse;
use tower::{Layer, Service};

use crate::{observer::ObserverRequest, router::CoapumRequest};

/// Tower layer that records a `tracing` span around every service call.
///
/// A new span is opened when `call` is invoked and closed when the response
/// future resolves. Structured fields record the path and response status.
///
/// Works for both the request-dispatch and observer-notification paths.
pub struct TraceLayer;

impl TraceLayer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TraceLayer {
    fn default() -> Self {
        Self
    }
}

impl<S> Layer<S> for TraceLayer {
    type Service = Trace<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Trace {
            inner: Arc::new(tokio::sync::Mutex::new(inner)),
        }
    }
}

/// Service produced by [`TraceLayer`].
#[derive(Clone)]
pub struct Trace<S> {
    inner: Arc<tokio::sync::Mutex<S>>,
}

impl<S, Addr> Service<CoapumRequest<Addr>> for Trace<S>
where
    S: Service<CoapumRequest<Addr>, Response = CoapResponse, Error = Infallible> + Send + 'static,
    S::Future: Send + 'static,
    Addr: std::fmt::Debug + Send + 'static,
{
    type Response = CoapResponse;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: CoapumRequest<Addr>) -> Self::Future {
        let path = req.get_path().clone();
        let method = *req.get_method();
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            let start = Instant::now();
            let span = tracing::info_span!("coap.request", path = %path, method = ?method);
            let resp = {
                let _enter = span.enter();
                let fut = inner.lock().await.call(req);
                fut.await.unwrap()
            };
            let status = *resp.get_status();
            tracing::info!(
                path = %path,
                status = ?status,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "coap.response"
            );
            Ok(resp)
        })
    }
}

impl<S, Addr> Service<ObserverRequest<Addr>> for Trace<S>
where
    S: Service<ObserverRequest<Addr>, Response = CoapResponse, Error = Infallible> + Send + 'static,
    S::Future: Send + 'static,
    Addr: std::fmt::Debug + Send + 'static,
{
    type Response = CoapResponse;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: ObserverRequest<Addr>) -> Self::Future {
        let path = req.path.clone();
        let inner = Arc::clone(&self.inner);
        Box::pin(async move {
            let start = Instant::now();
            let span = tracing::info_span!("coap.notification", path = %path);
            let resp = {
                let _enter = span.enter();
                let fut = inner.lock().await.call(req);
                fut.await.unwrap()
            };
            let status = *resp.get_status();
            tracing::info!(
                path = %path,
                status = ?status,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "coap.notification.response"
            );
            Ok(resp)
        })
    }
}
