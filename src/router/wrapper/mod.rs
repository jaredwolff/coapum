use coap_lite::{CoapResponse, Packet, RequestType, ResponseType};

use core::fmt::{self, Debug};
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::{fmt::Formatter, hash::Hasher};

use tokio::sync::Mutex;

use super::{CoapumRequest, Handler, Request};

pub mod observer;

/// A wrapper struct for `RequestType` that implements `Hash`, `PartialEq`, and `Eq` traits.
#[derive(Clone, Copy, Debug)]
pub struct RequestTypeWrapper(RequestType);

impl std::hash::Hash for RequestTypeWrapper {
    /// Hashes the `RequestTypeWrapper` instance.
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
    /// Compares two `RequestTypeWrapper` instances for equality.
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for RequestTypeWrapper {}

impl From<RequestType> for RequestTypeWrapper {
    /// Converts a `RequestType` instance to a `RequestTypeWrapper` instance.
    fn from(r: RequestType) -> Self {
        RequestTypeWrapper(r)
    }
}

impl From<&RequestType> for RequestTypeWrapper {
    /// Converts a reference to a `RequestType` instance to a `RequestTypeWrapper` instance.
    fn from(r: &RequestType) -> Self {
        RequestTypeWrapper(*r)
    }
}

/// A struct that represents a route handler.
pub struct RouteHandler<S>
where
    S: Send + Sync + 'static,
{
    /// The handler function for the route.
    pub handler: Box<dyn ErasedHandler<S>>,
    /// The handler function for the observe request.
    pub observe_handler: Option<Box<dyn ErasedHandler<S>>>,
    /// The request type for the route.
    pub method: RequestType,
}

impl<S> Debug for RouteHandler<S>
where
    S: Send + Sync + 'static,
{
    /// Formats the `RouteHandler` instance for debugging purposes.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "RouteHandler {{ method: {:?} }}", self.method)
    }
}

impl<S> Clone for RouteHandler<S>
where
    S: Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone_erased(),
            observe_handler: self.observe_handler.as_ref().map(|h| h.clone_erased()),
            method: self.method,
        }
    }
}

/// Creates a `RouteHandler` instance for a GET request.
#[deprecated(note = "Use RouterBuilder::get instead")]
pub fn get<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = CoapResponseResult> + Send + 'static,
    S: Send + Sync + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        let future = f(req, state);
        Box::pin(future) as Pin<Box<dyn Future<Output = CoapResponseResult> + Send>>
    });
    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: None,
        method: RequestType::Get,
    }
}

/// Creates a `RouteHandler` instance for a PUT request.
#[deprecated(note = "Use RouterBuilder::put instead")]
pub fn put<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = CoapResponseResult> + Send + 'static,
    S: Send + Sync + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        let future = f(req, state);
        Box::pin(future) as Pin<Box<dyn Future<Output = CoapResponseResult> + Send>>
    });
    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: None,
        method: RequestType::Put,
    }
}

/// Creates a `RouteHandler` instance for a POST request.
#[deprecated(note = "Use RouterBuilder::post instead")]
pub fn post<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = CoapResponseResult> + Send + 'static,
    S: Send + Sync + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        let future = f(req, state);
        Box::pin(future) as Pin<Box<dyn Future<Output = CoapResponseResult> + Send>>
    });
    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: None,
        method: RequestType::Post,
    }
}

/// Creates a `RouteHandler` instance for a DELETE request.
#[deprecated(note = "Use RouterBuilder::delete instead")]
pub fn delete<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = CoapResponseResult> + Send + 'static,
    S: Send + Sync + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        let future = f(req, state);
        Box::pin(future) as Pin<Box<dyn Future<Output = CoapResponseResult> + Send>>
    });
    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: None,
        method: RequestType::Delete,
    }
}

/// Creates a `RouteHandler` instance for an UNKNOWN request.
#[deprecated(note = "Use RouterBuilder::any instead")]
pub fn unknown<S, F, Fut>(f: F) -> RouteHandler<S>
where
    F: Fn(Box<dyn Request>, Arc<Mutex<S>>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = CoapResponseResult> + Send + 'static,
    S: Send + Sync + 'static,
{
    let handler = Arc::new(move |req: Box<dyn Request>, state: Arc<Mutex<S>>| {
        let future = f(req, state);
        Box::pin(future) as Pin<Box<dyn Future<Output = CoapResponseResult> + Send>>
    });
    RouteHandler {
        handler: Box::new(LegacyHandlerWrapper::new(handler)),
        observe_handler: None,
        method: RequestType::UnKnown,
    }
}

pub type CoapResponseResult = Result<CoapResponse, Infallible>;

/// Legacy handler type for compatibility during migration
type LegacyHandler<S> = Arc<
    dyn Fn(
            Box<dyn Request>,
            Arc<Mutex<S>>,
        ) -> Pin<Box<dyn Future<Output = CoapResponseResult> + Send>>
        + Send
        + Sync,
>;

/// Wrapper to bridge old handler format to new ErasedHandler
pub struct LegacyHandlerWrapper<S> {
    handler: LegacyHandler<S>,
}

impl<S> LegacyHandlerWrapper<S> {
    pub fn new(handler: LegacyHandler<S>) -> Self {
        Self { handler }
    }
}

#[async_trait]
impl<S> ErasedHandler<S> for LegacyHandlerWrapper<S>
where
    S: Send + Sync + 'static,
{
    async fn call_erased(
        &self,
        req: CoapumRequest<SocketAddr>,
        state: Arc<Mutex<S>>,
    ) -> Result<CoapResponse, Infallible> {
        // Create a minimal Request implementation for the legacy handler
        struct LegacyRequest {
            raw: CoapumRequest<SocketAddr>,
            value: serde_json::Value,
        }

        impl Request for LegacyRequest {
            fn get_value(&self) -> &serde_json::Value {
                &self.value
            }

            fn get_raw(&self) -> &CoapumRequest<SocketAddr> {
                &self.raw
            }
        }

        let legacy_req = LegacyRequest {
            raw: req,
            value: serde_json::Value::Null, // Default value for compatibility
        };

        (self.handler)(Box::new(legacy_req), state).await
    }

    fn clone_erased(&self) -> Box<dyn ErasedHandler<S>> {
        Box::new(LegacyHandlerWrapper {
            handler: self.handler.clone(),
        })
    }
}

pub trait IntoCoapResponse {
    fn into_response(self) -> CoapResponseResult;
}

impl IntoCoapResponse for ResponseType {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.set_status(self);
        Ok(response)
    }
}

impl<R> IntoCoapResponse for (ResponseType, R)
where
    R: IntoCoapResponse,
{
    fn into_response(self) -> CoapResponseResult {
        let mut response = self.1.into_response().unwrap();
        response.set_status(self.0);
        Ok(response)
    }
}

impl IntoCoapResponse for Box<dyn Request> {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.message.header.message_id = self.get_raw().message.header.message_id;
        response
            .message
            .set_token(self.get_raw().message.get_token().to_vec());
        Ok(response)
    }
}

impl IntoCoapResponse for &CoapumRequest<SocketAddr> {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.message.header.message_id = self.message.header.message_id;
        response
            .message
            .set_token(self.message.get_token().to_vec());
        Ok(response)
    }
}

impl IntoCoapResponse for CoapumRequest<SocketAddr> {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.message.header.message_id = self.message.header.message_id;
        response
            .message
            .set_token(self.message.get_token().to_vec());
        Ok(response)
    }
}

impl IntoCoapResponse for ciborium::Value {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();

        let mut buffer = Vec::new();
        let _ = ciborium::ser::into_writer(&self, &mut buffer);

        response.message.payload = buffer;
        Ok(response)
    }
}

impl IntoCoapResponse for serde_json::Value {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.message.payload = serde_json::to_vec(&self).unwrap();
        Ok(response)
    }
}

impl IntoCoapResponse for Vec<u8> {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.message.payload = self;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use coap_lite::{CoapRequest, Packet};
    use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

    #[tokio::test]
    async fn test_get() {
        let handler: RouteHandler<()> = get(|_, _| async { (ResponseType::Valid).into_response() });
        assert_eq!(handler.method, RequestType::Get);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_put() {
        let handler: RouteHandler<()> = put(|_, _| async { (ResponseType::Valid).into_response() });
        assert_eq!(handler.method, RequestType::Put);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_post() {
        let handler: RouteHandler<()> =
            post(|_, _| async { (ResponseType::Valid).into_response() });
        assert_eq!(handler.method, RequestType::Post);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_delete() {
        let handler: RouteHandler<()> =
            delete(|_, _| async { (ResponseType::Valid).into_response() });
        assert_eq!(handler.method, RequestType::Delete);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_unknown() {
        let handler: RouteHandler<()> =
            unknown(|_, _| async { (ResponseType::Valid).into_response() });
        assert_eq!(handler.method, RequestType::UnKnown);
        assert!(handler.observe_handler.is_none());
    }

    #[tokio::test]
    async fn test_handler() {
        let handler: RouteHandler<()> =
            get(|_, _| async { (ResponseType::Valid, vec![1, 2, 3]).into_response() });
        assert_eq!(handler.method, RequestType::Get);
        assert!(handler.observe_handler.is_none());

        // Create request
        let request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );

        let coap_request: CoapumRequest<SocketAddr> = request.into();
        let state = Arc::new(Mutex::new(()));
        let result = handler
            .handler
            .call_erased(coap_request, state)
            .await
            .unwrap();
        assert_eq!(result.message.payload, vec![1, 2, 3]);
    }
}
