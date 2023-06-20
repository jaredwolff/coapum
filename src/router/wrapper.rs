use coap_lite::{CoapResponse, RequestType};

use core::fmt::{self, Debug};
use std::fmt::Formatter;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::{Handler, Request, RouterError};

#[derive(Clone)]
pub struct RouteHandler<S>
where
    S: Clone,
{
    pub handler: Handler<S>,
    pub method: RequestType,
}

impl<S> Debug for RouteHandler<S>
where
    S: Clone,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "RouteHandler {{ method: {:?} }}", self.method)
    }
}

pub fn get<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        method: RequestType::Get,
    }
}

pub fn put<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        method: RequestType::Put,
    }
}

pub fn post<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        method: RequestType::Post,
    }
}

pub fn delete<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        method: RequestType::Delete,
    }
}

pub fn unknown<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        method: RequestType::UnKnown,
    }
}
