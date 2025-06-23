//! Payload extraction for CoAP requests
//!
//! This module provides extractors for different payload formats commonly used
//! in CoAP applications, including CBOR, JSON, and raw bytes.

use super::{FromRequest, IntoResponse, ResponseError, StatusCode};
use crate::router::CoapumRequest;
use async_trait::async_trait;
use coap_lite::{ContentFormat, ResponseType};
use serde::{Deserialize, Serialize};
use std::{fmt, net::SocketAddr};

/// Extract raw bytes from the request payload
///
/// This is the most basic payload extractor that simply returns the raw bytes
/// from the CoAP message payload.
///
/// # Example
///
/// ```rust
/// use coapum::extract::Bytes;
///
/// async fn handle_raw_data(payload: Bytes) {
///     println!("Received {} bytes", payload.len());
/// }
/// ```
pub struct Bytes(pub Vec<u8>);

impl fmt::Debug for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Bytes")
            .field(&format!("{} bytes", self.0.len()))
            .finish()
    }
}

impl Clone for Bytes {
    fn clone(&self) -> Self {
        Bytes(self.0.clone())
    }
}

impl std::ops::Deref for Bytes {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Bytes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(bytes: Vec<u8>) -> Self {
        Bytes(bytes)
    }
}

impl From<Bytes> for Vec<u8> {
    fn from(bytes: Bytes) -> Self {
        bytes.0
    }
}

#[async_trait]
impl<S> FromRequest<S> for Bytes {
    type Rejection = std::convert::Infallible;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Bytes(req.message.payload.clone()))
    }
}

impl IntoResponse for Bytes {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        let packet = crate::Packet::new();
        let mut response = crate::CoapResponse::new(&packet).ok_or_else(|| {
            ResponseError::InvalidResponse("Failed to create response".to_string())
        })?;
        response.message.payload = self.0;
        response.set_status(ResponseType::Content);
        Ok(response)
    }
}

/// Extract and serialize CBOR payloads
///
/// This extractor automatically deserializes CBOR payloads into the specified type
/// and can serialize responses back to CBOR format.
///
/// # Example
///
/// ```rust
/// use coapum::extract::Cbor;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Deserialize, Serialize)]
/// struct DeviceState {
///     temperature: f32,
///     humidity: f32,
/// }
///
/// async fn handle_device_state(Cbor(state): Cbor<DeviceState>) -> Cbor<DeviceState> {
///     println!("Temperature: {}°C", state.temperature);
///     Cbor(state)
/// }
/// ```
pub struct Cbor<T>(pub T);

impl<T> fmt::Debug for Cbor<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Cbor").field(&self.0).finish()
    }
}

impl<T> Clone for Cbor<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Cbor(self.0.clone())
    }
}

impl<T> std::ops::Deref for Cbor<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Cbor<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Rejection type for CBOR extraction failures
#[derive(Debug)]
pub struct CborRejection {
    kind: CborRejectionKind,
}

#[derive(Debug)]
enum CborRejectionKind {
    InvalidCborData { error: String },
    MissingCborContentType,
    EmptyPayload,
}

impl fmt::Display for CborRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            CborRejectionKind::InvalidCborData { error } => {
                write!(f, "Invalid CBOR data: {}", error)
            }
            CborRejectionKind::MissingCborContentType => {
                write!(f, "Expected CBOR content type")
            }
            CborRejectionKind::EmptyPayload => {
                write!(f, "Empty payload")
            }
        }
    }
}

impl std::error::Error for CborRejection {}

impl IntoResponse for CborRejection {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        match self.kind {
            CborRejectionKind::InvalidCborData { .. } => StatusCode::BadRequest.into_response(),
            CborRejectionKind::MissingCborContentType => {
                StatusCode::UnsupportedContentFormat.into_response()
            }
            CborRejectionKind::EmptyPayload => StatusCode::BadRequest.into_response(),
        }
    }
}

#[async_trait]
impl<T, S> FromRequest<S> for Cbor<T>
where
    T: for<'de> Deserialize<'de> + Send,
    S: Send + Sync,
{
    type Rejection = CborRejection;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        if req.message.payload.is_empty() {
            return Err(CborRejection {
                kind: CborRejectionKind::EmptyPayload,
            });
        }

        // Check content format if available
        if let Some(content_format) = req.message.get_content_format() {
            match content_format {
                ContentFormat::ApplicationCBOR => {}
                _ => {
                    return Err(CborRejection {
                        kind: CborRejectionKind::MissingCborContentType,
                    });
                }
            }
        }

        // Deserialize CBOR data
        let value =
            ciborium::de::from_reader(&req.message.payload[..]).map_err(|e| CborRejection {
                kind: CborRejectionKind::InvalidCborData {
                    error: e.to_string(),
                },
            })?;

        Ok(Cbor(value))
    }
}

impl<T> IntoResponse for Cbor<T>
where
    T: Serialize,
{
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        let packet = crate::Packet::new();
        let mut response = crate::CoapResponse::new(&packet).ok_or_else(|| {
            ResponseError::InvalidResponse("Failed to create response".to_string())
        })?;

        let mut buffer = Vec::new();
        ciborium::ser::into_writer(&self.0, &mut buffer).map_err(|e| {
            ResponseError::SerializationError(format!("CBOR serialization failed: {}", e))
        })?;

        response.message.payload = buffer;
        response
            .message
            .set_content_format(ContentFormat::ApplicationCBOR);
        response.set_status(ResponseType::Content);
        Ok(response)
    }
}

/// Extract and serialize JSON payloads
///
/// This extractor automatically deserializes JSON payloads into the specified type
/// and can serialize responses back to JSON format.
///
/// # Example
///
/// ```rust
/// use coapum::extract::Json;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Deserialize, Serialize)]
/// struct ApiRequest {
///     action: String,
///     data: serde_json::Value,
/// }
///
/// async fn handle_api_request(Json(req): Json<ApiRequest>) -> Json<serde_json::Value> {
///     Json(serde_json::json!({"result": "success", "action": req.action}))
/// }
/// ```
pub struct Json<T>(pub T);

impl<T> fmt::Debug for Json<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Json").field(&self.0).finish()
    }
}

impl<T> Clone for Json<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Json(self.0.clone())
    }
}

impl<T> std::ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Rejection type for JSON extraction failures
#[derive(Debug)]
pub struct JsonRejection {
    kind: JsonRejectionKind,
}

#[derive(Debug)]
enum JsonRejectionKind {
    InvalidJsonData { error: String },
    MissingJsonContentType,
    EmptyPayload,
}

impl fmt::Display for JsonRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            JsonRejectionKind::InvalidJsonData { error } => {
                write!(f, "Invalid JSON data: {}", error)
            }
            JsonRejectionKind::MissingJsonContentType => {
                write!(f, "Expected JSON content type")
            }
            JsonRejectionKind::EmptyPayload => {
                write!(f, "Empty payload")
            }
        }
    }
}

impl std::error::Error for JsonRejection {}

impl IntoResponse for JsonRejection {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        match self.kind {
            JsonRejectionKind::InvalidJsonData { .. } => StatusCode::BadRequest.into_response(),
            JsonRejectionKind::MissingJsonContentType => {
                StatusCode::UnsupportedContentFormat.into_response()
            }
            JsonRejectionKind::EmptyPayload => StatusCode::BadRequest.into_response(),
        }
    }
}

#[async_trait]
impl<T, S> FromRequest<S> for Json<T>
where
    T: for<'de> Deserialize<'de> + Send,
    S: Send + Sync,
{
    type Rejection = JsonRejection;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        if req.message.payload.is_empty() {
            return Err(JsonRejection {
                kind: JsonRejectionKind::EmptyPayload,
            });
        }

        // Check content format if available
        if let Some(content_format) = req.message.get_content_format() {
            match content_format {
                ContentFormat::ApplicationJSON => {}
                _ => {
                    return Err(JsonRejection {
                        kind: JsonRejectionKind::MissingJsonContentType,
                    });
                }
            }
        }

        // Deserialize JSON data
        let value = serde_json::from_slice(&req.message.payload).map_err(|e| JsonRejection {
            kind: JsonRejectionKind::InvalidJsonData {
                error: e.to_string(),
            },
        })?;

        Ok(Json(value))
    }
}

impl<T> IntoResponse for Json<T>
where
    T: Serialize,
{
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        let packet = crate::Packet::new();
        let mut response = crate::CoapResponse::new(&packet).ok_or_else(|| {
            ResponseError::InvalidResponse("Failed to create response".to_string())
        })?;

        let payload = serde_json::to_vec(&self.0).map_err(|e| {
            ResponseError::SerializationError(format!("JSON serialization failed: {}", e))
        })?;

        response.message.payload = payload;
        response
            .message
            .set_content_format(ContentFormat::ApplicationJSON);
        response.set_status(ResponseType::Content);
        Ok(response)
    }
}

/// Raw payload extractor that preserves all request metadata
///
/// This extractor provides access to the raw CoAP request for cases where
/// you need fine-grained control over the response construction.
pub struct Raw {
    pub payload: Vec<u8>,
    pub content_format: Option<ContentFormat>,
}

impl fmt::Debug for Raw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Raw")
            .field("payload_len", &self.payload.len())
            .field("content_format", &self.content_format)
            .finish()
    }
}

#[async_trait]
impl<S> FromRequest<S> for Raw {
    type Rejection = std::convert::Infallible;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Raw {
            payload: req.message.payload.clone(),
            content_format: req.message.get_content_format(),
        })
    }
}

impl IntoResponse for Raw {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        let packet = crate::Packet::new();
        let mut response = crate::CoapResponse::new(&packet).ok_or_else(|| {
            ResponseError::InvalidResponse("Failed to create response".to_string())
        })?;

        response.message.payload = self.payload;
        if let Some(content_format) = self.content_format {
            response.message.set_content_format(content_format);
        }
        response.set_status(ResponseType::Content);
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoapRequest, Packet};
    use serde::{Deserialize, Serialize};
    use std::net::{Ipv4Addr, SocketAddrV4};

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }

    fn create_test_request_with_payload(payload: Vec<u8>) -> CoapumRequest<SocketAddr> {
        let mut request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );
        request.message.payload = payload;
        request.into()
    }

    #[tokio::test]
    async fn test_bytes_extraction() {
        let payload = vec![1, 2, 3, 4, 5];
        let req = create_test_request_with_payload(payload.clone());

        let result = Bytes::from_request(&req, &()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, payload);
    }

    #[tokio::test]
    async fn test_cbor_extraction_success() {
        let test_data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let mut buffer = Vec::new();
        ciborium::ser::into_writer(&test_data, &mut buffer).unwrap();

        let mut req = create_test_request_with_payload(buffer);
        req.message
            .set_content_format(ContentFormat::ApplicationCBOR);

        let result = Cbor::<TestData>::from_request(&req, &()).await;
        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert_eq!(extracted.name, "test");
        assert_eq!(extracted.value, 42);
    }

    #[tokio::test]
    async fn test_cbor_extraction_invalid_data() {
        let req = create_test_request_with_payload(vec![0xFF, 0xFF, 0xFF]);

        let result = Cbor::<TestData>::from_request(&req, &()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_json_extraction_success() {
        let test_data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let payload = serde_json::to_vec(&test_data).unwrap();
        let mut req = create_test_request_with_payload(payload);
        req.message
            .set_content_format(ContentFormat::ApplicationJSON);

        let result = Json::<TestData>::from_request(&req, &()).await;
        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert_eq!(extracted.name, "test");
        assert_eq!(extracted.value, 42);
    }

    #[tokio::test]
    async fn test_json_extraction_invalid_data() {
        let req = create_test_request_with_payload(vec![0xFF, 0xFF, 0xFF]);

        let result = Json::<TestData>::from_request(&req, &()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_raw_extraction() {
        let payload = vec![1, 2, 3, 4, 5];
        let mut req = create_test_request_with_payload(payload.clone());
        req.message
            .set_content_format(ContentFormat::ApplicationCBOR);

        let result = Raw::from_request(&req, &()).await;
        assert!(result.is_ok());
        let raw = result.unwrap();
        assert_eq!(raw.payload, payload);
        assert_eq!(raw.content_format, Some(ContentFormat::ApplicationCBOR));
    }

    #[tokio::test]
    async fn test_cbor_response() {
        let test_data = TestData {
            name: "response".to_string(),
            value: 123,
        };

        let cbor = Cbor(test_data.clone());
        let response = cbor.into_response().unwrap();

        assert_eq!(*response.get_status(), ResponseType::Content);
        assert_eq!(
            response.message.get_content_format(),
            Some(ContentFormat::ApplicationCBOR)
        );

        // Verify we can deserialize the response payload
        let deserialized: TestData =
            ciborium::de::from_reader(&response.message.payload[..]).unwrap();
        assert_eq!(deserialized, test_data);
    }

    #[tokio::test]
    async fn test_json_response() {
        let test_data = TestData {
            name: "response".to_string(),
            value: 123,
        };

        let json = Json(test_data.clone());
        let response = json.into_response().unwrap();

        assert_eq!(*response.get_status(), ResponseType::Content);
        assert_eq!(
            response.message.get_content_format(),
            Some(ContentFormat::ApplicationJSON)
        );

        // Verify we can deserialize the response payload
        let deserialized: TestData = serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(deserialized, test_data);
    }
}
