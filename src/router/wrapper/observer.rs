use coap_lite::{CoapResponse, RequestType};

use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::RouteHandler;
use super::{Request, RouterError};

pub fn get<S, F, Fut>(f: F, o: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        observe_handler: Some(Arc::new(
            move |req: Box<dyn Request>, state: Arc<Mutex<S>>| Box::pin(o(req, state)),
        )),
        method: RequestType::Get,
    }
}

pub fn put<S, F, Fut>(f: F, o: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        observe_handler: Some(Arc::new(
            move |req: Box<dyn Request>, state: Arc<Mutex<S>>| Box::pin(o(req, state)),
        )),
        method: RequestType::Put,
    }
}

pub fn post<S, F, Fut>(f: F, o: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        observe_handler: Some(Arc::new(
            move |req: Box<dyn Request>, state: Arc<Mutex<S>>| Box::pin(o(req, state)),
        )),
        method: RequestType::Post,
    }
}

pub fn delete<S, F, Fut>(f: F, o: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        observe_handler: Some(Arc::new(
            move |req: Box<dyn Request>, state: Arc<Mutex<S>>| Box::pin(o(req, state)),
        )),
        method: RequestType::Delete,
    }
}

pub fn unknown<S, F, Fut>(f: F, o: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    S: Clone,
{
    RouteHandler {
        handler: Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
            Box::pin(f(req, state))
        }),
        observe_handler: Some(Arc::new(
            move |req: Box<dyn Request>, state: Arc<Mutex<S>>| Box::pin(o(req, state)),
        )),
        method: RequestType::UnKnown,
    }
}
