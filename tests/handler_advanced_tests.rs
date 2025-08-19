//! Advanced handler tests
//!
//! These tests focus on complex handler scenarios including multiple parameter
//! combinations, error handling, type conversions, and edge cases.

use std::sync::Arc;

use coapum::{
    extract::{Bytes, Cbor, Json, Path, Source, State, StatusCode},
    observer::memory::MemObserver,
    router::RouterBuilder,
    ContentFormat,
};
use serde::{Deserialize, Serialize};
use tower::Service;

#[derive(Debug, Clone)]
struct HandlerTestState {
    request_count: Arc<std::sync::Mutex<u32>>,
    last_path_param: Arc<std::sync::Mutex<Option<String>>>,
    last_payload_size: Arc<std::sync::Mutex<usize>>,
}

impl AsRef<HandlerTestState> for HandlerTestState {
    fn as_ref(&self) -> &HandlerTestState {
        self
    }
}

impl Default for HandlerTestState {
    fn default() -> Self {
        Self {
            request_count: Arc::new(std::sync::Mutex::new(0)),
            last_path_param: Arc::new(std::sync::Mutex::new(None)),
            last_payload_size: Arc::new(std::sync::Mutex::new(0)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestPayload {
    id: u32,
    name: String,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ResponseData {
    processed: bool,
    count: u32,
    message: String,
}

mod handler_parameter_combinations {
    use super::*;

    // Handler with no parameters - baseline test
    async fn handler_no_params() -> StatusCode {
        StatusCode::Content
    }

    // Handler with state only
    async fn handler_state_only(State(state): State<HandlerTestState>) -> StatusCode {
        let mut count = state.request_count.lock().unwrap();
        *count += 1;
        StatusCode::Content
    }

    // Handler with path parameter only
    async fn handler_path_only(Path(id): Path<String>) -> Json<serde_json::Value> {
        Json(serde_json::json!({"extracted_id": id}))
    }

    // Handler with bytes only
    async fn handler_bytes_only(bytes: Bytes) -> Json<serde_json::Value> {
        Json(serde_json::json!({"payload_size": bytes.len()}))
    }

    // Handler with JSON payload only
    async fn handler_json_only(Json(payload): Json<TestPayload>) -> Cbor<ResponseData> {
        Cbor(ResponseData {
            processed: true,
            count: payload.id,
            message: payload.name,
        })
    }

    // Handler with CBOR payload only
    async fn handler_cbor_only(Cbor(payload): Cbor<TestPayload>) -> Json<ResponseData> {
        Json(ResponseData {
            processed: true,
            count: payload.id,
            message: payload.name,
        })
    }

    // Handler with 2 parameters: State + Path
    async fn handler_state_path(
        State(state): State<HandlerTestState>,
        Path(id): Path<String>,
    ) -> Json<serde_json::Value> {
        let mut count = state.request_count.lock().unwrap();
        *count += 1;
        let mut last_param = state.last_path_param.lock().unwrap();
        *last_param = Some(id.clone());

        Json(serde_json::json!({
            "id": id,
            "count": *count
        }))
    }

    // Handler with 3 parameters: State + Path + Bytes
    async fn handler_state_path_bytes(
        State(state): State<HandlerTestState>,
        Path(id): Path<String>,
        bytes: Bytes,
    ) -> Json<serde_json::Value> {
        let mut count = state.request_count.lock().unwrap();
        *count += 1;
        let mut last_param = state.last_path_param.lock().unwrap();
        *last_param = Some(id.clone());
        let mut last_size = state.last_payload_size.lock().unwrap();
        *last_size = bytes.len();

        Json(serde_json::json!({
            "id": id,
            "payload_size": bytes.len(),
            "count": *count
        }))
    }

    // Handler with 4 parameters: State + Path + JSON + Source
    async fn handler_four_params(
        State(state): State<HandlerTestState>,
        Path(id): Path<String>,
        Json(payload): Json<TestPayload>,
        Source(addr): Source,
    ) -> Json<serde_json::Value> {
        let mut count = state.request_count.lock().unwrap();
        *count += 1;

        Json(serde_json::json!({
            "id": id,
            "payload_id": payload.id,
            "payload_name": payload.name,
            "source_port": addr.port(),
            "count": *count
        }))
    }

    #[tokio::test]
    async fn test_handler_no_parameters() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .get("/no_params", handler_no_params)
            .build();

        let request = coapum::test_utils::create_test_request("/no_params");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        // Verify state wasn't modified
        let count = state.request_count.lock().unwrap();
        assert_eq!(*count, 0);
    }

    #[tokio::test]
    async fn test_handler_state_only() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .get("/state_only", handler_state_only)
            .build();

        let request = coapum::test_utils::create_test_request("/state_only");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        // Verify state was modified
        let count = state.request_count.lock().unwrap();
        assert_eq!(*count, 1);
    }

    #[tokio::test]
    async fn test_handler_path_only() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/path/:id", handler_path_only)
            .build();

        let request = coapum::test_utils::create_test_request("/path/test123");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        // Verify path was extracted
        let json_data: serde_json::Value =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_data["extracted_id"], "test123");
    }

    #[tokio::test]
    async fn test_handler_bytes_only() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/bytes", handler_bytes_only)
            .build();

        let payload = vec![1, 2, 3, 4, 5];
        let request =
            coapum::test_utils::create_test_request_with_payload("/bytes", payload.clone());

        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        let json_data: serde_json::Value =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_data["payload_size"], 5);
    }

    #[tokio::test]
    async fn test_handler_json_payload() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/json", handler_json_only)
            .build();

        let test_payload = TestPayload {
            id: 42,
            name: "test_payload".to_string(),
            data: vec![1, 2, 3],
        };
        let json_data = serde_json::to_vec(&test_payload).unwrap();

        let request = coapum::test_utils::create_test_request_with_content(
            "/json",
            json_data,
            ContentFormat::ApplicationJSON,
        );

        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        // Verify CBOR response
        let response_data: ResponseData =
            ciborium::de::from_reader(&response.message.payload[..]).unwrap();
        assert!(response_data.processed);
        assert_eq!(response_data.count, 42);
        assert_eq!(response_data.message, "test_payload");
    }

    #[tokio::test]
    async fn test_handler_cbor_payload() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/cbor", handler_cbor_only)
            .build();

        let test_payload = TestPayload {
            id: 99,
            name: "cbor_test".to_string(),
            data: vec![4, 5, 6],
        };

        let mut cbor_data = Vec::new();
        ciborium::ser::into_writer(&test_payload, &mut cbor_data).unwrap();

        let request = coapum::test_utils::create_test_request_with_content(
            "/cbor",
            cbor_data,
            ContentFormat::ApplicationCBOR,
        );

        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        // Verify JSON response
        let response_data: ResponseData =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert!(response_data.processed);
        assert_eq!(response_data.count, 99);
        assert_eq!(response_data.message, "cbor_test");
    }

    #[tokio::test]
    async fn test_handler_state_and_path() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .get("/state_path/:id", handler_state_path)
            .build();

        let request = coapum::test_utils::create_test_request("/state_path/device123");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        // Verify response
        let json_data: serde_json::Value =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_data["id"], "device123");
        assert_eq!(json_data["count"], 1);

        // Verify state was updated
        let count = state.request_count.lock().unwrap();
        assert_eq!(*count, 1);
        let last_param = state.last_path_param.lock().unwrap();
        assert_eq!(*last_param, Some("device123".to_string()));
    }

    #[tokio::test]
    async fn test_handler_three_parameters() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .post("/three/:id", handler_state_path_bytes)
            .build();

        let payload = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let request =
            coapum::test_utils::create_test_request_with_payload("/three/sensor789", payload);

        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        let json_data: serde_json::Value =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_data["id"], "sensor789");
        assert_eq!(json_data["payload_size"], 8);
        assert_eq!(json_data["count"], 1);

        // Verify state
        let last_size = state.last_payload_size.lock().unwrap();
        assert_eq!(*last_size, 8);
    }

    #[tokio::test]
    async fn test_handler_four_parameters() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state.clone(), observer)
            .post("/four/:id", handler_four_params)
            .build();

        let test_payload = TestPayload {
            id: 777,
            name: "four_param_test".to_string(),
            data: vec![9, 8, 7],
        };
        let json_data = serde_json::to_vec(&test_payload).unwrap();

        let request = coapum::test_utils::create_test_request_with_content(
            "/four/gateway456",
            json_data,
            ContentFormat::ApplicationJSON,
        );

        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        let json_response: serde_json::Value =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_response["id"], "gateway456");
        assert_eq!(json_response["payload_id"], 777);
        assert_eq!(json_response["payload_name"], "four_param_test");
        assert_eq!(json_response["count"], 1);
        // source_port should be present
        assert!(json_response["source_port"].is_number());
    }
}

mod handler_error_scenarios {
    use super::*;

    // Handler that returns Result with error
    async fn handler_with_error() -> Result<StatusCode, StatusCode> {
        Err(StatusCode::BadRequest)
    }

    // Handler that returns Result with success
    async fn handler_with_success() -> Result<StatusCode, StatusCode> {
        Ok(StatusCode::Content)
    }

    // Handler that requires JSON but might get invalid data
    async fn handler_expecting_json(Json(_payload): Json<TestPayload>) -> StatusCode {
        StatusCode::Content
    }

    // Handler that requires CBOR but might get invalid data
    async fn handler_expecting_cbor(Cbor(_payload): Cbor<TestPayload>) -> StatusCode {
        StatusCode::Content
    }

    // Handler with path parameter that requires specific type conversion
    async fn handler_numeric_path(Path(num_str): Path<String>) -> Json<serde_json::Value> {
        match num_str.parse::<i32>() {
            Ok(num) => Json(serde_json::json!({"number": num})),
            Err(_) => Json(serde_json::json!({"error": "invalid number"})),
        }
    }

    // Handler that might fail to convert response
    async fn handler_response_conversion_issue() -> Json<std::collections::HashMap<String, String>>
    {
        // This should work fine, but tests response conversion path
        let mut map = std::collections::HashMap::new();
        map.insert("status".to_string(), "ok".to_string());
        Json(map)
    }

    #[tokio::test]
    async fn test_handler_explicit_error() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/error", handler_with_error)
            .build();

        let request = coapum::test_utils::create_test_request("/error");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::BadRequest);
    }

    #[tokio::test]
    async fn test_handler_explicit_success() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/success", handler_with_success)
            .build();

        let request = coapum::test_utils::create_test_request("/success");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_handler_json_extraction_failure() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/expect_json", handler_expecting_json)
            .build();

        // Send invalid JSON
        let request = coapum::test_utils::create_test_request_with_content(
            "/expect_json",
            vec![0xFF, 0xFE, 0xFD], // Invalid JSON bytes
            ContentFormat::ApplicationJSON,
        );

        let response = router.call(request).await.unwrap();

        // Should return error due to extraction failure
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_handler_cbor_extraction_failure() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .post("/expect_cbor", handler_expecting_cbor)
            .build();

        // Send invalid CBOR
        let request = coapum::test_utils::create_test_request_with_content(
            "/expect_cbor",
            vec![0xFF, 0xFF, 0xFF], // Invalid CBOR bytes
            ContentFormat::ApplicationCBOR,
        );

        let response = router.call(request).await.unwrap();

        // Should return error due to extraction failure
        assert_ne!(*response.get_status(), coapum::ResponseType::Content);
    }

    #[tokio::test]
    async fn test_handler_path_conversion_failure() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/number/:num", handler_numeric_path)
            .build();

        // Send non-numeric path parameter
        let request = coapum::test_utils::create_test_request("/number/not_a_number");
        let response = router.call(request).await.unwrap();

        // Should succeed but return error in JSON
        assert_eq!(*response.get_status(), coapum::ResponseType::Content);
        let json_data: serde_json::Value =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_data["error"], "invalid number");
    }

    #[tokio::test]
    async fn test_handler_path_conversion_success() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/number/:num", handler_numeric_path)
            .build();

        // Send valid numeric path parameter
        let request = coapum::test_utils::create_test_request("/number/42");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        let json_data: serde_json::Value =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_data["number"], 42);
    }

    #[tokio::test]
    async fn test_handler_response_conversion() {
        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let mut router = RouterBuilder::new(state, observer)
            .get("/response_test", handler_response_conversion_issue)
            .build();

        let request = coapum::test_utils::create_test_request("/response_test");
        let response = router.call(request).await.unwrap();

        assert_eq!(*response.get_status(), coapum::ResponseType::Content);

        // Verify the response can be deserialized
        let json_data: std::collections::HashMap<String, String> =
            serde_json::from_slice(&response.message.payload).unwrap();
        assert_eq!(json_data.get("status"), Some(&"ok".to_string()));
    }
}

mod handler_concurrent_access {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static CONCURRENT_COUNTER: AtomicU32 = AtomicU32::new(0);

    async fn concurrent_handler(State(state): State<HandlerTestState>) -> Json<serde_json::Value> {
        // Simulate some processing time
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let global_count = CONCURRENT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut local_count = state.request_count.lock().unwrap();
        *local_count += 1;

        Json(serde_json::json!({
            "global_count": global_count,
            "local_count": *local_count
        }))
    }

    #[tokio::test]
    async fn test_concurrent_handler_execution() {
        CONCURRENT_COUNTER.store(0, Ordering::SeqCst);

        let state = HandlerTestState::default();
        let observer = MemObserver::new();
        let router = RouterBuilder::new(state.clone(), observer)
            .get("/concurrent", concurrent_handler)
            .build();

        // Launch multiple concurrent requests
        let mut handles = vec![];
        for _i in 0..5 {
            let mut router_clone = router.clone();
            let handle = tokio::spawn(async move {
                let request = coapum::test_utils::create_test_request("/concurrent");
                router_clone.call(request).await
            });
            handles.push(handle);
        }

        // Wait for all requests to complete
        let mut results = vec![];
        for handle in handles {
            let result = handle.await.unwrap().unwrap();
            assert_eq!(*result.get_status(), coapum::ResponseType::Content);
            results.push(result);
        }

        // Verify all handlers were called
        assert_eq!(CONCURRENT_COUNTER.load(Ordering::SeqCst), 5);

        // Verify local state counter
        let local_count = state.request_count.lock().unwrap();
        assert_eq!(*local_count, 5);

        // Verify each response has unique global counts
        let mut global_counts = vec![];
        for result in results {
            let json_data: serde_json::Value =
                serde_json::from_slice(&result.message.payload).unwrap();
            global_counts.push(json_data["global_count"].as_u64().unwrap());
        }

        global_counts.sort();
        assert_eq!(global_counts, vec![0, 1, 2, 3, 4]);
    }
}
