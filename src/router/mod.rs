use coap_lite::{CoapRequest, CoapResponse, Packet};
use route_recognizer::Router;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tower::Service;

use self::wrapper::RouteHandler;

pub mod wrapper;

pub type RouterError = Box<(dyn std::error::Error + Send + Sync + 'static)>;

pub type Handler<S> = Arc<
    dyn Fn(
            CoapRequest<SocketAddr>,
            Arc<Mutex<S>>,
        ) -> Pin<Box<dyn Future<Output = Result<CoapResponse, RouterError>> + Send>>
        + Send
        + Sync,
>;

pub struct CoapRouter<S = ()> {
    inner: Router<RouteHandler<S>>,
    state: Arc<Mutex<S>>, // Shared state
}

impl CoapRouter<()> {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Default for CoapRouter<()> {
    fn default() -> Self {
        Self {
            inner: Router::new(),
            state: Arc::new(Mutex::new(())),
        }
    }
}

impl<S> CoapRouter<S>
where
    S: Send,
{
    pub fn new_with_state(state: S) -> Self {
        Self {
            inner: Router::new(),
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn add(&mut self, route: &str, handler: RouteHandler<S>) {
        self.inner.add(route, handler);
    }

    pub fn lookup(&self, request: &CoapRequest<SocketAddr>) -> Option<Handler<S>> {
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

impl<S> Service<CoapRequest<SocketAddr>> for CoapRouter<S>
where
    S: Send + Sync + 'static,
{
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
        let state = self.state.clone(); // Clone the state so it can be moved into the async block

        match self.lookup(&request) {
            Some(handler) => {
                log::debug!("Handler found for route: {:?}", request.get_path());

                // If a matching route handler is found, delegate the request to it
                Box::pin(async move { handler(request, state).await }) // Pass the state to the handler
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
