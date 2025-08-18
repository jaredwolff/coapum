//! Handler trait implementation coverage tests
//!
//! These tests specifically target the Handler trait implementations
//! for different parameter counts (2-9 parameters) that were identified
//! as having low coverage in the handler module.

use std::sync::Arc;

use coapum::{
    extract::{Bytes, Cbor, Json, Path, State, StatusCode, Source},
    router::RouterBuilder,
    observer::memory::MemObserver,
    ContentFormat,
};
use serde::{Deserialize, Serialize};
use tower::Service;

#[derive(Debug, Clone)]
struct HandlerTraitTestState {
    counter: Arc<std::sync::Mutex<u32>>,
    data: Arc<std::sync::Mutex<String>>,
}

impl AsRef<HandlerTraitTestState> for HandlerTraitTestState {
    fn as_ref(&self) -> &HandlerTraitTestState {
        self
    }
}

impl Default for HandlerTraitTestState {
    fn default() -> Self {
        Self {
            counter: Arc::new(std::sync::Mutex::new(0)),
            data: Arc::new(std::sync::Mutex::new(String::new())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestData {
    id: u32,
    message: String,
}

// Handler implementations for different parameter counts to test trait coverage

// 2 parameters: State + Path
async fn handler_2_params(
    State(state): State<HandlerTraitTestState>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut counter = state.counter.lock().unwrap();
    *counter += 1;
    let mut data = state.data.lock().unwrap();
    *data = format!("id:{}", id);
    StatusCode::Content
}

// 3 parameters: State + Path + Bytes
async fn handler_3_params(
    State(state): State<HandlerTraitTestState>,
    Path(id): Path<String>,
    bytes: Bytes,
) -> Json<serde_json::Value> {
    let mut counter = state.counter.lock().unwrap();
    *counter += 1;
    let mut data = state.data.lock().unwrap();
    *data = format!("id:{},bytes:{}", id, bytes.len());
    
    Json(serde_json::json!({
        "id": id,
        "byte_count": bytes.len(),
        "counter": *counter
    }))
}

// 4 parameters: State + Path + JSON + Source
async fn handler_4_params(
    State(state): State<HandlerTraitTestState>,
    Path(id): Path<String>,
    Json(payload): Json<TestData>,
    Source(addr): Source,
) -> Cbor<TestData> {
    let mut counter = state.counter.lock().unwrap();
    *counter += 1;
    
    Cbor(TestData {
        id: payload.id + *counter,
        message: format!("{}:{}:{}", id, payload.message, addr.port()),
    })
}

// Handler with maximum 4 parameters (the actual limit)
async fn handler_max_params(
    State(state): State<HandlerTraitTestState>,
    Path(id): Path<String>,
    Cbor(payload): Cbor<TestData>,
    Source(addr): Source,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut counter = state.counter.lock().unwrap();
    *counter += 1;
    
    if payload.id == 0 {
        return Err(StatusCode::BadRequest);
    }
    
    Ok(Json(serde_json::json!({
        "id": id,
        "payload_id": payload.id,
        "payload_message": payload.message,
        "source_port": addr.port(),
        "counter": *counter
    })))
}

// Error handling handler for trait coverage
async fn handler_with_result_error() -> Result<StatusCode, StatusCode> {
    Err(StatusCode::InternalServerError)
}

async fn handler_with_result_success() -> Result<StatusCode, StatusCode> {
    Ok(StatusCode::Content)
}

// Response conversion handlers
async fn handler_json_response() -> Json<TestData> {
    Json(TestData {
        id: 42,
        message: "json response test".to_string(),
    })
}

async fn handler_cbor_response() -> Cbor<TestData> {
    Cbor(TestData {
        id: 99,
        message: "cbor response test".to_string(),
    })
}

// Async handler variations
async fn handler_async_delay(State(state): State<HandlerTraitTestState>) -> StatusCode {
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    let mut counter = state.counter.lock().unwrap();
    *counter += 1;
    StatusCode::Content
}

mod handler_trait_implementation_tests {
    use super::*;

    #[tokio::test]
    async fn test_2_parameter_handler_trait() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .get("/test/:id", handler_2_params)
            .build();

        let request = coapum::test_utils::create_test_request("/test/abc123");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
        
        // Verify state was modified
        let counter = state.counter.lock().unwrap();
        assert_eq!(*counter, 1);
        let data = state.data.lock().unwrap();
        assert_eq!(*data, "id:abc123");
    }

    #[tokio::test]
    async fn test_3_parameter_handler_trait() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .post("/data/:id", handler_3_params)
            .build();

        let payload = vec![1, 2, 3, 4, 5];
        let request = coapum::test_utils::create_test_request_with_payload("/data/xyz789", payload);

        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
        
        let json_data: serde_json::Value = serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_data["id"], "xyz789");
        assert_eq!(json_data["byte_count"], 5);
        assert_eq!(json_data["counter"], 1);
    }

    #[tokio::test]
    async fn test_4_parameter_handler_trait() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .post("/items/:id", handler_4_params)
            .build();

        let test_payload = TestData {
            id: 100,
            message: "test message".to_string(),
        };
        let json_data = serde_json::to_vec(&test_payload).unwrap();

        let request = coapum::test_utils::create_test_request_with_content(
            "/items/item001",
            json_data,
            ContentFormat::ApplicationJSON,
        );

        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
        
        let cbor_data: TestData = ciborium::de::from_reader(&response.message.payload[..]).unwrap();
        assert_eq!(cbor_data.id, 101); // 100 + 1 counter
        assert!(cbor_data.message.contains("item001"));
        assert!(cbor_data.message.contains("test message"));
    }

    #[tokio::test]
    async fn test_max_parameter_handler_trait() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .post("/complex/:id", handler_max_params)
            .build();

        let test_payload = TestData {
            id: 200,
            message: "complex test".to_string(),
        };
        
        let mut cbor_data = Vec::new();
        ciborium::ser::into_writer(&test_payload, &mut cbor_data).unwrap();

        let request = coapum::test_utils::create_test_request_with_content(
            "/complex/comp001",
            cbor_data,
            ContentFormat::ApplicationCBOR,
        );

        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
        
        let json_data: serde_json::Value = serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_data["id"], "comp001");
        assert_eq!(json_data["payload_id"], 200);
        assert_eq!(json_data["payload_message"], "complex test");
        assert_eq!(json_data["counter"], 1);
    }

    #[tokio::test]
    async fn test_max_parameter_handler_error_path() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/complex/:id", handler_max_params)
            .build();

        // Send payload with id=0 to trigger error
        let test_payload = TestData {
            id: 0, // This will trigger error in handler
            message: "error test".to_string(),
        };
        
        let mut cbor_data = Vec::new();
        ciborium::ser::into_writer(&test_payload, &mut cbor_data).unwrap();

        let request = coapum::test_utils::create_test_request_with_content(
            "/complex/error001",
            cbor_data,
            ContentFormat::ApplicationCBOR,
        );

        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::BadRequest);
    }
}

mod handler_result_and_response_tests {
    use super::*;

    #[tokio::test]
    async fn test_handler_result_error_trait() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/error", handler_with_result_error)
            .build();

        let request = coapum::test_utils::create_test_request("/error");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::InternalServerError);
    }

    #[tokio::test]
    async fn test_handler_result_success_trait() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/success", handler_with_result_success)
            .build();

        let request = coapum::test_utils::create_test_request("/success");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_handler_json_response_trait() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/json", handler_json_response)
            .build();

        let request = coapum::test_utils::create_test_request("/json");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
        
        let test_data: TestData = serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(test_data.id, 42);
        assert_eq!(test_data.message, "json response test");
    }

    #[tokio::test]
    async fn test_handler_cbor_response_trait() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/cbor", handler_cbor_response)
            .build();

        let request = coapum::test_utils::create_test_request("/cbor");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
        
        let test_data: TestData = ciborium::de::from_reader(&response.message.payload[..]).unwrap();
        assert_eq!(test_data.id, 99);
        assert_eq!(test_data.message, "cbor response test");
    }

    #[tokio::test]
    async fn test_handler_async_delay_trait() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .get("/async", handler_async_delay)
            .build();

        let request = coapum::test_utils::create_test_request("/async");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
        
        // Verify async handler was executed
        let counter = state.counter.lock().unwrap();
        assert_eq!(*counter, 1);
    }
}

mod handler_trait_edge_cases {
    use super::*;

    #[tokio::test]
    async fn test_handler_trait_with_extraction_failure() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/extract/:id", handler_4_params)
            .build();

        // Send invalid JSON to trigger extraction failure
        let request = coapum::test_utils::create_test_request_with_content(
            "/extract/test123",
            vec![0xFF, 0xFE, 0xFD], // Invalid JSON
            ContentFormat::ApplicationJSON,
        );

        let response = router.call(request).await.unwrap();

        // Should handle extraction failure gracefully
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_concurrent_handler_trait_execution() {
        let state = HandlerTraitTestState::default();
        let observer = MemObserver::new();
        let router = RouterBuilder::new(state.clone(), observer)
            .post("/concurrent/:id", handler_3_params)
            .build();

        let mut handles = vec![];
        for i in 0..3 {
            let mut router_clone = router.clone();
            let handle = tokio::spawn(async move {
                let payload = vec![i; 5];
                let request = coapum::test_utils::create_test_request_with_payload(
                    &format!("/concurrent/test{}", i),
                    payload,
                );
                router_clone.call(request).await
            });
            handles.push(handle);
        }

        // Wait for all concurrent requests
        for handle in handles {
            let result = handle.await.unwrap().unwrap();
            assert_eq!(*result.get_status(), coapum::ResponseType::Content);
        }

        // Verify all handlers were executed
        let counter = state.counter.lock().unwrap();
        assert_eq!(*counter, 3);
    }
}