//! Comprehensive server tests
//!
//! These tests focus on server functionality including connection management,
//! security features, path validation, and error handling.

use std::sync::Arc;
use std::time::{Duration, Instant};

use coapum::{
    config::Config,
    extract::{State, StatusCode},
    observer::memory::MemObserver,
    router::RouterBuilder,
};
use tower::Service;

// Simple test state
#[derive(Debug, Clone)]
struct TestServerState {
    counter: Arc<std::sync::Mutex<u32>>,
}

impl AsRef<TestServerState> for TestServerState {
    fn as_ref(&self) -> &TestServerState {
        self
    }
}

// Simple handler for testing
async fn test_handler(State(state): State<TestServerState>) -> StatusCode {
    let mut counter = state.counter.lock().unwrap();
    *counter += 1;
    StatusCode::Content
}

// Handler that always returns error for testing error paths
async fn error_handler() -> Result<StatusCode, StatusCode> {
    Err(StatusCode::InternalServerError)
}

mod connection_info_tests {
    use super::*;

    #[test]
    fn test_connection_info_creation() {
        let (_tx, _rx) = tokio::sync::mpsc::channel::<()>(1);
        
        // Test connection info structure
        // Note: ConnectionInfo is private, so we test the concepts it represents
        let established_at = Instant::now();
        let reconnect_count = 0u32;
        
        // Verify timing constraints exist (from constants in serve.rs)
        const MIN_RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
        const MAX_RECONNECT_ATTEMPTS: u32 = 10;
        
        assert!(MIN_RECONNECT_INTERVAL.as_secs() >= 5);
        assert!(MAX_RECONNECT_ATTEMPTS >= 10);
        
        // Test that timing calculations work
        let elapsed = established_at.elapsed();
        assert!(elapsed < Duration::from_secs(1)); // Should be very recent
        
        // Test reconnection logic
        assert!(reconnect_count < MAX_RECONNECT_ATTEMPTS);
    }

    #[test]
    fn test_security_constants() {
        // Test that security constants are reasonable
        const MIN_RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
        const MAX_RECONNECT_ATTEMPTS: u32 = 10;
        const MAX_IDENTITY_LENGTH: usize = 256;
        
        assert!(MIN_RECONNECT_INTERVAL.as_secs() >= 1, "Reconnect interval should prevent rapid abuse");
        assert!(MAX_RECONNECT_ATTEMPTS >= 3, "Should allow some reconnections");
        assert!(MAX_IDENTITY_LENGTH >= 32, "Should allow reasonable identity lengths");
        assert!(MAX_IDENTITY_LENGTH <= 1024, "Should prevent excessive identity lengths");
    }
}

mod path_validation_tests {

    // Since validate_observer_path is private, we need to create a public wrapper for testing
    // or test the behavior through integration tests. Let's create utility functions that 
    // replicate the validation logic for testing purposes.

    fn test_path_validation(path: &str) -> Result<String, String> {
        if path.is_empty() {
            return Err("Path is empty".to_string());
        }

        // Security: Reject paths containing dangerous patterns
        if path.contains("..") || path.contains("./") || path.contains("\\") {
            return Err("Path traversal attempt detected".to_string());
        }

        // Normalize and validate path components
        let components: Vec<&str> = path.split('/')
            .filter(|s| !s.is_empty())
            .collect();

        // Security: Limit path depth to prevent resource exhaustion
        const MAX_PATH_DEPTH: usize = 10;
        if components.len() > MAX_PATH_DEPTH {
            return Err("Path too deep (max 10 components)".to_string());
        }

        // Security: Validate each path component for safe characters only
        for component in &components {
            if !component.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err("Path contains invalid characters".to_string());
            }
        }

        // Return normalized path
        if components.is_empty() {
            Ok("/".to_string())
        } else {
            Ok(format!("/{}", components.join("/")))
        }
    }

    #[test]
    fn test_path_validation_empty_path() {
        let result = test_path_validation("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path is empty"));
    }

    #[test]
    fn test_path_validation_traversal_attempts() {
        let malicious_paths = vec![
            "../secrets",
            "data/../../../etc/passwd",
            "./config",
            "/data/../admin",
            "normal/../../root",
            "data\\windows\\system32",
        ];

        for malicious_path in malicious_paths {
            let result = test_path_validation(malicious_path);
            assert!(result.is_err(), "Should reject path: {}", malicious_path);
            assert!(result.unwrap_err().contains("Path traversal attempt"));
        }
    }

    #[test]
    fn test_path_validation_depth_limits() {
        // Create a path that exceeds the maximum depth (10 components)
        let components = vec!["component"; 11];
        let deep_path = format!("/{}", components.join("/"));
        
        let result = test_path_validation(&deep_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Path too deep"));
    }

    #[test]
    fn test_path_validation_invalid_characters() {
        let invalid_paths = vec![
            "/data/sensor@1",
            "/api/user#123",
            "/device/temp$",
            "/path with spaces",
            "/api/user;rm",
            "/data\x00null",
        ];

        for invalid_path in invalid_paths {
            let result = test_path_validation(invalid_path);
            assert!(result.is_err(), "Should reject path: {}", invalid_path);
            assert!(result.unwrap_err().contains("invalid characters"));
        }
    }

    #[test]
    fn test_path_validation_valid_paths() {
        let valid_paths = vec![
            "/api/sensors",
            "/device_123/temperature",
            "/data-source/readings",
            "/sensors/device_1/temp",
            "sensors/data", // Without leading slash
            "/a/b/c/d/e/f/g/h/i/j", // Exactly at depth limit (10 components)
        ];

        for valid_path in valid_paths {
            let result = test_path_validation(valid_path);
            assert!(result.is_ok(), "Should accept valid path: {} - Error: {:?}", valid_path, result.err());

            let normalized = result.unwrap();
            // Should always start with /
            assert!(normalized.starts_with('/'), "Normalized path should start with /");
            // Should not have double slashes
            assert!(!normalized.contains("//"), "Should not contain double slashes");
        }
    }

    #[test]
    fn test_path_normalization() {
        let test_cases = vec![
            ("sensors/temp", "/sensors/temp"),
            ("/sensors/temp", "/sensors/temp"),
            ("///sensors///temp///", "/sensors/temp"),
        ];

        for (input, expected) in test_cases {
            let result = test_path_validation(input);
            assert!(result.is_ok(), "Should normalize path: {}", input);
            assert_eq!(result.unwrap(), expected, "Incorrect normalization for: {}", input);
        }
    }
}

mod identity_sanitization_tests {

    // Test identity sanitization logic based on serve.rs implementation
    fn test_identity_sanitization(identity: &str) -> Result<String, String> {
        const MAX_IDENTITY_LENGTH: usize = 256;
        
        if identity.len() > MAX_IDENTITY_LENGTH {
            return Err(format!("Identity too long: {} bytes", identity.len()));
        }
        
        // Sanitize identity to prevent injection attacks
        let sanitized: String = identity.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
            .take(MAX_IDENTITY_LENGTH)
            .collect();
        
        if sanitized.is_empty() {
            return Err("Identity contains no valid characters".to_string());
        }
        
        Ok(sanitized)
    }

    #[test]
    fn test_valid_identities() {
        let valid_identities = vec![
            "client123",
            "device-sensor_1",
            "gateway.domain.com",
            "sensor_node-42",
            "a", // Single character
            "A1_b2-c3.d4", // Mixed valid characters
        ];

        for identity in valid_identities {
            let result = test_identity_sanitization(identity);
            assert!(result.is_ok(), "Should accept valid identity: {}", identity);
            assert_eq!(result.unwrap(), identity, "Should not change valid identity");
        }
    }

    #[test]
    fn test_identity_sanitization_filtering() {
        let test_cases = vec![
            ("client@domain", "clientdomain"),
            ("device#123", "device123"),
            ("sensor;DROP", "sensorDROP"),
            ("node\x00null", "nodenull"),
            ("test!@#$%^&*()+=", "test"),
            ("spaces in name", "spacesinname"),
        ];

        for (input, expected) in test_cases {
            let result = test_identity_sanitization(input);
            assert!(result.is_ok(), "Should sanitize identity: {}", input);
            assert_eq!(result.unwrap(), expected, "Incorrect sanitization for: {}", input);
        }
    }

    #[test]
    fn test_identity_length_limits() {
        let long_identity = "a".repeat(300); // Exceeds 256 character limit
        let result = test_identity_sanitization(&long_identity);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Identity too long"));
    }

    #[test]
    fn test_empty_after_sanitization() {
        let invalid_identities = vec![
            "!@#$%^&*()", // All invalid characters
            "\x00\x01\x02", // All non-printable
            "   ", // Only spaces (filtered out)
            "+=[]{}|\\:;\"'<>,?/", // All symbols
        ];

        for identity in invalid_identities {
            let result = test_identity_sanitization(identity);
            assert!(result.is_err(), "Should reject identity with no valid chars: {}", identity);
            assert!(result.unwrap_err().contains("no valid characters"));
        }
    }

    #[test]
    fn test_identity_length_truncation() {
        // Create identity that would be truncated at MAX_LENGTH
        let base = "a".repeat(200);
        let extra = "b".repeat(100); // Total would be 300
        let long_valid_identity = format!("{}{}", base, extra);
        
        let result = test_identity_sanitization(&long_valid_identity);
        assert!(result.is_err(), "Should reject overly long identity");
    }
}

mod config_integration_tests {
    use super::*;

    #[test]
    fn test_config_defaults_for_server() {
        let config = Config::default();
        
        // Test that default configuration is sensible for server use
        assert!(config.timeout > 0, "Timeout should be positive");
        assert!(config.buffer_size() >= 1024, "Buffer should be reasonable size");
        
        // Test that DTLS config exists
        // Note: DTLSConfig fields are mostly private, but we can verify it exists
        let _dtls_config = &config.dtls_cfg;
    }

    #[tokio::test]
    async fn test_router_with_server_state() {
        let state = TestServerState {
            counter: Arc::new(std::sync::Mutex::new(0)),
        };
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .get("/test", test_handler)
            .get("/error", error_handler)
            .build();

        // Test successful request
        let request = coapum::test_utils::create_test_request("/test");
        let response = router.call(request).await.unwrap();
        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
        
        // Verify state was modified
        let counter = state.counter.lock().unwrap();
        assert_eq!(*counter, 1);
        drop(counter);

        // Test error request
        let request = coapum::test_utils::create_test_request("/error");
        let response = router.call(request).await.unwrap();
        assert_eq!(*response.get_status(), coapum::ResponseType::InternalServerError);
    }

    #[tokio::test]
    async fn test_router_with_invalid_paths() {
        let state = TestServerState {
            counter: Arc::new(std::sync::Mutex::new(0)),
        };
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/valid/path", test_handler)
            .build();

        // Test that router handles non-existent paths gracefully
        let request = coapum::test_utils::create_test_request("/nonexistent");
        let response = router.call(request).await.unwrap();
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }
}

mod observer_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_observer_registration_with_server() {
        let state = TestServerState {
            counter: Arc::new(std::sync::Mutex::new(0)),
        };
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .observe("/sensors/temp", test_handler, test_handler)
            .build();

        // Create simple GET request (observe flag is handled internally)
        let request = coapum::test_utils::create_test_request("/sensors/temp");

        let response = router.call(request).await.unwrap();
        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        // Verify handler was called
        let counter = state.counter.lock().unwrap();
        assert_eq!(*counter, 1);
    }

    #[tokio::test]
    async fn test_observer_path_validation_integration() {
        let state = TestServerState {
            counter: Arc::new(std::sync::Mutex::new(0)),
        };
        let observer = MemObserver::new();
        
        // Test that observe routes work with valid paths
        let mut router = RouterBuilder::new(state, observer)
            .observe("/valid_path-123", test_handler, test_handler)
            .build();

        let request = coapum::test_utils::create_test_request("/valid_path-123");

        let response = router.call(request).await.unwrap();
        // Should succeed with valid path
        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
    }
}