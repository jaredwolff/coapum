use coap_lite::{CoapResponse, RequestType};

use std::convert::Infallible;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::{LegacyHandlerWrapper, Request, RouteHandler};

/// Creates a new `RouteHandler` for GET requests.
///
/// # Arguments
///
/// * `f` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
/// * `o` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
///
/// # Returns
///
/// A `RouteHandler` for GET requests.
pub fn get<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    S: Send + Sync + Clone + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(f(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });
    let observe_handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(o(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });

    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: Some(Box::new(LegacyHandlerWrapper::new(observe_handler))),
        method: RequestType::Get,
    }
}

/// Creates a new `RouteHandler` for PUT requests.
///
/// # Arguments
///
/// * `f` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
/// * `o` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
///
/// # Returns
///
/// A `RouteHandler` for PUT requests.
pub fn put<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    S: Send + Sync + Clone + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(f(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });
    let observe_handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(o(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });

    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: Some(Box::new(LegacyHandlerWrapper::new(observe_handler))),
        method: RequestType::Put,
    }
}

/// Creates a new `RouteHandler` for POST requests.
///
/// # Arguments
///
/// * `f` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
/// * `o` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
///
/// # Returns
///
/// A `RouteHandler` for POST requests.
pub fn post<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    S: Send + Sync + Clone + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(f(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });
    let observe_handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(o(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });

    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: Some(Box::new(LegacyHandlerWrapper::new(observe_handler))),
        method: RequestType::Post,
    }
}

/// Creates a new `RouteHandler` for DELETE requests.
///
/// # Arguments
///
/// * `f` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
/// * `o` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
///
/// # Returns
///
/// A `RouteHandler` for DELETE requests.
pub fn delete<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    S: Send + Sync + Clone + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(f(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });
    let observe_handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(o(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });

    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: Some(Box::new(LegacyHandlerWrapper::new(observe_handler))),
        method: RequestType::Delete,
    }
}

/// Creates a new `RouteHandler` for unknown requests.
///
/// # Arguments
///
/// * `f` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
/// * `o` - A closure that takes a boxed `Request` and an `Arc<Mutex<S>>` and returns a future that resolves to a `CoapResponse`.
///
/// # Returns
///
/// A `RouteHandler` for unknown requests.
pub fn unknown<S, F, G, Fut1, Fut2>(f: F, o: G) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut1 + Send + Sync + 'static,
    G: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut2 + Send + Sync + 'static,
    Fut1: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    Fut2: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static,
    S: Send + Sync + Clone + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(f(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });
    let observe_handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        Box::pin(o(req, state))
            as std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>
    });

    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: Some(Box::new(LegacyHandlerWrapper::new(observe_handler))),
        method: RequestType::UnKnown,
    }
}

#[cfg(test)]
mod tests {

    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    use crate::router::wrapper::IntoCoapResponse;
    use crate::router::CoapumRequest;

    use super::*;
    use coap_lite::{CoapRequest, Packet, ResponseType};

    #[tokio::test]
    async fn test_get() {
        let handler: RouteHandler<()> = get(
            |_, _| async { (ResponseType::Valid).into_response() },
            |_, _| async { (ResponseType::Valid).into_response() },
        );
        assert_eq!(handler.method, RequestType::Get);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_put() {
        let handler: RouteHandler<()> = put(
            |_, _| async { (ResponseType::Valid).into_response() },
            |_, _| async { (ResponseType::Valid).into_response() },
        );
        assert_eq!(handler.method, RequestType::Put);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_post() {
        let handler: RouteHandler<()> = post(
            |_, _| async { (ResponseType::Valid).into_response() },
            |_, _| async { (ResponseType::Valid).into_response() },
        );
        assert_eq!(handler.method, RequestType::Post);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_delete() {
        let handler: RouteHandler<()> = delete(
            |_, _| async { (ResponseType::Valid).into_response() },
            |_, _| async { (ResponseType::Valid).into_response() },
        );
        assert_eq!(handler.method, RequestType::Delete);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_unknown() {
        let handler: RouteHandler<()> = unknown(
            |_, _| async { (ResponseType::Valid).into_response() },
            |_, _| async { (ResponseType::Valid).into_response() },
        );
        assert_eq!(handler.method, RequestType::UnKnown);
        assert!(handler.observe_handler.is_some());
    }

    #[tokio::test]
    async fn test_handler() {
        let handler: RouteHandler<()> = get(
            |_, _| async { (ResponseType::Valid, vec![1, 2, 3]).into_response() },
            |_, _| async { (ResponseType::Valid, vec![3, 2, 1]).into_response() },
        );
        assert_eq!(handler.method, RequestType::Get);
        assert!(handler.observe_handler.is_some());

        // Create request
        let request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );

        let coap_request: CoapumRequest<SocketAddr> = request.into();

        let state = Arc::new(Mutex::new(()));
        let result = handler
            .handler
            .call_erased(coap_request.clone(), state.clone())
            .await
            .unwrap();
        assert_eq!(result.message.payload, vec![1, 2, 3]);

        assert!(handler.observe_handler.is_some());
        if let Some(h) = &handler.observe_handler {
            let result = h.call_erased(coap_request, state).await.unwrap();
            assert_eq!(result.message.payload, vec![3, 2, 1]);
        }
    }
}
