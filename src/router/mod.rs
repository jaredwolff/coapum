use coap_lite::{CoapRequest, CoapResponse, Packet};
use route_recognizer::Router;

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use tower::Service;

use self::wrapper::RouteHandler;

pub mod wrapper;

pub type RouterError = Box<(dyn std::error::Error + Send + Sync + 'static)>;

pub type Handler = Arc<
    dyn Fn(
            CoapRequest<SocketAddr>,
        ) -> Pin<Box<dyn Future<Output = Result<CoapResponse, RouterError>> + Send>>
        + Send
        + Sync,
>;

pub struct CoapRouter {
    inner: Router<RouteHandler>,
}

impl CoapRouter {
    pub fn new() -> Self {
        CoapRouter {
            inner: Router::new(),
        }
    }

    pub fn add(&mut self, route: &str, handler: RouteHandler) {
        self.inner.add(route, handler);
    }

    pub fn lookup(&self, request: &CoapRequest<SocketAddr>) -> Option<Handler> {
        match self.inner.recognize(&request.get_path()) {
            Ok(matched) => {
                let handler = matched.handler();

                if handler.method == *request.get_method() {
                    Some(handler.handler.clone())
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }
}

impl Service<CoapRequest<SocketAddr>> for CoapRouter {
    type Response = CoapResponse;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        // Assume that the router is always ready.
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: CoapRequest<SocketAddr>) -> Self::Future {
        match self.lookup(&request) {
            Some(handler) => {
                log::debug!("Handler found for route: {:?}", request.get_path());

                // If a matching route handler is found, delegate the request to it
                Box::pin(async move { handler(request).await })
            }
            None => {
                log::error!(
                    "No handler found for method: {:#?} to: {:?}",
                    request.get_method(),
                    request.get_path()
                );

                // TODO: If no route handler is found, return a not found error
                let pkt = Packet::default();
                let response = CoapResponse::new(&pkt).unwrap();
                Box::pin(async move { Ok(response) })
            }
        }
    }
}
