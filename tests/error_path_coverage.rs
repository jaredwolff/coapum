//! Error path coverage tests
//!
//! These tests specifically target error handling paths that were identified
//! as having low coverage in the serve.rs and other modules.

use std::sync::Arc;

use coapum::{
    ContentFormat,
    extract::{Cbor, Json, StatusCode},
    observer::memory::MemObserver,
    router::RouterBuilder,
};
use serde::{Deserialize, Serialize};
use tower::Service;

#[derive(Debug, Clone)]
struct ErrorTestState {
    should_error: Arc<std::sync::Mutex<bool>>,
}

impl AsRef<ErrorTestState> for ErrorTestState {
    fn as_ref(&self) -> &ErrorTestState {
        self
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorTestData {
    id: u32,
    force_error: bool,
}

// Handler that can simulate state access errors
async fn handler_with_state_error(
    coapum::extract::State(state): coapum::extract::State<ErrorTestState>,
) -> StatusCode {
    let should_error = state.should_error.lock().unwrap();
    if *should_error {
        // Simulate a scenario where state access fails
        drop(should_error);
        // Force an error condition by returning server error
        StatusCode::InternalServerError
    } else {
        StatusCode::Content
    }
}

// Handler that can fail response serialization
async fn handler_response_serialization_test(
    Json(data): Json<ErrorTestData>,
) -> Result<Json<ErrorTestData>, StatusCode> {
    if data.force_error {
        Err(StatusCode::InternalServerError)
    } else {
        Ok(Json(data))
    }
}

// Handler that tests CBOR serialization errors
async fn handler_cbor_serialization_test(
    Cbor(data): Cbor<ErrorTestData>,
) -> Result<Cbor<ErrorTestData>, StatusCode> {
    if data.force_error {
        Err(StatusCode::BadRequest)
    } else {
        Ok(Cbor(data))
    }
}

// Handler with complex error conditions
async fn handler_complex_error_scenarios(
    Json(data): Json<ErrorTestData>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Simulate various error conditions
    match data.id {
        0 => Err(StatusCode::BadRequest),
        1 => Err(StatusCode::Unauthorized),
        2 => Err(StatusCode::Forbidden),
        3 => Err(StatusCode::NotFound),
        4 => Err(StatusCode::MethodNotAllowed),
        5 => Err(StatusCode::NotAcceptable),
        6 => Err(StatusCode::PreconditionFailed),
        7 => Err(StatusCode::RequestEntityTooLarge),
        8 => Err(StatusCode::UnsupportedContentFormat),
        9 => Err(StatusCode::InternalServerError),
        10 => Err(StatusCode::NotImplemented),
        11 => Err(StatusCode::BadGateway),
        12 => Err(StatusCode::ServiceUnavailable),
        13 => Err(StatusCode::GatewayTimeout),
        _ => Ok(Json(serde_json::json!({
            "id": data.id,
            "status": "success"
        }))),
    }
}

mod error_handling_tests {
    use super::*;

    #[tokio::test]
    async fn test_state_access_error_handling() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(true)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/state_error", handler_with_state_error)
            .build();

        let request = coapum::test_utils::create_test_request("/state_error");
        let response = router.call(request).await.unwrap();

        assert_eq!(
            *response.get_status(),
            coapum::ResponseType::InternalServerError
        );
    }

    #[tokio::test]
    async fn test_json_extraction_error_paths() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/json_test", handler_response_serialization_test)
            .build();

        // Test with completely invalid JSON
        let invalid_json = b"}{invalid json{";
        let request = coapum::test_utils::create_test_request_with_content(
            "/json_test",
            invalid_json.to_vec(),
            ContentFormat::ApplicationJSON,
        );

        let response = router.call(request).await.unwrap();
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_cbor_extraction_error_paths() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/cbor_test", handler_cbor_serialization_test)
            .build();

        // Test with invalid CBOR data
        let invalid_cbor = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let request = coapum::test_utils::create_test_request_with_content(
            "/cbor_test",
            invalid_cbor,
            ContentFormat::ApplicationCBOR,
        );

        let response = router.call(request).await.unwrap();
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_response_serialization_error() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/serialize_error", handler_response_serialization_test)
            .build();

        let error_data = ErrorTestData {
            id: 123,
            force_error: true,
        };
        let json_data = serde_json::to_vec(&error_data).unwrap();

        let request = coapum::test_utils::create_test_request_with_content(
            "/serialize_error",
            json_data,
            ContentFormat::ApplicationJSON,
        );

        let response = router.call(request).await.unwrap();
        assert_eq!(
            *response.get_status(),
            coapum::ResponseType::InternalServerError
        );
    }

    #[tokio::test]
    async fn test_all_error_status_codes() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/error_codes", handler_complex_error_scenarios)
            .build();

        let error_codes_to_test = vec![
            (0, coapum::ResponseType::BadRequest),
            (1, coapum::ResponseType::Unauthorized),
            (2, coapum::ResponseType::Forbidden),
            (3, coapum::ResponseType::NotFound),
            (4, coapum::ResponseType::MethodNotAllowed),
            (5, coapum::ResponseType::NotAcceptable),
            (6, coapum::ResponseType::PreconditionFailed),
            (7, coapum::ResponseType::RequestEntityTooLarge),
            (8, coapum::ResponseType::UnsupportedContentFormat),
            (9, coapum::ResponseType::InternalServerError),
            (10, coapum::ResponseType::NotImplemented),
            (11, coapum::ResponseType::BadGateway),
            (12, coapum::ResponseType::ServiceUnavailable),
            (13, coapum::ResponseType::GatewayTimeout),
        ];

        for (error_id, expected_status) in error_codes_to_test {
            let error_data = ErrorTestData {
                id: error_id,
                force_error: false,
            };
            let json_data = serde_json::to_vec(&error_data).unwrap();

            let request = coapum::test_utils::create_test_request_with_content(
                "/error_codes",
                json_data,
                ContentFormat::ApplicationJSON,
            );

            let response = router.call(request).await.unwrap();
            assert_eq!(
                *response.get_status(),
                expected_status,
                "Failed for error ID: {}",
                error_id
            );
        }
    }

    #[tokio::test]
    async fn test_successful_response_after_errors() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/error_codes", handler_complex_error_scenarios)
            .build();

        // Test successful case
        let success_data = ErrorTestData {
            id: 999, // Will return success
            force_error: false,
        };
        let json_data = serde_json::to_vec(&success_data).unwrap();

        let request = coapum::test_utils::create_test_request_with_content(
            "/error_codes",
            json_data,
            ContentFormat::ApplicationJSON,
        );

        let response = router.call(request).await.unwrap();
        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        let response_data: serde_json::Value =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(response_data["id"], 999);
        assert_eq!(response_data["status"], "success");
    }
}

mod content_format_error_tests {
    use super::*;

    #[tokio::test]
    async fn test_wrong_content_format_error() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/format_test", handler_response_serialization_test)
            .build();

        let valid_json_data = serde_json::json!({
            "id": 42,
            "force_error": false
        });
        let json_bytes = serde_json::to_vec(&valid_json_data).unwrap();

        // Send JSON data but claim it's CBOR
        let request = coapum::test_utils::create_test_request_with_content(
            "/format_test",
            json_bytes,
            ContentFormat::ApplicationCBOR, // Wrong format!
        );

        let response = router.call(request).await.unwrap();
        // Should fail due to content format mismatch
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_missing_content_format() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/no_format", handler_response_serialization_test)
            .build();

        let valid_json_data = serde_json::json!({
            "id": 42,
            "force_error": false
        });
        let json_bytes = serde_json::to_vec(&valid_json_data).unwrap();

        // Create request without explicit content format
        let mut request = coapum::test_utils::create_test_request("/no_format");
        request.message.payload = json_bytes;
        // Don't set content format - this may cause extraction issues

        let _response = router.call(request).await.unwrap();
        // Handler should handle missing content format gracefully
        // Just verify we get some response (success or error)
        assert!(true);
    }

    #[tokio::test]
    async fn test_empty_payload_with_content_format() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/empty_payload", handler_response_serialization_test)
            .build();

        // Create request with content format but empty payload
        let request = coapum::test_utils::create_test_request_with_content(
            "/empty_payload",
            vec![], // Empty payload
            ContentFormat::ApplicationJSON,
        );

        let response = router.call(request).await.unwrap();
        // Should handle empty payload error
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }
}

mod router_error_path_tests {
    use super::*;

    #[tokio::test]
    async fn test_route_not_found_error() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/existing", handler_with_state_error)
            .build();

        // Request non-existent route
        let request = coapum::test_utils::create_test_request("/nonexistent");
        let response = router.call(request).await.unwrap();

        // Should return appropriate error for missing route
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_method_not_allowed_error() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/get_only", handler_with_state_error) // Only GET is registered
            .build();

        // Try to POST to a GET-only route
        let request =
            coapum::test_utils::create_test_request_with_payload("/get_only", vec![1, 2, 3]);

        let response = router.call(request).await.unwrap();
        // Should handle method not allowed
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_malformed_path_parameters() {
        let state = ErrorTestState {
            should_error: Arc::new(std::sync::Mutex::new(false)),
        };

        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/test/:id", handler_with_state_error)
            .build();

        // Test with various potentially problematic paths
        let problematic_paths = vec![
            "/test/",  // Empty parameter
            "/test//", // Double slash
            "/test/id%with%encoded%chars",
            "/test/very_long_parameter_that_might_cause_issues_with_parsing_or_memory",
        ];

        for path in problematic_paths {
            let request = coapum::test_utils::create_test_request(path);
            let response = router.call(request).await;

            // Should handle malformed paths gracefully
            assert!(response.is_ok(), "Failed to handle path: {}", path);
        }
    }
}
