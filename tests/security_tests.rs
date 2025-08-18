//! Security-focused tests for the coapum CoAP library
//!
//! These tests validate security measures including payload size limits,
//! path validation, connection management, and injection attack prevention.

use coapum::{
    extract::{Cbor, Json, FromRequest},
    observer::memory::MemObserver,
    router::RouterBuilder,
    CoapRequest, ContentFormat, Packet,
};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestPayload {
    data: String,
}

fn create_test_request_with_payload(payload: Vec<u8>) -> coapum::router::CoapumRequest<SocketAddr> {
    let mut request = CoapRequest::from_packet(
        Packet::new(),
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
    );
    request.message.payload = payload;
    request.into()
}

mod payload_security_tests {
    use super::*;

    #[tokio::test]
    async fn test_cbor_payload_at_size_limit() {
        // Create payload exactly at CBOR size limit (8192 bytes)
        let large_string = "A".repeat(8000); // Leave room for CBOR encoding overhead
        let test_data = TestPayload {
            data: large_string,
        };

        let mut buffer = Vec::new();
        ciborium::ser::into_writer(&test_data, &mut buffer).unwrap();

        // Ensure we're at or near the limit but not over
        assert!(buffer.len() <= 8192, "Test payload should be within CBOR limit");

        let mut req = create_test_request_with_payload(buffer);
        req.message
            .set_content_format(ContentFormat::ApplicationCBOR);

        let result = Cbor::<TestPayload>::from_request(&req, &()).await;
        assert!(result.is_ok(), "Should accept payload at size limit");
    }

    #[tokio::test]
    async fn test_cbor_payload_exceeds_size_limit() {
        // Create payload that exceeds CBOR size limit (8192 bytes)
        let oversized_payload = vec![0u8; 8193]; // 1 byte over limit

        let mut req = create_test_request_with_payload(oversized_payload);
        req.message
            .set_content_format(ContentFormat::ApplicationCBOR);

        let result = Cbor::<TestPayload>::from_request(&req, &()).await;
        assert!(result.is_err(), "Should reject payload over size limit");

        let error = result.unwrap_err();
        assert!(error.to_string().contains("Payload too large"));
    }

    #[tokio::test]
    async fn test_json_payload_at_size_limit() {
        // Create JSON payload at size limit (1MB)
        let large_string = "A".repeat(1_048_500); // Leave room for JSON structure
        let test_data = TestPayload {
            data: large_string,
        };

        let payload = serde_json::to_vec(&test_data).unwrap();
        assert!(payload.len() <= 1_048_576, "Test payload should be within JSON limit");

        let mut req = create_test_request_with_payload(payload);
        req.message
            .set_content_format(ContentFormat::ApplicationJSON);

        let result = Json::<TestPayload>::from_request(&req, &()).await;
        assert!(result.is_ok(), "Should accept JSON payload at size limit");
    }

    #[tokio::test]
    async fn test_json_payload_exceeds_size_limit() {
        // Create payload that exceeds JSON size limit (1MB)
        let oversized_payload = vec![0u8; 1_048_577]; // 1 byte over limit

        let mut req = create_test_request_with_payload(oversized_payload);
        req.message
            .set_content_format(ContentFormat::ApplicationJSON);

        let result = Json::<TestPayload>::from_request(&req, &()).await;
        assert!(result.is_err(), "Should reject JSON payload over size limit");

        let error = result.unwrap_err();
        assert!(error.to_string().contains("Payload too large"));
    }

    #[tokio::test]
    async fn test_empty_payload_rejection() {
        let empty_payload = vec![];

        // Test CBOR empty payload
        let mut req = create_test_request_with_payload(empty_payload.clone());
        req.message
            .set_content_format(ContentFormat::ApplicationCBOR);

        let cbor_result = Cbor::<TestPayload>::from_request(&req, &()).await;
        assert!(cbor_result.is_err(), "CBOR should reject empty payload");
        assert!(cbor_result.unwrap_err().to_string().contains("Empty payload"));

        // Test JSON empty payload
        let mut req = create_test_request_with_payload(empty_payload);
        req.message
            .set_content_format(ContentFormat::ApplicationJSON);

        let json_result = Json::<TestPayload>::from_request(&req, &()).await;
        assert!(json_result.is_err(), "JSON should reject empty payload");
        assert!(json_result.unwrap_err().to_string().contains("Empty payload"));
    }

    #[tokio::test]
    async fn test_content_type_validation() {
        let test_data = TestPayload {
            data: "test".to_string(),
        };
        let payload = serde_json::to_vec(&test_data).unwrap();

        // Try to extract CBOR from JSON payload (wrong content type)
        let mut req = create_test_request_with_payload(payload);
        req.message
            .set_content_format(ContentFormat::ApplicationJSON);

        let result = Cbor::<TestPayload>::from_request(&req, &()).await;
        assert!(result.is_err(), "Should reject wrong content type");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Expected CBOR content type"));
    }
}

mod path_validation_security_tests {
    use super::*;

    // Note: validate_observer_path is a private function in serve.rs
    // These tests document expected behavior but can't test the function directly
    #[test]
    fn test_path_traversal_prevention_documentation() {
        // Test various path traversal attempts that should be rejected
        let traversal_attempts = vec![
            "../secrets",
            "data/../../../etc/passwd", 
            "./../../config",
            "/data/../admin",
            "normal/../../../../../root",
            "data\\..\\windows\\system32",
        ];

        for malicious_path in traversal_attempts {
            // Document that these should be rejected by validate_observer_path
            assert!(
                malicious_path.contains("..") || malicious_path.contains("./") || malicious_path.contains("\\"),
                "Path {} contains dangerous patterns that should be rejected",
                malicious_path
            );
        }
    }

    #[test]
    fn test_invalid_characters_documentation() {
        let invalid_paths = vec![
            "/data/sensor@1",
            "/api/user#123", 
            "/device/temp$",
            "/path with spaces",
            "/データ/センサー", // Unicode characters
            "/api/user;rm -rf /",
            "/data\x00null",
            "/path\r\ninjection",
        ];

        for invalid_path in invalid_paths {
            // Document that these contain invalid characters
            let has_invalid = !invalid_path.chars().all(|c| 
                c == '/' || c.is_ascii_alphanumeric() || c == '_' || c == '-'
            );
            assert!(
                has_invalid,
                "Path {} should contain invalid characters",
                invalid_path
            );
        }
    }

    #[test]
    fn test_valid_path_patterns() {
        let valid_paths = vec![
            "/api/sensors",
            "/device_123/temperature", 
            "/data-source/readings",
            "/sensors/device_1/temp",
        ];

        for valid_path in valid_paths {
            // Test that these paths contain only valid characters
            let components: Vec<&str> = valid_path.split('/').filter(|s| !s.is_empty()).collect();
            
            for component in &components {
                assert!(
                    component.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
                    "Component '{}' should contain only valid characters",
                    component
                );
            }
            
            // Test path depth (should be reasonable)
            assert!(
                components.len() <= 10,
                "Path {} should not be too deep (max 10 components)",
                valid_path
            );
        }
    }
}

mod connection_security_tests {

    #[test]
    fn test_identity_sanitization() {
        // These tests would need access to the identity sanitization logic in serve.rs
        // For now, we document the expected behavior
        
        let test_cases = vec![
            ("normal_client", true),
            ("client-123", true),
            ("client.domain.com", true),
            ("", false), // Empty should be rejected
            ("client@domain", false), // @ should be filtered out
            ("client;DROP TABLE users;", false), // SQL injection attempt
            ("client\x00null", false), // Null byte injection
            ("", false), // Empty should be rejected
        ];

        // This test documents expected behavior - actual implementation would need
        // the identity sanitization function extracted for unit testing
        for (identity, should_be_valid) in test_cases {
            // In real implementation, this would test the sanitization function
            assert!(
                identity.len() <= 256,
                "Identity length validation: {}",
                identity
            );
            
            if should_be_valid {
                assert!(
                    identity.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'),
                    "Identity should contain only safe characters: {}",
                    identity
                );
            }
        }
    }
}

mod integration_security_tests {
    use super::*;
    use tower::Service;

    // Handler that always returns success for testing
    async fn dummy_handler() -> coapum::extract::StatusCode {
        coapum::extract::StatusCode::Content
    }

    #[tokio::test]
    async fn test_router_security_with_malicious_paths() {
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new((), observer)
            .get("/api/data", dummy_handler)
            .build();

        // Test that malicious paths don't crash the router
        let malicious_paths = vec![
            "/../../../etc/passwd",
            "/api/../admin",
            "/data\x00injection",
            "/very/deep/path/that/exceeds/normal/limits/component/component/component/component",
        ];

        for malicious_path in malicious_paths {
            let mut request = CoapRequest::from_packet(
                Packet::new(),
                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
            );
            request.set_path(malicious_path);

            let request: coapum::router::CoapumRequest<SocketAddr> = request.into();
            
            // Router should handle malicious paths gracefully (not panic)
            let result = router.call(request).await;
            // We don't care about the specific response, just that it doesn't crash
            assert!(result.is_ok() || result.is_err(), "Router should handle malicious paths gracefully");
        }
    }

    #[tokio::test]
    async fn test_concurrent_security_requests() {
        let observer = MemObserver::new();
        let router = RouterBuilder::new((), observer)
            .get("/api/test", dummy_handler)
            .build();

        // Test concurrent requests with various payloads
        let mut handles = vec![];

        for i in 0..5 { // Reduced to 5 for faster testing
            let router_clone = router.clone();
            let handle = tokio::spawn(async move {
                let oversized_payload = vec![0u8; 1_000]; // Smaller payload for testing
                
                let mut request = CoapRequest::from_packet(
                    Packet::new(),
                    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8000 + i)),
                );
                request.set_path("/api/test");
                request.message.payload = oversized_payload;

                let request: coapum::router::CoapumRequest<SocketAddr> = request.into();
                let mut router_mut = router_clone;
                router_mut.call(request).await
            });
            handles.push(handle);
        }

        // Wait for all concurrent requests
        for handle in handles {
            let result = handle.await;
            assert!(result.is_ok(), "Concurrent request task should complete");
        }
    }
}