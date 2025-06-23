//! Ergonomic extractors for CoAP request handling
//!
//! This module provides a type-safe way to extract data from CoAP requests,
//! similar to Axum's extraction system but tailored for CoAP.

use async_trait::async_trait;
use coap_lite::ResponseType;
use std::{convert::Infallible, fmt, net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;

use crate::router::CoapumRequest;

pub mod path;
pub mod payload;
pub mod state;

pub use path::Path;
pub use payload::{Bytes, Cbor, Json, Raw};
pub use state::{Identity, ObserveFlag, Source, State};

/// Trait for extracting data from CoAP requests
///
/// Types that implement this trait can be used as handler function parameters
/// and will be automatically extracted from the incoming request.
#[async_trait]
pub trait FromRequest<S>: Sized {
    /// The error type returned when extraction fails
    type Rejection: IntoResponse;

    /// Extract this type from the request
    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        state: &S,
    ) -> Result<Self, Self::Rejection>;
}

/// Trait for converting values into CoAP responses
pub trait IntoResponse {
    /// Convert this value into a CoAP response
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError>;
}

/// Error types for response conversion
#[derive(Debug)]
pub enum ResponseError {
    SerializationError(String),
    InvalidResponse(String),
}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResponseError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            ResponseError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
        }
    }
}

impl std::error::Error for ResponseError {}

/// Standard CoAP status codes for responses
#[derive(Debug, Clone, Copy)]
pub enum StatusCode {
    Created,
    Deleted,
    Valid,
    Changed,
    Content,
    Continue,
    BadRequest,
    Unauthorized,
    BadOption,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    NotAcceptable,
    RequestEntityIncomplete,
    ConflictingResource,
    PreconditionFailed,
    RequestEntityTooLarge,
    UnsupportedContentFormat,
    UnprocessableEntity,
    InternalServerError,
    NotImplemented,
    BadGateway,
    ServiceUnavailable,
    GatewayTimeout,
    ProxyingNotSupported,
}

impl From<StatusCode> for ResponseType {
    fn from(status: StatusCode) -> Self {
        match status {
            StatusCode::Created => ResponseType::Created,
            StatusCode::Deleted => ResponseType::Deleted,
            StatusCode::Valid => ResponseType::Valid,
            StatusCode::Changed => ResponseType::Changed,
            StatusCode::Content => ResponseType::Content,
            StatusCode::Continue => ResponseType::Continue,
            StatusCode::BadRequest => ResponseType::BadRequest,
            StatusCode::Unauthorized => ResponseType::Unauthorized,
            StatusCode::BadOption => ResponseType::BadOption,
            StatusCode::Forbidden => ResponseType::Forbidden,
            StatusCode::NotFound => ResponseType::NotFound,
            StatusCode::MethodNotAllowed => ResponseType::MethodNotAllowed,
            StatusCode::NotAcceptable => ResponseType::NotAcceptable,
            StatusCode::RequestEntityIncomplete => ResponseType::RequestEntityIncomplete,
            StatusCode::ConflictingResource => ResponseType::PreconditionFailed,
            StatusCode::PreconditionFailed => ResponseType::PreconditionFailed,
            StatusCode::RequestEntityTooLarge => ResponseType::RequestEntityTooLarge,
            StatusCode::UnsupportedContentFormat => ResponseType::UnsupportedContentFormat,
            StatusCode::UnprocessableEntity => ResponseType::UnprocessableEntity,
            StatusCode::InternalServerError => ResponseType::InternalServerError,
            StatusCode::NotImplemented => ResponseType::NotImplemented,
            StatusCode::BadGateway => ResponseType::BadGateway,
            StatusCode::ServiceUnavailable => ResponseType::ServiceUnavailable,
            StatusCode::GatewayTimeout => ResponseType::GatewayTimeout,
            StatusCode::ProxyingNotSupported => ResponseType::ProxyingNotSupported,
        }
    }
}

impl IntoResponse for StatusCode {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        let packet = crate::Packet::new();
        let mut response = crate::CoapResponse::new(&packet).ok_or_else(|| {
            ResponseError::InvalidResponse("Failed to create response".to_string())
        })?;
        response.set_status(self.into());
        Ok(response)
    }
}

impl IntoResponse for () {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        StatusCode::Valid.into_response()
    }
}

impl IntoResponse for Infallible {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        match self {}
    }
}

impl<T> IntoResponse for Result<T, StatusCode>
where
    T: IntoResponse,
{
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        match self {
            Ok(value) => value.into_response(),
            Err(status) => status.into_response(),
        }
    }
}

/// Helper trait for converting handler functions
pub trait Handler<S, Args>: Clone + Send + Sized + 'static {
    /// The future returned by this handler
    type Future: std::future::Future<Output = Result<crate::CoapResponse, Infallible>>
        + Send
        + 'static;

    /// Call this handler with the given request and state
    fn call(self, req: CoapumRequest<SocketAddr>, state: Arc<Mutex<S>>) -> Self::Future;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_code_conversion() {
        assert_eq!(ResponseType::Valid, StatusCode::Valid.into());
        assert_eq!(ResponseType::BadRequest, StatusCode::BadRequest.into());
        assert_eq!(ResponseType::NotFound, StatusCode::NotFound.into());
    }

    #[tokio::test]
    async fn test_status_code_into_response() {
        let response = StatusCode::Valid.into_response().unwrap();
        assert_eq!(*response.get_status(), ResponseType::Valid);
    }

    #[tokio::test]
    async fn test_unit_into_response() {
        let response = ().into_response().unwrap();
        assert_eq!(*response.get_status(), ResponseType::Valid);
    }
}
