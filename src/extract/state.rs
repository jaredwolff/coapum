//! State and metadata extraction for CoAP requests
//!
//! This module provides extractors for accessing request metadata and application state,
//! including PSK identity, source address, observe flags, and shared application state.

use super::{FromRequest, IntoResponse, ResponseError, StatusCode};
use crate::router::CoapumRequest;
use async_trait::async_trait;
use coap_lite::ObserveOption;
use std::{fmt, net::SocketAddr};

/// Extract the PSK identity from the request
///
/// This extractor provides access to the Pre-Shared Key identity that was used
/// to establish the DTLS connection. This is commonly used for client identification
/// in IoT applications.
///
/// # Example
///
/// ```rust
/// use coapum::extract::Identity;
///
/// async fn handle_authenticated_request(Identity(client_id): Identity) {
///     println!("Request from client: {}", client_id);
/// }
/// ```
pub struct Identity(pub String);

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Identity").field(&self.0).finish()
    }
}

impl Clone for Identity {
    fn clone(&self) -> Self {
        Identity(self.0.clone())
    }
}

impl std::ops::Deref for Identity {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Identity {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[async_trait]
impl<S> FromRequest<S> for Identity {
    type Rejection = std::convert::Infallible;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Identity(req.identity.clone()))
    }
}

/// Extract the source address from the request
///
/// This extractor provides access to the network address (IP and port) of the
/// client that sent the request.
///
/// # Example
///
/// ```rust
/// use coapum::extract::Source;
/// use std::net::SocketAddr;
///
/// async fn handle_request_with_source(Source(addr): Source) {
///     println!("Request from: {}", addr);
/// }
/// ```
pub struct Source(pub SocketAddr);

impl fmt::Debug for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Source").field(&self.0).finish()
    }
}

impl Clone for Source {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for Source {}

impl std::ops::Deref for Source {
    type Target = SocketAddr;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<S> FromRequest<S> for Source {
    type Rejection = std::convert::Infallible;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let addr = req.source.unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
        Ok(Source(addr))
    }
}

/// Extract the CoAP observe flag from the request
///
/// This extractor provides access to the observe option in CoAP requests,
/// which is used for the observe pattern (server-sent notifications).
///
/// # Example
///
/// ```rust
/// use coapum::extract::ObserveFlag;
/// use coap_lite::ObserveOption;
///
/// async fn handle_observe_request(ObserveFlag(observe): ObserveFlag) {
///     match observe {
///         Some(ObserveOption::Register) => println!("Client wants to observe"),
///         Some(ObserveOption::Deregister) => println!("Client wants to stop observing"),
///         None => println!("Regular request"),
///     }
/// }
/// ```
pub struct ObserveFlag(pub Option<ObserveOption>);

impl fmt::Debug for ObserveFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ObserveFlag").field(&self.0).finish()
    }
}

impl Clone for ObserveFlag {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for ObserveFlag {}

impl std::ops::Deref for ObserveFlag {
    type Target = Option<ObserveOption>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[async_trait]
impl<S> FromRequest<S> for ObserveFlag {
    type Rejection = std::convert::Infallible;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(ObserveFlag(*req.get_observe_flag()))
    }
}

/// Extract shared application state
///
/// This extractor provides access to the shared application state that was
/// provided when creating the router. The state is automatically cloned for
/// each request, avoiding the need to manage Arc<Mutex<T>> manually.
///
/// ## Database Connection Patterns
///
/// The state system is designed to work seamlessly with database connection pools.
/// Since state is cloned for each request, database connections should use
/// connection pools wrapped in Arc for efficient sharing.
///
/// ### PostgreSQL Example (using sqlx)
///
/// ```rust,ignore
/// use coapum::extract::State;
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct AppState {
///     db: Arc<sqlx::PgPool>,
///     cache: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
/// }
///
/// async fn get_user_handler(State(state): State<AppState>) -> Result<String, Box<dyn std::error::Error>> {
///     let user = sqlx::query!("SELECT name FROM users WHERE id = $1", 1)
///         .fetch_one(&*state.db)
///         .await?;
///     Ok(user.name)
/// }
/// ```
///
/// ### SQLite Example (using sqlx)
///
/// ```rust,ignore
/// use coapum::{Json, extract::State};
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct AppState {
///     db: Arc<sqlx::SqlitePool>,
///     config: AppConfig,
/// }
///
/// async fn insert_data_handler(
///     State(state): State<AppState>,
///     Json(data): Json<serde_json::Value>
/// ) -> Result<(), Box<dyn std::error::Error>> {
///     sqlx::query!("INSERT INTO data (payload) VALUES (?)", data.to_string())
///         .execute(&*state.db)
///         .await?;
///     Ok(())
/// }
/// ```
///
/// ### Diesel Example
///
/// ```rust,ignore
/// use coapum::extract::State;
/// use std::sync::Arc;
/// use diesel::r2d2::{Pool, ConnectionManager};
/// use diesel::PgConnection;
///
/// type DbPool = Arc<Pool<ConnectionManager<PgConnection>>>;
///
/// #[derive(Clone)]
/// struct AppState {
///     db_pool: DbPool,
///     redis_client: Arc<redis::Client>,
/// }
///
/// async fn database_handler(State(state): State<AppState>) {
///     let mut conn = state.db_pool.get().expect("Failed to get connection");
///     // Use connection for database operations
/// }
/// ```
///
/// ### Generic Database Pattern
///
/// For maximum flexibility, you can define a trait for database operations:
///
/// ```rust,ignore
/// use async_trait::async_trait;
/// use coapum::extract::State;
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct User {
///     id: i32,
///     name: String,
/// }
///
/// #[derive(Clone)]
/// struct Cache {
///     // Cache implementation
/// }
///
/// #[async_trait]
/// pub trait DatabaseOps: Send + Sync + Clone {
///     type Error: std::error::Error + Send + Sync + 'static;
///     
///     async fn get_user(&self, id: i32) -> Result<User, Self::Error>;
///     async fn save_data(&self, data: &serde_json::Value) -> Result<(), Self::Error>;
/// }
///
/// #[derive(Clone)]
/// struct AppState<DB: DatabaseOps> {
///     db: DB,
///     cache: Arc<tokio::sync::RwLock<Cache>>,
/// }
///
/// async fn generic_handler<DB: DatabaseOps>(
///     State(state): State<AppState<DB>>
/// ) -> Result<(), DB::Error> {
///     let user = state.db.get_user(123).await?;
///     Ok(())
/// }
/// ```
///
/// ## Best Practices for State Management
///
/// 1. **Use Arc for shared resources**: Database pools, configuration, caches
/// 2. **Keep state lightweight**: Large objects should be behind Arc
/// 3. **Prefer connection pools**: For database connections, always use pools
/// 4. **Clone should be cheap**: State is cloned per request, so make it efficient
/// 5. **Consider read-heavy workloads**: Use `Arc<RwLock<T>>` for cached data that's read frequently
///
/// ## Basic Example
///
/// ```rust
/// use coapum::extract::State;
///
/// #[derive(Clone)]
/// struct AppState {
///     database_url: String,
///     api_key: String,
/// }
///
/// async fn handle_with_state(State(state): State<AppState>) {
///     println!("Database: {}", state.database_url);
/// }
/// ```
pub struct State<T>(pub T);

impl<T> fmt::Debug for State<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("State").field(&self.0).finish()
    }
}

impl<T> Clone for State<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        State(self.0.clone())
    }
}

impl<T> std::ops::Deref for State<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for State<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Rejection type for state extraction failures
#[derive(Debug)]
pub struct StateRejection {
    message: String,
}

impl fmt::Display for StateRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "State extraction failed: {}", self.message)
    }
}

impl std::error::Error for StateRejection {}

impl IntoResponse for StateRejection {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        StatusCode::InternalServerError.into_response()
    }
}

#[async_trait]
impl<T, S> FromRequest<S> for State<T>
where
    T: Clone + Send + Sync + 'static,
    S: AsRef<T> + Send + Sync,
{
    type Rejection = StateRejection;

    async fn from_request(
        _req: &CoapumRequest<SocketAddr>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(State(state.as_ref().clone()))
    }
}

/// Extract the full CoAP request for advanced use cases
///
/// This extractor provides access to the complete CoAP request structure
/// for cases where you need fine-grained control or access to fields not
/// covered by other extractors.
///
/// # Example
///
/// ```rust
/// use coapum::FullRequest;
///
/// async fn handle_full_request(FullRequest(req): FullRequest) {
///     println!("Method: {:?}", req.get_method());
///     println!("Path: {}", req.get_path());
///     println!("Message ID: {}", req.message.header.message_id);
/// }
/// ```
pub struct FullRequest(pub CoapumRequest<SocketAddr>);

impl fmt::Debug for FullRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("FullRequest")
            .field(&format!("CoapumRequest({})", self.0.get_path()))
            .finish()
    }
}

impl Clone for FullRequest {
    fn clone(&self) -> Self {
        FullRequest(self.0.clone())
    }
}

impl std::ops::Deref for FullRequest {
    type Target = CoapumRequest<SocketAddr>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for FullRequest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[async_trait]
impl<S> FromRequest<S> for FullRequest {
    type Rejection = std::convert::Infallible;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(FullRequest(req.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoapRequest, Packet};
    use coap_lite::RequestType;
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn create_test_request() -> CoapumRequest<SocketAddr> {
        let mut request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)),
        );
        request.set_method(RequestType::Get);
        request.set_path("test");

        let mut coap_request: CoapumRequest<SocketAddr> = request.into();
        coap_request.identity = "test_client".to_string();
        coap_request
    }

    #[tokio::test]
    async fn test_identity_extraction() {
        let req = create_test_request();
        let result = Identity::from_request(&req, &()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, "test_client");
    }

    #[tokio::test]
    async fn test_source_extraction() {
        let req = create_test_request();
        let result = Source::from_request(&req, &()).await;

        assert!(result.is_ok());
        let source = result.unwrap();
        assert_eq!(source.port(), 8080);
    }

    #[tokio::test]
    async fn test_observe_flag_extraction() {
        let req = create_test_request();
        let result = ObserveFlag::from_request(&req, &()).await;

        assert!(result.is_ok());
        let observe_flag = result.unwrap();
        assert!(observe_flag.is_none());
    }

    #[tokio::test]
    async fn test_state_extraction() {
        #[derive(Clone, Debug, PartialEq)]
        struct TestState {
            value: i32,
        }

        impl AsRef<TestState> for TestState {
            fn as_ref(&self) -> &TestState {
                self
            }
        }

        let req = create_test_request();
        let state = TestState { value: 42 };
        let result = State::<TestState>::from_request(&req, &state).await;

        assert!(result.is_ok());
        let extracted_state = result.unwrap();
        assert_eq!(extracted_state.value, 42);
    }

    #[tokio::test]
    async fn test_full_request_extraction() {
        let req = create_test_request();
        let result = FullRequest::from_request(&req, &()).await;

        assert!(result.is_ok());
        let full_request = result.unwrap();
        assert_eq!(full_request.get_path(), "test");
        assert_eq!(*full_request.get_method(), RequestType::Get);
        assert_eq!(full_request.identity, "test_client");
    }
}
