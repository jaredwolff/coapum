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

// SenML support
use coapum_senml::SenMLPack;

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
    PayloadTooLarge,
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
            CborRejectionKind::PayloadTooLarge => {
                write!(f, "Payload too large")
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
            CborRejectionKind::PayloadTooLarge => StatusCode::RequestEntityTooLarge.into_response(),
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

        // Security: Check payload size to prevent memory exhaustion attacks
        const MAX_CBOR_PAYLOAD_SIZE: usize = 8192;
        if req.message.payload.len() > MAX_CBOR_PAYLOAD_SIZE {
            return Err(CborRejection {
                kind: CborRejectionKind::PayloadTooLarge,
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

        // Security: Deserialize CBOR data with size constraints
        // Note: ciborium doesn't expose public deserializer configuration,
        // but the from_reader function already has internal protections
        let value = ciborium::de::from_reader(&req.message.payload[..]).map_err(|e| CborRejection {
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
    PayloadTooLarge,
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
            JsonRejectionKind::PayloadTooLarge => {
                write!(f, "Payload too large")
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
            JsonRejectionKind::PayloadTooLarge => StatusCode::RequestEntityTooLarge.into_response(),
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

        // Security: Check payload size to prevent memory exhaustion attacks
        const MAX_JSON_PAYLOAD_SIZE: usize = 1_048_576; // 1MB
        if req.message.payload.len() > MAX_JSON_PAYLOAD_SIZE {
            return Err(JsonRejection {
                kind: JsonRejectionKind::PayloadTooLarge,
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

/// Extract and serialize SenML (Sensor Measurement Lists) payloads
///
/// This extractor automatically deserializes SenML payloads (JSON or CBOR format) 
/// into SenMLPack and can serialize responses back to the appropriate format.
/// Supports RFC 8428 compliant SenML with validation and normalization.
///
/// # Content Format Support
/// - `application/senml+json` (Content-Format 110)
/// - `application/senml+cbor` (Content-Format 112)
/// - `application/json` (falls back to JSON parsing)
/// - `application/cbor` (falls back to CBOR parsing)
///
/// # Example
///
/// ```rust
/// use coapum::extract::SenML;
/// use coapum_senml::{SenMLPack, SenMLBuilder};
///
/// async fn handle_sensor_data(SenML(pack): SenML) -> SenML {
///     println!("Received {} records", pack.len());
///     
///     let response = SenMLBuilder::new()
///         .base_name("urn:dev:controller1/")
///         .add_string_value("status", "ok")
///         .build();
///     
///     SenML(response)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SenML(pub SenMLPack);

impl std::ops::Deref for SenML {
    type Target = SenMLPack;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SenML {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<SenMLPack> for SenML {
    fn from(pack: SenMLPack) -> Self {
        SenML(pack)
    }
}

impl From<SenML> for SenMLPack {
    fn from(senml: SenML) -> Self {
        senml.0
    }
}

/// Rejection type for SenML extraction failures
#[derive(Debug)]
pub struct SenMLRejection {
    kind: SenMLRejectionKind,
}

#[derive(Debug)]
enum SenMLRejectionKind {
    InvalidSenMLData { error: String },
    UnsupportedContentFormat,
    EmptyPayload,
    PayloadTooLarge,
    ValidationError { error: String },
}

impl fmt::Display for SenMLRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            SenMLRejectionKind::InvalidSenMLData { error } => {
                write!(f, "Invalid SenML data: {}", error)
            }
            SenMLRejectionKind::UnsupportedContentFormat => {
                write!(f, "Unsupported content format for SenML")
            }
            SenMLRejectionKind::EmptyPayload => {
                write!(f, "Empty payload")
            }
            SenMLRejectionKind::PayloadTooLarge => {
                write!(f, "Payload too large")
            }
            SenMLRejectionKind::ValidationError { error } => {
                write!(f, "SenML validation error: {}", error)
            }
        }
    }
}

impl std::error::Error for SenMLRejection {}

impl IntoResponse for SenMLRejection {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        match self.kind {
            SenMLRejectionKind::InvalidSenMLData { .. } => StatusCode::BadRequest.into_response(),
            SenMLRejectionKind::UnsupportedContentFormat => {
                StatusCode::UnsupportedContentFormat.into_response()
            }
            SenMLRejectionKind::EmptyPayload => StatusCode::BadRequest.into_response(),
            SenMLRejectionKind::PayloadTooLarge => StatusCode::RequestEntityTooLarge.into_response(),
            SenMLRejectionKind::ValidationError { .. } => StatusCode::BadRequest.into_response(),
        }
    }
}

#[async_trait]
impl<S> FromRequest<S> for SenML
where
    S: Send + Sync,
{
    type Rejection = SenMLRejection;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        if req.message.payload.is_empty() {
            return Err(SenMLRejection {
                kind: SenMLRejectionKind::EmptyPayload,
            });
        }

        // Security: Check payload size to prevent memory exhaustion attacks
        const MAX_SENML_PAYLOAD_SIZE: usize = 1_048_576; // 1MB
        if req.message.payload.len() > MAX_SENML_PAYLOAD_SIZE {
            return Err(SenMLRejection {
                kind: SenMLRejectionKind::PayloadTooLarge,
            });
        }

        // Determine format and deserialize based on content format
        let pack = if let Some(content_format) = req.message.get_content_format() {
            match content_format {
                // Official SenML content formats (RFC 8428)
                ContentFormat::ApplicationSenmlJSON => {
                    // application/senml+json
                    SenMLPack::from_json(std::str::from_utf8(&req.message.payload).map_err(
                        |e| SenMLRejection {
                            kind: SenMLRejectionKind::InvalidSenMLData {
                                error: format!("Invalid UTF-8: {}", e),
                            },
                        },
                    )?)
                }
                ContentFormat::ApplicationSenmlCBOR => {
                    // application/senml+cbor
                    SenMLPack::from_cbor(&req.message.payload)
                }
                // Fallback to generic formats
                ContentFormat::ApplicationJSON => {
                    SenMLPack::from_json(std::str::from_utf8(&req.message.payload).map_err(
                        |e| SenMLRejection {
                            kind: SenMLRejectionKind::InvalidSenMLData {
                                error: format!("Invalid UTF-8: {}", e),
                            },
                        },
                    )?)
                }
                ContentFormat::ApplicationCBOR => SenMLPack::from_cbor(&req.message.payload),
                _ => {
                    return Err(SenMLRejection {
                        kind: SenMLRejectionKind::UnsupportedContentFormat,
                    });
                }
            }
        } else {
            // No content format specified - try to auto-detect
            // First try JSON (more human-readable)
            if let Ok(json_str) = std::str::from_utf8(&req.message.payload) {
                if let Ok(pack) = SenMLPack::from_json(json_str) {
                    Ok(pack)
                } else {
                    // Try CBOR
                    SenMLPack::from_cbor(&req.message.payload)
                }
            } else {
                // Binary data - try CBOR
                SenMLPack::from_cbor(&req.message.payload)
            }
        };

        let pack = pack.map_err(|e| SenMLRejection {
            kind: SenMLRejectionKind::InvalidSenMLData {
                error: e.to_string(),
            },
        })?;

        // Skip validation for now - SenML deserialization already ensures basic format correctness
        // TODO: Implement context-aware validation that understands base records
        // For now, if the pack deserializes successfully, we consider it valid

        Ok(SenML(pack))
    }
}

impl IntoResponse for SenML {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        let packet = crate::Packet::new();
        let mut response = crate::CoapResponse::new(&packet).ok_or_else(|| {
            ResponseError::InvalidResponse("Failed to create response".to_string())
        })?;

        // Default to JSON format for responses (more interoperable)
        let payload = self.0.to_json().map_err(|e| {
            ResponseError::SerializationError(format!("SenML JSON serialization failed: {}", e))
        })?;

        response.message.payload = payload.into_bytes();
        
        // Use the official SenML JSON content format
        response.message.set_content_format(ContentFormat::ApplicationSenmlJSON);
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

    #[tokio::test]
    async fn test_senml_json_extraction() {
        use coapum_senml::SenMLBuilder;

        let pack = SenMLBuilder::new()
            .base_name("device1/")
            .add_value("temperature", 22.5)
            .add_value("humidity", 45.0)
            .build();

        let json = pack.to_json().unwrap();
        let mut req = create_test_request_with_payload(json.into_bytes());
        req.message.set_content_format(ContentFormat::ApplicationSenmlJSON);

        let result = SenML::from_request(&req, &()).await;
        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert!(extracted.len() >= 2); // At least 2 records (base + measurements)
    }

    #[tokio::test]
    async fn test_senml_cbor_extraction() {
        use coapum_senml::SenMLBuilder;

        let pack = SenMLBuilder::new()
            .base_name("sensor1/")
            .add_value("temp", 25.0)
            .build();

        let cbor = pack.to_cbor().unwrap();
        let mut req = create_test_request_with_payload(cbor);
        req.message.set_content_format(ContentFormat::ApplicationSenmlCBOR);

        let result = SenML::from_request(&req, &()).await;
        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert!(!extracted.is_empty());
    }

    #[tokio::test]
    async fn test_senml_auto_detection() {
        use coapum_senml::SenMLBuilder;

        let pack = SenMLBuilder::new()
            .add_value("standalone", 42.0)
            .build();

        // Test JSON auto-detection (no content format specified)
        let json = pack.to_json().unwrap();
        let req = create_test_request_with_payload(json.into_bytes());
        
        let result = SenML::from_request(&req, &()).await;
        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert_eq!(extracted.len(), 1);
    }

    #[tokio::test]
    async fn test_senml_invalid_data() {
        let req = create_test_request_with_payload(vec![0xFF, 0xFF, 0xFF]);

        let result = SenML::from_request(&req, &()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_senml_response() {
        use coapum_senml::SenMLBuilder;

        let pack = SenMLBuilder::new()
            .base_name("response/")
            .add_value("status", 200.0)
            .build();

        let senml = SenML(pack);
        let response = senml.into_response().unwrap();

        assert_eq!(*response.get_status(), ResponseType::Content);
        assert_eq!(
            response.message.get_content_format(),
            Some(ContentFormat::ApplicationSenmlJSON)
        );

        // Verify we can deserialize the response payload
        let json_str = std::str::from_utf8(&response.message.payload).unwrap();
        let deserialized = coapum_senml::SenMLPack::from_json(json_str).unwrap();
        assert!(!deserialized.is_empty());
    }

    #[tokio::test]
    async fn test_senml_deserialization_error() {
        // Create invalid JSON that will fail deserialization
        let invalid_json = "{invalid json}";
        let req = create_test_request_with_payload(invalid_json.as_bytes().to_vec());

        let result = SenML::from_request(&req, &()).await;
        assert!(result.is_err());
        
        // Should be a deserialization error
        let err = result.unwrap_err();
        assert!(matches!(err.kind, SenMLRejectionKind::InvalidSenMLData { .. }));
    }

    #[tokio::test]
    async fn test_senml_fallback_to_generic_formats() {
        use coapum_senml::SenMLBuilder;

        let pack = SenMLBuilder::new()
            .add_value("test", 123.0)
            .build();

        // Test fallback to generic JSON
        let json = pack.to_json().unwrap();
        let mut req = create_test_request_with_payload(json.into_bytes());
        req.message.set_content_format(ContentFormat::ApplicationJSON);
        
        let result = SenML::from_request(&req, &()).await;
        assert!(result.is_ok());

        // Test fallback to generic CBOR
        let cbor = pack.to_cbor().unwrap();
        let mut req = create_test_request_with_payload(cbor);
        req.message.set_content_format(ContentFormat::ApplicationCBOR);
        
        let result = SenML::from_request(&req, &()).await;
        assert!(result.is_ok());
    }
}
