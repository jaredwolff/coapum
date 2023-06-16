use coap_lite::{CoapRequest, CoapResponse, RequestType};

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use super::{Handler, RouterError};

pub struct RouteHandler {
    pub handler: Handler,
    pub method: RequestType,
}

pub fn get<F, Fut>(f: F) -> RouteHandler
where
    F: Fn(CoapRequest<SocketAddr>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
{
    RouteHandler {
        handler: Arc::new(move |req: CoapRequest<SocketAddr>| Box::pin(f(req))),
        method: RequestType::Get,
    }
}
