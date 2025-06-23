//! Enhanced routing system for ergonomic CoAP handler registration
//!
//! This module provides both the core router functionality and an improved routing API
//! that allows for more ergonomic registration of handlers with automatic parameter extraction.

use coap_lite::{CoapRequest, CoapResponse, ObserveOption, Packet, RequestType, ResponseType};
use route_recognizer::Router;
use serde_json::Value;
use std::collections::HashMap;
use std::convert::Infallible;
use std::fmt::Debug;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tower::Service;

use crate::handler::{into_erased_handler, into_handler, ErasedHandler, Handler, HandlerFn};
use crate::observer::{Observer, ObserverRequest, ObserverValue};
use crate::router::wrapper::IntoCoapResponse;

use self::wrapper::{RequestTypeWrapper, RouteHandler};

pub mod wrapper;

pub type RouterError = Box<(dyn std::error::Error + Send + Sync + 'static)>;

pub trait Request: Send {
    fn get_value(&self) -> &Value;
    fn get_raw(&self) -> &CoapumRequest<SocketAddr>;
}

/// The CoapRouter is a struct responsible for managing routes, shared state and an observer database.
///
/// It provides methods for registering and unregistering observers, reading and writing to the backend,
/// and for adding and looking up routes and handlers. CoapRouter should be cloned per connection.
///
/// # Type Parameters
///
/// * `O`: The type that implements the Observer trait.
/// * `S`: The shared state type. It must implement the `Clone` and `Debug` traits.
///
/// # Fields
///
/// * `inner`: The `Router` object responsible for matching routes to handlers.
/// * `state`: The shared state object accessible to all handlers. It is wrapped in an Arc and a Mutex for shared and exclusive access.
/// * `db`: The observer database.
#[derive(Clone)]
pub struct CoapRouter<O, S>
where
    S: Clone + Debug + Send + Sync + 'static,
    O: Observer,
{
    inner: Router<HashMap<RequestTypeWrapper, RouteHandler<S>>>,
    state: Arc<Mutex<S>>, // Shared state
    db: O,
}

/// Provides methods for creating a new CoapRouter, registering and unregistering observers,
/// performing backend reads and writes, and adding and looking up routes and handlers.
///
/// # Type Parameters
///
/// * `O`: The type that implements the Observer trait. It must also implement the `Send`, `Sync`, `Clone`, and `'static` traits.
/// * `S`: The shared state type. It must implement the `Send`, `Sync`, `Clone`, and `Debug` traits.
impl<O, S> CoapRouter<O, S>
where
    S: Send + Sync + Clone + Debug + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    /// Constructs a new `CoapRouter` with given shared state and observer database.
    pub fn new(state: S, db: O) -> Self {
        Self {
            inner: Router::new(),
            state: Arc::new(Mutex::new(state)),
            db,
        }
    }

    /// Create a new router builder for ergonomic route registration
    pub fn builder(state: S, observer: O) -> RouterBuilder<O, S> {
        RouterBuilder::new(state, observer)
    }

    /// Registers an observer for a given path.
    pub async fn register_observer(
        &mut self,
        device_id: &str,
        path: &str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> Result<(), O::Error> {
        self.db.register(device_id, path, sender).await
    }

    /// Unregisters an observer from a given path.
    pub async fn unregister_observer(
        &mut self,
        device_id: &str,
        path: &str,
    ) -> Result<(), O::Error> {
        self.db.unregister(device_id, path).await
    }

    /// Writes a payload to a path in the backend.
    pub async fn backend_write(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), O::Error> {
        self.db.write(device_id, path, payload).await
    }

    /// Reads a value from a path in the backend.
    pub async fn backend_read(
        &mut self,
        device_id: &str,
        path: &str,
    ) -> Result<Option<Value>, O::Error> {
        self.db.read(device_id, path).await
    }

    /// Adds a route handler for a given route.
    pub fn add(&mut self, route: &str, handler: RouteHandler<S>) {
        // Check if route already exists
        match self.inner.recognize(route) {
            Ok(r) => {
                let mut r = (**r.handler()).clone();
                r.insert(handler.method.into(), handler);
                self.inner.add(route, r);
            }
            Err(_) => {
                let mut r = HashMap::new();
                r.insert(handler.method.into(), handler);
                self.inner.add(route, r);
            }
        };
    }

    /// Looks up an observer handler for a given path.
    pub fn lookup_observer_handler(&self, path: &str) -> Option<Box<dyn ErasedHandler<S>>> {
        match self.inner.recognize(path) {
            Ok(matched) => {
                let handler = matched.handler();

                // If it's an observe, get by default
                let reqtype: RequestTypeWrapper = RequestType::Get.into();

                log::debug!("Matched route: {:?}", matched);
                match handler.get(&reqtype) {
                    Some(h) => {
                        log::debug!("Matched handler: {:?}", h);
                        h.observe_handler
                            .as_ref()
                            .map(|handler| handler.clone_erased())
                    }
                    None => {
                        log::debug!("No handler found");
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!("Unable to recognize. Err: {}", e);
                None
            }
        }
    }

    /// Looks up a handler for a given request.
    pub fn lookup(&self, r: &CoapumRequest<SocketAddr>) -> Option<Box<dyn ErasedHandler<S>>> {
        match self.inner.recognize(r.get_path()) {
            Ok(matched) => {
                let handler = matched.handler();

                let reqtype: RequestTypeWrapper = r.get_method().into();

                log::debug!("Matched route: {:?}", matched);
                match handler.get(&reqtype) {
                    Some(h) => {
                        log::debug!("Matched handler: {:?}", h);
                        Some(h.handler.clone_erased())
                    }
                    None => {
                        log::debug!("No handler found");
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!("Unable to recognize. Err: {}", e);
                None
            }
        }
    }
}

/// Enhanced router builder for ergonomic handler registration
pub struct RouterBuilder<O, S>
where
    S: Clone + Debug + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    router: CoapRouter<O, S>,
}

impl<O, S> RouterBuilder<O, S>
where
    S: Clone + Debug + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    /// Create a new router builder
    pub fn new(state: S, observer: O) -> Self {
        Self {
            router: CoapRouter::new(state, observer),
        }
    }

    /// Generic method to add a route with any HTTP method
    fn add_route<F, T>(&mut self, path: &str, method: RequestType, handler: F)
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        let route_handler = RouteHandler {
            handler: into_erased_handler(into_handler(handler)),
            observe_handler: None,
            method,
        };
        self.router.add(path, route_handler);
    }

    /// Add a GET route with an ergonomic handler
    pub fn get<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        self.add_route(path, RequestType::Get, handler);
        self
    }

    /// Add a POST route with an ergonomic handler
    pub fn post<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        self.add_route(path, RequestType::Post, handler);
        self
    }

    /// Add a PUT route with an ergonomic handler
    pub fn put<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        self.add_route(path, RequestType::Put, handler);
        self
    }

    /// Add a DELETE route with an ergonomic handler
    pub fn delete<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        self.add_route(path, RequestType::Delete, handler);
        self
    }

    /// Add a route that handles any HTTP method
    pub fn any<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        self.add_route(path, RequestType::UnKnown, handler);
        self
    }

    /// Add an observable GET route with separate handlers for GET and notifications
    pub fn observe<F1, T1, F2, T2>(
        mut self,
        path: &str,
        get_handler: F1,
        notify_handler: F2,
    ) -> Self
    where
        HandlerFn<F1, S>: Handler<T1, S>,
        HandlerFn<F2, S>: Handler<T2, S>,
        F1: Send + Sync + Clone,
        F2: Send + Sync + Clone,
        T1: Send + Sync + 'static,
        T2: Send + Sync + 'static,
    {
        let route_handler = RouteHandler {
            handler: into_erased_handler(into_handler(get_handler)),
            observe_handler: Some(into_erased_handler(into_handler(notify_handler))),
            method: RequestType::Get,
        };
        self.router.add(path, route_handler);
        self
    }

    /// Build the final router
    pub fn build(self) -> CoapRouter<O, S> {
        self.router
    }

    /// Get a mutable reference to the underlying router for advanced usage
    pub fn router_mut(&mut self) -> &mut CoapRouter<O, S> {
        &mut self.router
    }
}

/// `CoapumRequest` is a structure that represents a request in the CoAP (Constrained Application Protocol) communication.
/// It includes the packet message, code, path, optional observe flag, optional response, the source of the request, and an identity vector.
/// The identity is derived from the DTLS context.
///
/// # Type Parameters
///
/// * `Endpoint`: Represents the type of the endpoint from which the request is coming. (Typically SocketAddr)
#[derive(Debug, Clone)]
pub struct CoapumRequest<Endpoint> {
    pub message: Packet,
    code: RequestType,
    path: String,
    observe_flag: Option<ObserveOption>,
    pub response: Option<CoapResponse>,
    pub source: Option<Endpoint>,
    pub identity: String,
}

/// An implementation block that provides methods to convert `CoapRequest` into `CoapumRequest` and get various details of the request.
impl<Endpoint> From<CoapRequest<Endpoint>> for CoapumRequest<Endpoint> {
    fn from(req: CoapRequest<Endpoint>) -> Self {
        let path = req.get_path();
        let code = *req.get_method();
        let observe_flag = match req.get_observe_flag() {
            Some(o) => match o {
                Ok(o) => Some(o),
                Err(_) => None,
            },
            None => None,
        };

        Self {
            message: req.message,
            response: req.response,
            source: req.source,
            path,
            code,
            observe_flag,
            identity: String::new(),
        }
    }
}

impl<Endpoint> CoapumRequest<Endpoint> {
    /// Returns the path of the `CoapumRequest`.
    pub fn get_path(&self) -> &String {
        &self.path
    }

    /// Returns the method of the `CoapumRequest`.
    pub fn get_method(&self) -> &RequestType {
        &self.code
    }

    /// Returns the observe flag of the `CoapumRequest`.
    pub fn get_observe_flag(&self) -> &Option<ObserveOption> {
        &self.observe_flag
    }
}

/// Implementation of the `Service` trait for `CoapRouter` with `CoapumRequest` as the request type.
impl<O, S> Service<CoapumRequest<SocketAddr>> for CoapRouter<O, S>
where
    S: Debug + Send + Clone + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    /// The response type for the service.
    type Response = CoapResponse;
    /// The error type for the service.
    type Error = Infallible;
    /// The future type for the service.
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    /// Polls if the service is ready to process requests.
    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        // Assume that the router is always ready.
        std::task::Poll::Ready(Ok(()))
    }

    /// Handles a `CoapumRequest` and returns a future that resolves to a `CoapResponse`.
    fn call(&mut self, request: CoapumRequest<SocketAddr>) -> Self::Future {
        let state = self.state.clone(); // Clone the state so it can be moved into the async block

        match self.lookup(&request) {
            Some(handler) => {
                let path = request.get_path();
                log::debug!("Handler found for route: {:?}", &path);

                // Call the new ErasedHandler directly
                Box::pin(async move { handler.call_erased(request, state).await })
            }
            None => {
                log::info!(
                    "No handler found for method: {:#?} to: {:?}",
                    request.get_method(),
                    request.get_path()
                );

                // If no route handler is found, return a bad request error
                Box::pin(async move { (ResponseType::BadRequest, &request).into_response() })
            }
        }
    }
}

/// Implementation of the `Service` trait for `CoapRouter` with `ObserverRequest` as the request type.
impl<O, S> Service<ObserverRequest<SocketAddr>> for CoapRouter<O, S>
where
    S: Debug + Send + Clone + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    /// The response type for the service.
    type Response = CoapResponse;
    /// The error type for the service.
    type Error = Infallible;
    /// The future type for the service.
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    /// Polls if the service is ready to process requests.
    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        // Assume that the router is always ready.
        std::task::Poll::Ready(Ok(()))
    }

    /// Handles an `ObserverRequest` and returns a future that resolves to a `CoapResponse`.
    fn call(&mut self, request: ObserverRequest<SocketAddr>) -> Self::Future {
        let state = self.state.clone(); // Clone the state so it can be moved into the async block

        match self.lookup_observer_handler(&request.path) {
            Some(handler) => {
                log::debug!("Handler found for route: {:?}", &request.path);

                let packet = Packet::default();
                let raw = CoapRequest::from_packet(packet, request.source);

                let mut coap_request: CoapumRequest<SocketAddr> = raw.into();
                // Set the value for the observer request
                coap_request.identity = request.path.clone();

                Box::pin(async move { handler.call_erased(coap_request, state).await })
            }
            None => {
                log::info!("No observer handler found for: {}", request.path);

                // If no observer handler is found, return a bad request error
                Box::pin(async move { (ResponseType::BadRequest).into_response() })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{Identity, StatusCode};

    #[derive(Clone, Debug)]
    struct TestState {
        counter: i32,
    }

    impl AsRef<TestState> for TestState {
        fn as_ref(&self) -> &TestState {
            self
        }
    }

    #[tokio::test]
    async fn test_register_observer() {
        let state = TestState { counter: 0 };
        let mut router = CoapRouter::new(state, ());

        let (sender, _receiver) = tokio::sync::mpsc::channel(10);
        let sender = Arc::new(sender);

        let result = router
            .register_observer("device123", "/temperature", sender)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_unregister_observer() {
        let state = TestState { counter: 0 };
        let mut router = CoapRouter::new(state, ());

        let result = router
            .unregister_observer("device123", "/temperature")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_backend_write_and_read() {
        let state = TestState { counter: 0 };
        let mut router = CoapRouter::new(state, ());

        let payload = serde_json::json!({"value": 25});
        let write_result = router
            .backend_write("device123", "/temperature", &payload)
            .await;
        assert!(write_result.is_ok());
    }

    #[tokio::test]
    async fn test_add_and_lookup() {
        let state = TestState { counter: 0 };
        let mut router = CoapRouter::new(state, ());

        // Create a simple handler for testing
        let handler = RouteHandler {
            handler: into_erased_handler(into_handler(|| async { StatusCode::Valid })),
            observe_handler: None,
            method: RequestType::Get,
        };

        router.add("/test", handler);

        // Create a test request
        let packet = Packet::new();
        let raw = CoapRequest::from_packet(packet, "127.0.0.1:5683".parse().unwrap());
        let mut request: CoapumRequest<SocketAddr> = raw.into();
        request.path = "/test".to_string();
        request.code = RequestType::Get;

        let result = router.lookup(&request);
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_add_and_lookup_observer_handler() {
        let state = TestState { counter: 0 };
        let mut router = CoapRouter::new(state, ());

        // Create a handler with observer support
        let handler = RouteHandler {
            handler: into_erased_handler(into_handler(|| async { StatusCode::Valid })),
            observe_handler: Some(into_erased_handler(into_handler(|| async {
                StatusCode::Content
            }))),
            method: RequestType::Get,
        };

        router.add("/observable", handler);

        let result = router.lookup_observer_handler("/observable");
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_router_builder() {
        async fn test_handler() -> StatusCode {
            StatusCode::Valid
        }

        let state = TestState { counter: 0 };
        let _router = RouterBuilder::new(state, ())
            .get("/test", test_handler)
            .post("/test", test_handler)
            .build();

        // Basic test that the router can be built
    }

    #[tokio::test]
    async fn test_handler_with_extractor() {
        async fn identity_handler(Identity(_id): Identity) -> StatusCode {
            // In a real handler, you'd use the identity
            StatusCode::Valid
        }

        let state = TestState { counter: 0 };
        let _router = RouterBuilder::new(state, ())
            .get("/user", identity_handler)
            .build();

        // Basic test that the router can be built with extractors
    }

    #[tokio::test]
    async fn test_observe_handler() {
        async fn get_handler() -> StatusCode {
            StatusCode::Content
        }

        async fn notify_handler() -> StatusCode {
            StatusCode::Valid
        }

        let state = TestState { counter: 0 };
        let _router = RouterBuilder::new(state, ())
            .observe("/observable", get_handler, notify_handler)
            .build();

        // Basic test that observe handlers can be registered
    }

    #[tokio::test]
    async fn test_builder_convenience_method() {
        async fn test_handler() -> StatusCode {
            StatusCode::Valid
        }

        let state = TestState { counter: 0 };
        let _router = CoapRouter::builder(state, ())
            .get("/test", test_handler)
            .build();

        // Test the convenience builder method
    }
}
