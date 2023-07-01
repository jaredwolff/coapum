use coap_lite::{CoapResponse, RequestType};

use core::fmt::{self, Debug};
use std::future::Future;
use std::sync::Arc;
use std::{fmt::Formatter, hash::Hasher};

use tokio::sync::Mutex;

use super::{Handler, Request, RouterError};

pub mod observer;

#[derive(Clone, Copy, Debug)]
pub struct RequestTypeWrapper(RequestType);

impl std::hash::Hash for RequestTypeWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.0 {
            RequestType::Get => 0u8.hash(state),
            RequestType::Post => 0u8.hash(state),
            RequestType::Put => 0u8.hash(state),
            RequestType::Delete => 0u8.hash(state),
            RequestType::Fetch => 0u8.hash(state),
            RequestType::Patch => 0u8.hash(state),
            RequestType::IPatch => 0u8.hash(state),
            RequestType::UnKnown => 0u8.hash(state),
        }
    }
}

impl PartialEq for RequestTypeWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for RequestTypeWrapper {}

impl From<RequestType> for RequestTypeWrapper {
    fn from(r: RequestType) -> Self {
        RequestTypeWrapper(r)
    }
}

impl From<&RequestType> for RequestTypeWrapper {
    fn from(r: &RequestType) -> Self {
        RequestTypeWrapper(*r)
    }
}

#[derive(Clone)]
pub struct RouteHandler<S>
where
    S: Clone,
{
    pub handler: Handler<S>,
    pub observe_handler: Option<Handler<S>>,
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
        observe_handler: None,
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
        observe_handler: None,
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
        observe_handler: None,
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
        observe_handler: None,
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
        observe_handler: None,
        method: RequestType::UnKnown,
    }
}

#[cfg(test)]
mod tests {

    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use crate::extractor::json::JsonPayload;

    use super::*;
    use coap_lite::{CoapRequest, CoapResponse, Packet};
    use serde_json::Value;

    #[tokio::test]
    async fn test_get() {
        let handler: RouteHandler<()> = get(|_, _| async {
            let pkt = Packet::default();
            Ok(CoapResponse::new(&pkt).unwrap())
        });
        assert_eq!(handler.method, RequestType::Get);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_put() {
        let handler: RouteHandler<()> = put(|_, _| async {
            let pkt = Packet::default();
            Ok(CoapResponse::new(&pkt).unwrap())
        });
        assert_eq!(handler.method, RequestType::Put);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_post() {
        let handler: RouteHandler<()> = post(|_, _| async {
            let pkt = Packet::default();
            Ok(CoapResponse::new(&pkt).unwrap())
        });
        assert_eq!(handler.method, RequestType::Post);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_delete() {
        let handler: RouteHandler<()> = delete(|_, _| async {
            let pkt = Packet::default();
            Ok(CoapResponse::new(&pkt).unwrap())
        });
        assert_eq!(handler.method, RequestType::Delete);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_unknown() {
        let handler: RouteHandler<()> = unknown(|_, _| async {
            let pkt = Packet::default();
            Ok(CoapResponse::new(&pkt).unwrap())
        });
        assert_eq!(handler.method, RequestType::UnKnown);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_handler() {
        let handler: RouteHandler<()> = get(|_, _| async {
            let pkt = Packet::default();
            Ok(CoapResponse::new(&pkt).unwrap())
        });
        assert_eq!(handler.method, RequestType::Get);
        assert!(handler.observe_handler.is_none());

        // Create request
        let request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );

        let payload = JsonPayload {
            raw: request.into(),
            value: Value::Null,
        };

        let state = Arc::new(Mutex::new(()));
        let result = (handler.handler)(Box::new(payload), state).await;
        assert!(result.is_ok());
    }
}
