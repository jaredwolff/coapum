//! Path parameter extraction for CoAP routes
//!
//! This module provides the `Path` extractor for extracting parameters from
//! wildcard routes like ".d/*" and ".s/*" commonly used in IoT applications.

use super::{FromRequest, IntoResponse, ResponseError, StatusCode};
use crate::router::CoapumRequest;
use async_trait::async_trait;

use std::{fmt, net::SocketAddr};

/// Extract path parameters from wildcard routes
///
/// This extractor handles IoT-specific routing patterns like:
/// - `.d/*` for device routes where `*` is the device ID
/// - `.s/*` for stream routes where `*` is the stream ID
/// - Custom patterns with named parameters
///
/// # Examples
///
/// ```rust
/// use coapum::extract::Path;
/// use serde::Deserialize;
///
/// // Extract device ID from ".d/device123" -> "device123"
/// async fn handle_device(Path(device_id): Path<String>) {
///     println!("Device: {}", device_id);
/// }
///
/// // Extract multiple parameters
/// #[derive(Deserialize)]
/// struct DeviceParams {
///     device_id: String,
///     property: String,
/// }
///
/// async fn handle_device_property(Path(params): Path<DeviceParams>) {
///     println!("Device: {}, Property: {}", params.device_id, params.property);
/// }
/// ```
pub struct Path<T>(pub T);

impl<T> fmt::Debug for Path<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Path").field(&self.0).finish()
    }
}

impl<T> Clone for Path<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Path(self.0.clone())
    }
}

impl<T> Copy for Path<T> where T: Copy {}

impl<T> std::ops::Deref for Path<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Path<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Rejection type for path extraction failures
#[derive(Debug)]
pub struct PathRejection {
    kind: PathRejectionKind,
}

#[derive(Debug)]
enum PathRejectionKind {
    FailedToDeserializePathParams { key: String },
    MissingPathParams,
    InvalidPathPattern,
}

impl fmt::Display for PathRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            PathRejectionKind::FailedToDeserializePathParams { key } => {
                write!(f, "Failed to deserialize path parameter `{}`", key)
            }
            PathRejectionKind::MissingPathParams => {
                write!(f, "Missing path parameters")
            }
            PathRejectionKind::InvalidPathPattern => {
                write!(f, "Invalid path pattern")
            }
        }
    }
}

impl std::error::Error for PathRejection {}

impl IntoResponse for PathRejection {
    fn into_response(self) -> Result<crate::CoapResponse, ResponseError> {
        StatusCode::BadRequest.into_response()
    }
}

#[async_trait]
impl<S> FromRequest<S> for Path<String> {
    type Rejection = PathRejection;

    async fn from_request(
        req: &CoapumRequest<SocketAddr>,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let path = req.get_path();

        // Extract parameter from common IoT patterns
        let param = extract_wildcard_param(path).ok_or(PathRejection {
            kind: PathRejectionKind::MissingPathParams,
        })?;

        Ok(Path(param))
    }
}

/// Extract wildcard parameter from IoT-specific path patterns
///
/// Supports patterns like:
/// - `.d/device123` -> `device123`
/// - `.s/stream456` -> `stream456`
/// - `devices/device123` -> `device123`
/// - `api/v1/devices/device123` -> `device123` (extracts last segment)
fn extract_wildcard_param(path: &str) -> Option<String> {
    // Remove leading slash if present
    let path = path.strip_prefix('/').unwrap_or(path);

    // Handle common IoT patterns
    if let Some(param) = path.strip_prefix(".d/") {
        return Some(param.to_string());
    }

    if let Some(param) = path.strip_prefix(".s/") {
        return Some(param.to_string());
    }

    // For other patterns, extract the last segment
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() >= 2 {
        Some(segments.last()?.to_string())
    } else {
        None
    }
}

/// Extract multiple path parameters from a structured path
///
/// This is a more advanced version that can handle paths like:
/// `/devices/{device_id}/properties/{property_name}`
fn extract_path_params(path: &str, pattern: &str) -> Option<Vec<(String, String)>> {
    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let pattern_segments: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();

    if path_segments.len() != pattern_segments.len() {
        return None;
    }

    let mut params = Vec::new();

    for (path_seg, pattern_seg) in path_segments.iter().zip(pattern_segments.iter()) {
        if pattern_seg.starts_with('{') && pattern_seg.ends_with('}') {
            let param_name = &pattern_seg[1..pattern_seg.len() - 1];
            params.push((param_name.to_string(), path_seg.to_string()));
        } else if path_seg != pattern_seg {
            return None;
        }
    }

    Some(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoapRequest, Packet};
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn create_test_request(path: &str) -> CoapumRequest<SocketAddr> {
        let mut request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );
        request.set_path(path);
        request.into()
    }

    #[test]
    fn test_extract_wildcard_param() {
        assert_eq!(
            extract_wildcard_param(".d/device123"),
            Some("device123".to_string())
        );
        assert_eq!(
            extract_wildcard_param(".s/stream456"),
            Some("stream456".to_string())
        );
        assert_eq!(
            extract_wildcard_param("devices/device123"),
            Some("device123".to_string())
        );
        assert_eq!(
            extract_wildcard_param("api/v1/devices/device123"),
            Some("device123".to_string())
        );
        assert_eq!(extract_wildcard_param("empty"), None);
        assert_eq!(extract_wildcard_param(""), None);
    }

    #[tokio::test]
    async fn test_path_extraction() {
        let req = create_test_request(".d/device123");
        let result = Path::<String>::from_request(&req, &()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, "device123");
    }

    #[tokio::test]
    async fn test_path_extraction_failure() {
        let req = create_test_request("invalid");
        let result = Path::<String>::from_request(&req, &()).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_extract_path_params() {
        let params = extract_path_params(
            "/devices/device123/properties/temperature",
            "/devices/{device_id}/properties/{property_name}",
        );

        assert!(params.is_some());
        let params = params.unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(
            params[0],
            ("device_id".to_string(), "device123".to_string())
        );
        assert_eq!(
            params[1],
            ("property_name".to_string(), "temperature".to_string())
        );
    }

    #[test]
    fn test_extract_path_params_mismatch() {
        let params = extract_path_params(
            "/devices/device123",
            "/devices/{device_id}/properties/{property_name}",
        );

        assert!(params.is_none());
    }
}
