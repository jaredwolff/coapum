use coap_lite::{CoapResponse, RequestType};

use core::fmt::{self, Debug};
use std::fmt::Formatter;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{future::Future, sync::Mutex};

use super::{CoapumRequest, Handler, RouterError};

pub struct RouteHandler<S> {
    pub handler: Handler<S>,
    pub method: RequestType,
}

impl<S> Debug for RouteHandler<S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "RouteHandler {{ method: {:?} }}", self.method)
    }
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

pub fn unknown<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(CoapumRequest<SocketAddr>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
{
    RouteHandler {
        handler: Arc::new(
            move |req: CoapumRequest<SocketAddr>, state: Arc<Mutex<S>>| Box::pin(f(req, state)),
        ),
        method: RequestType::UnKnown,
    }
}
