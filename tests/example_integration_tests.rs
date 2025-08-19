//! Integration tests based on examples
//!
//! These tests convert the examples into runnable integration tests
//! to ensure they work correctly and provide coverage.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use coapum::{
    extract::{Cbor, Json, State, StatusCode},
    observer::memory::MemObserver,
    router::RouterBuilder,
};
use serde::{Deserialize, Serialize};
use tower::Service;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct SensorReading {
    temperature: f32,
    humidity: f32,
    timestamp: u64,
}

#[derive(Debug, Clone)]
struct ExampleAppState {
    readings: Arc<std::sync::Mutex<Vec<SensorReading>>>,
}

impl AsRef<ExampleAppState> for ExampleAppState {
    fn as_ref(&self) -> &ExampleAppState {
        self
    }
}

// Based on cbor_server.rs example
async fn get_sensor_readings(State(state): State<ExampleAppState>) -> Cbor<Vec<SensorReading>> {
    let readings = state.readings.lock().unwrap();
    Cbor(readings.clone())
}

async fn post_sensor_reading(
    Cbor(reading): Cbor<SensorReading>,
    State(state): State<ExampleAppState>,
) -> StatusCode {
    let mut readings = state.readings.lock().unwrap();
    readings.push(reading);
    StatusCode::Created
}

// Based on raw_server.rs example
async fn get_status() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "uptime": 12345,
        "version": "1.0.0"
    }))
}

async fn health_check() -> StatusCode {
    StatusCode::Content
}

#[tokio::test]
async fn test_cbor_server_example_functionality() {
    let app_state = ExampleAppState {
        readings: Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(app_state.clone(), observer)
        .get("/sensors", get_sensor_readings)
        .post("/sensors", post_sensor_reading)
        .build();

    // Test POST to add sensor reading
    let test_reading = SensorReading {
        temperature: 22.5,
        humidity: 60.0,
        timestamp: 1234567890,
    };

    let mut cbor_data = Vec::new();
    ciborium::ser::into_writer(&test_reading, &mut cbor_data).unwrap();

    let post_request = coapum::test_utils::create_test_request_with_content(
        "/sensors",
        cbor_data,
        coapum::ContentFormat::ApplicationCBOR,
    );

    let response = router.call(post_request).await.unwrap();
    assert_eq!(*response.get_status(), coapum::ResponseType::Created);

    // Test GET to retrieve readings
    let get_request = coapum::test_utils::create_test_request("/sensors");
    let response = router.call(get_request).await.unwrap();
    assert_eq!(*response.get_status(), coapum::ResponseType::Content);

    // Verify the reading was stored
    let readings: Vec<SensorReading> =
        ciborium::de::from_reader(&response.message.payload[..]).unwrap();
    assert_eq!(readings.len(), 1);
    assert_eq!(readings[0], test_reading);
}

#[tokio::test]
async fn test_raw_server_example_functionality() {
    let app_state = ExampleAppState {
        readings: Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(app_state, observer)
        .get("/status", get_status)
        .get("/health", health_check)
        .build();

    // Test status endpoint
    let status_request = coapum::test_utils::create_test_request("/status");
    let response = router.call(status_request).await.unwrap();
    assert_eq!(*response.get_status(), coapum::ResponseType::Content);

    let status_data: serde_json::Value = serde_json::from_slice(&response.message.payload).unwrap();
    assert_eq!(status_data["status"], "ok");
    assert_eq!(status_data["version"], "1.0.0");

    // Test health endpoint
    let health_request = coapum::test_utils::create_test_request("/health");
    let response = router.call(health_request).await.unwrap();
    assert_eq!(*response.get_status(), coapum::ResponseType::Content);
}

#[tokio::test]
async fn test_concurrency_example_simulation() {
    let app_state = ExampleAppState {
        readings: Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    let observer = MemObserver::new();
    let router = RouterBuilder::new(app_state.clone(), observer)
        .post("/sensors", post_sensor_reading)
        .get("/sensors", get_sensor_readings)
        .build();

    // Simulate concurrent requests like in concurrency.rs example
    let mut handles = Vec::new();

    for i in 0..5 {
        let mut router_clone = router.clone();
        let handle = tokio::spawn(async move {
            let reading = SensorReading {
                temperature: 20.0 + i as f32,
                humidity: 50.0 + i as f32,
                timestamp: 1234567890 + i as u64,
            };

            let mut cbor_data = Vec::new();
            ciborium::ser::into_writer(&reading, &mut cbor_data).unwrap();

            let request = coapum::test_utils::create_test_request_with_content(
                "/sensors",
                cbor_data,
                coapum::ContentFormat::ApplicationCBOR,
            );

            router_clone.call(request).await
        });
        handles.push(handle);
    }

    // Wait for all concurrent requests
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(*response.get_status(), coapum::ResponseType::Created);
    }

    // Verify all readings were stored
    let readings = app_state.readings.lock().unwrap();
    assert_eq!(readings.len(), 5);
}

#[tokio::test]
async fn test_client_server_interaction_simulation() {
    // Simulates the client-server interaction from cbor_client.rs and cbor_server.rs
    let app_state = ExampleAppState {
        readings: Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(app_state.clone(), observer)
        .get("/sensors", get_sensor_readings)
        .post("/sensors", post_sensor_reading)
        .build();

    // Simulate client sending multiple readings
    let client_readings = vec![
        SensorReading {
            temperature: 18.0,
            humidity: 45.0,
            timestamp: 1000,
        },
        SensorReading {
            temperature: 22.0,
            humidity: 55.0,
            timestamp: 2000,
        },
        SensorReading {
            temperature: 25.0,
            humidity: 65.0,
            timestamp: 3000,
        },
    ];

    // Post each reading
    for reading in &client_readings {
        let mut cbor_data = Vec::new();
        ciborium::ser::into_writer(reading, &mut cbor_data).unwrap();

        let request = coapum::test_utils::create_test_request_with_content(
            "/sensors",
            cbor_data,
            coapum::ContentFormat::ApplicationCBOR,
        );

        let response = router.call(request).await.unwrap();
        assert_eq!(*response.get_status(), coapum::ResponseType::Created);
    }

    // Client retrieves all readings
    let get_request = coapum::test_utils::create_test_request("/sensors");
    let response = router.call(get_request).await.unwrap();
    assert_eq!(*response.get_status(), coapum::ResponseType::Content);

    let server_readings: Vec<SensorReading> =
        ciborium::de::from_reader(&response.message.payload[..]).unwrap();
    assert_eq!(server_readings.len(), 3);

    // Verify readings match what client sent
    for (sent, received) in client_readings.iter().zip(server_readings.iter()) {
        assert_eq!(sent, received);
    }
}

#[tokio::test]
async fn test_error_handling_in_examples() {
    let app_state = ExampleAppState {
        readings: Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(app_state, observer)
        .post("/sensors", post_sensor_reading)
        .build();

    // Test with invalid CBOR data
    let invalid_cbor = vec![0xFF, 0xFF, 0xFF];
    let request = coapum::test_utils::create_test_request_with_content(
        "/sensors",
        invalid_cbor,
        coapum::ContentFormat::ApplicationCBOR,
    );

    let response = router.call(request).await.unwrap();
    // Should handle the error gracefully
    assert_ne!(*response.get_status(), coapum::ResponseType::Created);
}

#[tokio::test]
async fn test_timeout_and_reliability() {
    let app_state = ExampleAppState {
        readings: Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(app_state, observer)
        .get("/sensors", get_sensor_readings)
        .build();

    // Test with timeout to simulate network conditions
    let request = coapum::test_utils::create_test_request("/sensors");

    let response = timeout(Duration::from_secs(5), router.call(request)).await;
    assert!(response.is_ok(), "Request should complete within timeout");

    let response = response.unwrap().unwrap();
    assert_eq!(*response.get_status(), coapum::ResponseType::Content);
}
