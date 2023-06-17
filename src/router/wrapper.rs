use coap_lite::{CoapRequest, CoapResponse, RequestType};

use std::net::SocketAddr;
use std::sync::Arc;
use std::{future::Future, sync::Mutex};

use super::{Handler, RouterError};

pub struct RouteHandler<S> {
    pub handler: Handler<S>,
    pub method: RequestType,
}

pub fn get<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(CoapRequest<SocketAddr>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
{
    RouteHandler {
        handler: Arc::new(move |req: CoapRequest<SocketAddr>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        method: RequestType::Get,
    }
}

pub fn put<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(CoapRequest<SocketAddr>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
{
    RouteHandler {
        handler: Arc::new(move |req: CoapRequest<SocketAddr>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        method: RequestType::Get,
    }
}
