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

#[cfg(test)]
mod tests {

    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use crate::extractor::json::JsonPayload;

    use super::*;
    use coap_lite::{CoapRequest, CoapResponse, Packet};
    use serde_json::Value;

    #[tokio::test]
    async fn test_get() {
        let handler: RouteHandler<()> = get(
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
        );
        assert_eq!(handler.method, RequestType::Get);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_put() {
        let handler: RouteHandler<()> = put(
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
        );
        assert_eq!(handler.method, RequestType::Put);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_post() {
        let handler: RouteHandler<()> = post(
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
        );
        assert_eq!(handler.method, RequestType::Post);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_delete() {
        let handler: RouteHandler<()> = delete(
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
        );
        assert_eq!(handler.method, RequestType::Delete);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_unknown() {
        let handler: RouteHandler<()> = unknown(
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
        );
        assert_eq!(handler.method, RequestType::UnKnown);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_handler() {
        let handler: RouteHandler<()> = get(
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
            |_, _| async {
                let pkt = Packet::default();
                Ok(CoapResponse::new(&pkt).unwrap())
            },
        );
        assert_eq!(handler.method, RequestType::Get);
        assert!(handler.observe_handler.is_some());

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
        let result = (handler.handler)(Box::new(payload.clone()), state.clone()).await;
        assert!(result.is_ok());

        assert!(handler.observe_handler.is_some());
        if let Some(h) = handler.observe_handler {
            let result = h(Box::new(payload), state).await;
            assert!(result.is_ok());
        }
    }
}
