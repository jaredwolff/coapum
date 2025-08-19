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
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::{Mutex, RwLock};
use tower::Service;

use crate::handler::{into_erased_handler, into_handler, ErasedHandler, Handler, HandlerFn};
use crate::observer::{Observer, ObserverRequest, ObserverValue};
use crate::router::wrapper::IntoCoapResponse;

use self::wrapper::{RequestTypeWrapper, RouteHandler};

pub mod wrapper;

pub type RouterError = Box<(dyn std::error::Error + Send + Sync + 'static)>;

/// Type alias for complex state update function type
type StateUpdateFn<S> = Box<dyn FnOnce(&mut S) + Send + 'static>;

/// Type alias for state update channel sender
type StateUpdateSender<S> = mpsc::Sender<StateUpdateFn<S>>;

/// Type alias for state update channel receiver
type StateUpdateReceiver<S> = mpsc::Receiver<StateUpdateFn<S>>;

/// A handle that allows external code to trigger observer notifications
/// without having direct access to the router.
#[derive(Clone)]
pub struct NotificationTrigger<O>
where
    O: Observer + Send + Sync + Clone + 'static,
{
    observer: O,
}

impl<O> NotificationTrigger<O>
where
    O: Observer + Send + Sync + Clone + 'static,
{
    /// Create a new notification trigger
    pub fn new(observer: O) -> Self {
        Self { observer }
    }

    /// Trigger a notification for observers of a specific device and path
    pub async fn trigger_notification(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &serde_json::Value,
    ) -> Result<(), O::Error> {
        self.observer.write(device_id, path, payload).await
    }
}

/// A handle that allows external code to update the application state
/// without having direct access to the router.
#[derive(Clone)]
pub struct StateUpdateHandle<S>
where
    S: Send + Sync + Clone + 'static,
{
    sender: StateUpdateSender<S>,
}

impl<S> StateUpdateHandle<S>
where
    S: Send + Sync + Clone + 'static,
{
    /// Create a new state update handle
    pub fn new(sender: StateUpdateSender<S>) -> Self {
        Self { sender }
    }

    /// Update the application state using a closure
    ///
    /// This allows external components to modify the shared state safely.
    /// The update is queued and applied asynchronously by the router.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use coapum::StateUpdateHandle;
    /// # #[derive(Clone)]
    /// # struct MyAppState {
    /// #     counter: i32,
    /// #     config: Config,
    /// # }
    /// # #[derive(Clone)]
    /// # struct Config {
    /// #     max_db_connections: usize,
    /// # }
    /// # async fn example(state_handle: StateUpdateHandle<MyAppState>) -> Result<(), Box<dyn std::error::Error>> {
    /// // Update a counter in the state
    /// state_handle.update(|state: &mut MyAppState| {
    ///     state.counter += 1;
    /// }).await?;
    ///
    /// // Update database connection pool size
    /// state_handle.update(|state: &mut MyAppState| {
    ///     state.config.max_db_connections = 50;
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn update<F>(&self, updater: F) -> Result<(), StateUpdateError>
    where
        F: FnOnce(&mut S) + Send + 'static,
    {
        self.sender
            .send(Box::new(updater))
            .await
            .map_err(|_| StateUpdateError::ChannelClosed)
    }

    /// Attempt to update the state without blocking
    ///
    /// Returns an error if the update channel is full or closed.
    pub fn try_update<F>(&self, updater: F) -> Result<(), StateUpdateError>
    where
        F: FnOnce(&mut S) + Send + 'static,
    {
        self.sender
            .try_send(Box::new(updater))
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => StateUpdateError::ChannelFull,
                mpsc::error::TrySendError::Closed(_) => StateUpdateError::ChannelClosed,
            })
    }
}

/// Error type for state update operations
#[derive(Debug, Clone, PartialEq)]
pub enum StateUpdateError {
    /// The update channel is full (try_update only)
    ChannelFull,
    /// The update channel is closed (router dropped)
    ChannelClosed,
}

impl std::fmt::Display for StateUpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateUpdateError::ChannelFull => write!(f, "State update channel is full"),
            StateUpdateError::ChannelClosed => write!(f, "State update channel is closed"),
        }
    }
}

impl std::error::Error for StateUpdateError {}

/// A handle that allows external code to manage client authentication
/// without having direct access to the server's PSK store.
#[derive(Clone)]
pub struct ClientManager {
    sender: mpsc::Sender<ClientCommand>,
}

/// Commands for client management operations
#[derive(Debug)]
pub enum ClientCommand {
    /// Add a new client with PSK authentication
    AddClient {
        identity: String,
        key: Vec<u8>,
        metadata: Option<ClientMetadata>,
    },
    /// Remove a client
    RemoveClient { identity: String },
    /// Update an existing client's key
    UpdateKey { identity: String, key: Vec<u8> },
    /// Update client metadata
    UpdateMetadata {
        identity: String,
        metadata: ClientMetadata,
    },
    /// Enable or disable a client
    SetClientEnabled { identity: String, enabled: bool },
    /// Get all client identities (response via oneshot channel)
    ListClients {
        response: tokio::sync::oneshot::Sender<Vec<String>>,
    },
}

/// Metadata associated with a client
#[derive(Debug, Clone, Default)]
pub struct ClientMetadata {
    /// Optional friendly name for the client
    pub name: Option<String>,
    /// Optional description
    pub description: Option<String>,
    /// Whether the client is enabled
    pub enabled: bool,
    /// Optional tags for categorization
    pub tags: Vec<String>,
    /// Custom key-value pairs
    pub custom: HashMap<String, String>,
}

impl ClientManager {
    /// Create a new client manager
    pub fn new(sender: mpsc::Sender<ClientCommand>) -> Self {
        Self { sender }
    }

    /// Add a new client with PSK authentication
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use coapum::router::ClientManager;
    /// # async fn example(client_manager: ClientManager) -> Result<(), Box<dyn std::error::Error>> {
    /// // Add a simple client
    /// client_manager.add_client("device_001", b"secret_key_123").await?;
    ///
    /// // Add a client with metadata
    /// let metadata = coapum::router::ClientMetadata {
    ///     name: Some("Temperature Sensor".to_string()),
    ///     enabled: true,
    ///     tags: vec!["sensor".to_string(), "outdoor".to_string()],
    ///     ..Default::default()
    /// };
    /// client_manager.add_client_with_metadata("device_002", b"secret_key_456", metadata).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn add_client(&self, identity: &str, key: &[u8]) -> Result<(), ClientManagerError> {
        self.sender
            .send(ClientCommand::AddClient {
                identity: identity.to_string(),
                key: key.to_vec(),
                metadata: None,
            })
            .await
            .map_err(|_| ClientManagerError::ChannelClosed)
    }

    /// Add a new client with metadata
    pub async fn add_client_with_metadata(
        &self,
        identity: &str,
        key: &[u8],
        metadata: ClientMetadata,
    ) -> Result<(), ClientManagerError> {
        self.sender
            .send(ClientCommand::AddClient {
                identity: identity.to_string(),
                key: key.to_vec(),
                metadata: Some(metadata),
            })
            .await
            .map_err(|_| ClientManagerError::ChannelClosed)
    }

    /// Remove a client
    pub async fn remove_client(&self, identity: &str) -> Result<(), ClientManagerError> {
        self.sender
            .send(ClientCommand::RemoveClient {
                identity: identity.to_string(),
            })
            .await
            .map_err(|_| ClientManagerError::ChannelClosed)
    }

    /// Update a client's PSK key
    pub async fn update_key(&self, identity: &str, key: &[u8]) -> Result<(), ClientManagerError> {
        self.sender
            .send(ClientCommand::UpdateKey {
                identity: identity.to_string(),
                key: key.to_vec(),
            })
            .await
            .map_err(|_| ClientManagerError::ChannelClosed)
    }

    /// Update client metadata
    pub async fn update_metadata(
        &self,
        identity: &str,
        metadata: ClientMetadata,
    ) -> Result<(), ClientManagerError> {
        self.sender
            .send(ClientCommand::UpdateMetadata {
                identity: identity.to_string(),
                metadata,
            })
            .await
            .map_err(|_| ClientManagerError::ChannelClosed)
    }

    /// Enable or disable a client
    pub async fn set_client_enabled(
        &self,
        identity: &str,
        enabled: bool,
    ) -> Result<(), ClientManagerError> {
        self.sender
            .send(ClientCommand::SetClientEnabled {
                identity: identity.to_string(),
                enabled,
            })
            .await
            .map_err(|_| ClientManagerError::ChannelClosed)
    }

    /// List all registered client identities
    pub async fn list_clients(&self) -> Result<Vec<String>, ClientManagerError> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.sender
            .send(ClientCommand::ListClients { response: tx })
            .await
            .map_err(|_| ClientManagerError::ChannelClosed)?;

        rx.await.map_err(|_| ClientManagerError::ResponseFailed)
    }
}

/// Error type for client manager operations
#[derive(Debug, Clone, PartialEq)]
pub enum ClientManagerError {
    /// The command channel is closed
    ChannelClosed,
    /// Failed to receive response
    ResponseFailed,
}

impl std::fmt::Display for ClientManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientManagerError::ChannelClosed => write!(f, "Client manager channel is closed"),
            ClientManagerError::ResponseFailed => {
                write!(f, "Failed to receive response from client manager")
            }
        }
    }
}

impl std::error::Error for ClientManagerError {}

/// Internal client store entry
#[derive(Debug, Clone)]
pub struct ClientEntry {
    /// The PSK key
    pub key: Vec<u8>,
    /// Client metadata
    pub metadata: ClientMetadata,
}

/// Shared client store type
pub type ClientStore = Arc<RwLock<HashMap<String, ClientEntry>>>;

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
    // Channel for external state updates
    state_update_sender: Option<StateUpdateSender<S>>,
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
            state_update_sender: None,
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

    /// Unregisters all observers for a given device.
    pub async fn unregister_all(&mut self, _device_id: &str) -> Result<(), O::Error> {
        self.db.unregister_all().await
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

    /// Triggers observer notifications for a specific device and path.
    /// This is useful when the application needs to notify observers
    /// about changes that happened outside of the normal request flow.
    pub async fn trigger_notification(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), O::Error> {
        // Use backend_write which will trigger the observer notifications
        self.backend_write(device_id, path, payload).await
    }

    /// Reads a value from a path in the backend.
    pub async fn backend_read(
        &mut self,
        device_id: &str,
        path: &str,
    ) -> Result<Option<Value>, O::Error> {
        self.db.read(device_id, path).await
    }

    /// Enable external state updates and return a handle for external components
    ///
    /// This creates a channel that allows external components to safely update
    /// the shared application state. Returns a StateUpdateHandle and starts
    /// a background task to process state updates.
    ///
    /// # Arguments
    ///
    /// * `buffer_size` - The size of the update channel buffer (default: 1000)
    ///
    /// # Returns
    ///
    /// Returns a StateUpdateHandle that external components can use to queue state updates.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use coapum::router::CoapRouter;
    /// # use coapum::observer::memory::MemObserver;
    /// # #[derive(Clone, Debug)]
    /// # struct AppState {
    /// #     counter: i32,
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let app_state = AppState { counter: 0 };
    /// # let observer = MemObserver::new();
    /// let mut router = CoapRouter::new(app_state, observer);
    /// let state_handle = router.enable_state_updates(1000);
    ///
    /// // External component can now update state:
    /// state_handle.update(|state| {
    ///     state.counter += 1;
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn enable_state_updates(&mut self, buffer_size: usize) -> StateUpdateHandle<S> {
        let (sender, receiver) = mpsc::channel(buffer_size);
        self.state_update_sender = Some(sender.clone());

        // Spawn background task to process state updates
        let state = Arc::clone(&self.state);
        tokio::spawn(async move {
            Self::process_state_updates(state, receiver).await;
        });

        StateUpdateHandle::new(sender)
    }

    /// Process state updates from the channel
    ///
    /// This runs in a background task and applies state updates sequentially
    /// to maintain consistency.
    async fn process_state_updates(state: Arc<Mutex<S>>, mut receiver: StateUpdateReceiver<S>) {
        while let Some(update) = receiver.recv().await {
            let mut state_guard = state.lock().await;
            update(&mut *state_guard);
        }
    }

    /// Get a state update handle if state updates are enabled
    ///
    /// Returns None if enable_state_updates() has not been called.
    pub fn state_update_handle(&self) -> Option<StateUpdateHandle<S>> {
        self.state_update_sender
            .as_ref()
            .map(|sender| StateUpdateHandle::new(sender.clone()))
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
        log::debug!("Looking up observer handler for path: '{}'", path);
        match self.inner.recognize(path) {
            Ok(matched) => {
                let handler = matched.handler();

                // If it's an observe, get by default
                let reqtype: RequestTypeWrapper = RequestType::Get.into();

                log::debug!("Matched route: {:?}", matched);
                match handler.get(&reqtype) {
                    Some(h) => {
                        log::debug!(
                            "Matched handler, has observe_handler: {}",
                            h.observe_handler.is_some()
                        );
                        h.observe_handler
                            .as_ref()
                            .map(|handler| handler.clone_erased())
                    }
                    None => {
                        log::debug!("No handler found for GET method");
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "Unable to recognize observer handler path '{}'. Err: {}",
                    path,
                    e
                );
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

    /// Create a notification trigger handle for external code to trigger observer notifications
    pub fn notification_trigger(&self) -> NotificationTrigger<O> {
        NotificationTrigger::new(self.router.db.clone())
    }

    /// Enable external state updates and return a handle for external components
    ///
    /// This is a convenience method that calls enable_state_updates on the underlying router.
    ///
    /// # Arguments
    ///
    /// * `buffer_size` - The size of the update channel buffer (default: 1000)
    ///
    /// # Returns
    ///
    /// Returns a StateUpdateHandle that external components can use to queue state updates.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use coapum::RouterBuilder;
    /// # use coapum::observer::memory::MemObserver;
    /// # #[derive(Clone, Debug)]
    /// # struct AppState {
    /// #     counter: i32,
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let app_state = AppState { counter: 0 };
    /// # let observer = MemObserver::new();
    /// let mut builder = RouterBuilder::new(app_state, observer);
    /// let state_handle = builder.enable_state_updates(1000);
    ///
    /// let router = builder
    ///     // .get("/api/data", handler)
    ///     .build();
    ///
    /// // External component can now update state:
    /// state_handle.update(|state| {
    ///     state.counter += 1;
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn enable_state_updates(&mut self, buffer_size: usize) -> StateUpdateHandle<S> {
        self.router.enable_state_updates(buffer_size)
    }

    /// Get a state update handle if state updates are enabled
    ///
    /// Returns None if enable_state_updates() has not been called.
    pub fn state_update_handle(&self) -> Option<StateUpdateHandle<S>> {
        self.router.state_update_handle()
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
            Some(o) => o.ok(),
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

        log::debug!("Processing ObserverRequest for path: {}", request.path);
        match self.lookup_observer_handler(&request.path) {
            Some(handler) => {
                log::debug!("Handler found for route: {:?}", &request.path);

                let packet = Packet::default();
                let mut raw = CoapRequest::from_packet(packet, request.source);
                // Set the path in the request for proper parameter extraction
                raw.set_path(&request.path);

                let mut coap_request: CoapumRequest<SocketAddr> = raw.into();
                // Identity should be empty or properly set - not the path
                coap_request.identity = String::new();

                Box::pin(async move { handler.call_erased(coap_request, state).await })
            }
            None => {
                log::debug!("No observer handler found for: {}", request.path);

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
        #[allow(dead_code)]
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
