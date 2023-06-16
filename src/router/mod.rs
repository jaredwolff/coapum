use coap_lite::{CoapRequest, CoapResponse, Packet, RequestType};

use std::collections::HashMap;
use std::future::Future;
use std::hash::{Hash, Hasher};
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

#[derive(Eq, Hash, PartialEq, Debug)]
pub struct Route {
    path: String,
    method: RequestTypeWrapper,
}

#[derive(Debug)]
pub struct RequestTypeWrapper(pub RequestType);

impl RequestTypeWrapper {
    pub fn as_u8(&self) -> u8 {
        match self.0 {
            RequestType::Get => 0,
            RequestType::Post => 1,
            RequestType::Put => 2,
            RequestType::Delete => 3,
            RequestType::Fetch => 4,
            RequestType::Patch => 5,
            RequestType::IPatch => 6,
            RequestType::UnKnown => 7,
        }
    }
}

impl std::hash::Hash for RequestTypeWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_u8().hash(state);
    }
}

impl PartialEq for RequestTypeWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for RequestTypeWrapper {}

pub struct CoapRouter {
    routes: HashMap<Route, Handler>,
}

impl CoapRouter {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn add_route(&mut self, path: impl Into<String>, wrapper: RouteHandler) {
        // let (method, handler)

        let route = Route {
            path: path.into(),
            method: RequestTypeWrapper(wrapper.method),
        };

        self.routes.insert(route, wrapper.handler);
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
        let method = request.get_method();
        let path = request.get_path();

        let route = Route {
            path,
            method: RequestTypeWrapper(*method),
        };

        match self.routes.get(&route) {
            Some(handler) => {
                log::debug!("Handler found for route: {:?}", route);

                // Clone the Arc
                let handler = Arc::clone(handler);
                // If a matching route handler is found, delegate the request to it
                Box::pin(async move { handler(request).await })
            }
            None => {
                log::debug!("No handler found for route: {:?}", route);

                // TODO: If no route handler is found, return a not found error
                let pkt = Packet::default();
                let response = CoapResponse::new(&pkt).unwrap();
                Box::pin(async move { Ok(response) })
            }
        }
    }
}
