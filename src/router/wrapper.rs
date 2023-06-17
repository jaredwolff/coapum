use coap_lite::{CoapResponse, RequestType};

use std::net::SocketAddr;
use std::sync::Arc;
use std::{future::Future, sync::Mutex};

use super::{CoapumRequest, Handler, RouterError};

pub struct RouteHandler<S> {
    pub handler: Handler<S>,
    pub method: RequestType,
}

pub fn get<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(CoapumRequest<SocketAddr>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
{
    RouteHandler {
        handler: Arc::new(
            move |req: CoapumRequest<SocketAddr>, state: Arc<Mutex<S>>| Box::pin(f(req, state)),
        ),
        method: RequestType::Get,
    }
}

pub fn put<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(CoapumRequest<SocketAddr>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
{
    RouteHandler {
        handler: Arc::new(
            move |req: CoapumRequest<SocketAddr>, state: Arc<Mutex<S>>| Box::pin(f(req, state)),
        ),
        method: RequestType::Get,
    }
}

pub fn delete<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(CoapumRequest<SocketAddr>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
{
    RouteHandler {
        handler: Arc::new(
            move |req: CoapumRequest<SocketAddr>, state: Arc<Mutex<S>>| Box::pin(f(req, state)),
        ),
        method: RequestType::Delete,
    }
}
