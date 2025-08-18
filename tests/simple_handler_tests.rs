//! Simple handler tests focusing on basic functionality
//!
//! These tests validate that handlers work correctly with different
//! parameter combinations and error scenarios.

use std::sync::Arc;
use coapum::{
    extract::{State, StatusCode},
    router::RouterBuilder,
    observer::memory::MemObserver,
};
use tower::Service;

#[derive(Debug, Clone)]
struct SimpleState {
    counter: Arc<std::sync::Mutex<i32>>,
}

impl AsRef<SimpleState> for SimpleState {
    fn as_ref(&self) -> &SimpleState {
        self
    }
}

// Simple handler with no parameters
async fn simple_handler() -> StatusCode {
    StatusCode::Content
}

// Handler with state parameter
async fn stateful_handler(State(state): State<SimpleState>) -> StatusCode {
    let mut counter = state.counter.lock().unwrap();
    *counter += 1;
    StatusCode::Content
}

// Handler that returns an error
async fn error_handler() -> Result<StatusCode, StatusCode> {
    Err(StatusCode::InternalServerError)
}

#[tokio::test]
async fn test_simple_handler_execution() {
    let state = SimpleState {
        counter: Arc::new(std::sync::Mutex::new(0)),
    };
    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state.clone(), observer)
        .get("/simple", simple_handler)
        .build();

    let request = coapum::test_utils::create_test_request("/simple");
    let response = router.call(request).await.unwrap();

    assert_eq!(*response.get_status(), coapum::ResponseType::Content);
}

#[tokio::test]
async fn test_stateful_handler_execution() {
    let state = SimpleState {
        counter: Arc::new(std::sync::Mutex::new(0)),
    };
    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state.clone(), observer)
        .get("/stateful", stateful_handler)
        .build();

    let request = coapum::test_utils::create_test_request("/stateful");
    let response = router.call(request).await.unwrap();

    assert_eq!(*response.get_status(), coapum::ResponseType::Content);
    
    // Verify state was modified
    let counter = state.counter.lock().unwrap();
    assert_eq!(*counter, 1);
}

#[tokio::test]
async fn test_error_handler_execution() {
    let state = SimpleState {
        counter: Arc::new(std::sync::Mutex::new(0)),
    };
    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state, observer)
        .get("/error", error_handler)
        .build();

    let request = coapum::test_utils::create_test_request("/error");
    let response = router.call(request).await.unwrap();

    assert_eq!(*response.get_status(), coapum::ResponseType::InternalServerError);
}

#[tokio::test]
async fn test_multiple_handlers_in_same_router() {
    let state = SimpleState {
        counter: Arc::new(std::sync::Mutex::new(0)),
    };
    let observer = MemObserver::new();
    let mut router = RouterBuilder::new(state.clone(), observer)
        .get("/simple", simple_handler)
        .get("/stateful", stateful_handler)
        .get("/error", error_handler)
        .build();

    // Test each handler
    let paths_and_expected = vec![
        ("/simple", coapum::ResponseType::Content),
        ("/stateful", coapum::ResponseType::Content),
        ("/error", coapum::ResponseType::InternalServerError),
    ];

    for (path, expected_status) in paths_and_expected {
        let request = coapum::test_utils::create_test_request(path);
        let response = router.call(request).await.unwrap();
        assert_eq!(*response.get_status(), expected_status, "Failed for path: {}", path);
    }

    // Verify stateful handler was called
    let counter = state.counter.lock().unwrap();
    assert_eq!(*counter, 1);
}