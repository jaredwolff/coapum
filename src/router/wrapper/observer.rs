use coap_lite::{CoapResponse, RequestType};

use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::RouteHandler;
use super::{Request, RouterError};

pub fn get<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
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

pub fn put<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
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

pub fn post<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
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

pub fn delete<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
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

pub fn unknown<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, RouterError>> + Send + 'static,
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
