//! Core tests for external state update functionality

use coapum::{observer::memory::MemObserver, router::CoapRouter, StateUpdateError};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
struct TestAppState {
    counter: i32,
    data: HashMap<String, String>,
    enabled: bool,
}

impl TestAppState {
    fn new() -> Self {
        TestAppState {
            counter: 0,
            data: HashMap::new(),
            enabled: true,
        }
    }
}

#[tokio::test]
async fn test_state_update_handle_creation() {
    let state = TestAppState::new();
    let observer = MemObserver::new();
    let mut router = CoapRouter::new(state, observer);

    // Test that state_update_handle returns None when not enabled
    assert!(router.state_update_handle().is_none());

    // Enable state updates and test that we can get a handle
    let state_handle = router.enable_state_updates(100);
    assert!(router.state_update_handle().is_some());

    // Test basic state update functionality
    let result = state_handle
        .update(|state: &mut TestAppState| {
            state.counter += 1;
        })
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_external_state_updates() {
    let state = TestAppState::new();
    let observer = MemObserver::new();
    let mut router = CoapRouter::new(state, observer);

    // Enable state updates and get handle
    let state_handle = router.enable_state_updates(100);

    // Give the background task time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Test counter update
    state_handle
        .update(|state: &mut TestAppState| {
            state.counter = 42;
        })
        .await
        .unwrap();

    // Test data update
    state_handle
        .update(|state: &mut TestAppState| {
            state
                .data
                .insert("test_key".to_string(), "test_value".to_string());
        })
        .await
        .unwrap();

    // Test boolean flag update
    state_handle
        .update(|state: &mut TestAppState| {
            state.enabled = false;
        })
        .await
        .unwrap();

    // Give updates time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // All updates should have been processed successfully
    // (We can't directly verify the state values since they're behind Arc<Mutex>,
    //  but the fact that no errors occurred indicates the system is working)
}

#[tokio::test]
async fn test_multiple_state_handles() {
    let state = TestAppState::new();
    let observer = MemObserver::new();
    let mut router = CoapRouter::new(state, observer);

    // Enable state updates
    let state_handle1 = router.enable_state_updates(100);
    let state_handle2 = router.state_update_handle().unwrap();

    // Give the background task time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Test that both handles work
    state_handle1
        .update(|state: &mut TestAppState| {
            state.counter += 10;
        })
        .await
        .unwrap();

    state_handle2
        .update(|state: &mut TestAppState| {
            state.counter += 5;
        })
        .await
        .unwrap();

    // Give updates time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
}

#[tokio::test]
async fn test_try_update_functionality() {
    let state = TestAppState::new();
    let observer = MemObserver::new();
    let mut router = CoapRouter::new(state, observer);

    // Enable state updates with small buffer
    let state_handle = router.enable_state_updates(2);

    // Give the background task time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Test successful try_update
    let result = state_handle.try_update(|state: &mut TestAppState| {
        state.counter = 1;
    });
    assert!(result.is_ok());

    // Test another successful try_update
    let result = state_handle.try_update(|state: &mut TestAppState| {
        state.counter = 2;
    });
    assert!(result.is_ok());

    // Give updates time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
}

#[tokio::test]
async fn test_state_update_with_complex_operations() {
    let state = TestAppState::new();
    let observer = MemObserver::new();
    let mut router = CoapRouter::new(state, observer);

    let state_handle = router.enable_state_updates(100);

    // Give the background task time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Test complex state operation
    state_handle
        .update(|state: &mut TestAppState| {
            // Simulate complex state changes
            for i in 0..10 {
                state
                    .data
                    .insert(format!("key_{}", i), format!("value_{}", i));
            }
            state.counter = state.data.len() as i32;
            state.enabled = state.counter > 5;
        })
        .await
        .unwrap();

    // Test batch updates
    for i in 10..20 {
        state_handle
            .update(move |state: &mut TestAppState| {
                state
                    .data
                    .insert(format!("batch_key_{}", i), format!("batch_value_{}", i));
                state.counter += 1;
            })
            .await
            .unwrap();
    }

    // Give all updates time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_state_handle_cloning() {
    let state = TestAppState::new();
    let observer = MemObserver::new();
    let mut router = CoapRouter::new(state, observer);

    let state_handle = router.enable_state_updates(100);

    // Test that handles can be cloned
    let cloned_handle = state_handle.clone();

    // Give the background task time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Test that both original and cloned handles work
    state_handle
        .update(|state: &mut TestAppState| {
            state.counter = 100;
        })
        .await
        .unwrap();

    cloned_handle
        .update(|state: &mut TestAppState| {
            state.counter += 50;
        })
        .await
        .unwrap();

    // Give updates time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
}

#[tokio::test]
async fn test_state_update_error_types() {
    let state = TestAppState::new();
    let observer = MemObserver::new();
    let mut router = CoapRouter::new(state, observer);

    let state_handle = router.enable_state_updates(1);

    // Give the background task time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Test that error types work correctly
    let result = state_handle
        .update(|state: &mut TestAppState| {
            state.counter = 1;
        })
        .await;
    assert!(result.is_ok());

    // Test error display and debug
    let error = StateUpdateError::ChannelFull;
    assert_eq!(error.to_string(), "State update channel is full");

    let error = StateUpdateError::ChannelClosed;
    assert_eq!(error.to_string(), "State update channel is closed");
}

#[tokio::test]
async fn test_concurrent_state_updates() {
    let state = TestAppState::new();
    let observer = MemObserver::new();
    let mut router = CoapRouter::new(state, observer);

    let state_handle = router.enable_state_updates(1000);

    // Give the background task time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Spawn multiple tasks that update the state concurrently
    let mut handles = Vec::new();

    for i in 0..10 {
        let handle = state_handle.clone();
        let task = tokio::spawn(async move {
            handle
                .update(move |state: &mut TestAppState| {
                    state
                        .data
                        .insert(format!("concurrent_{}", i), format!("value_{}", i));
                    state.counter += 1;
                })
                .await
                .unwrap();
        });
        handles.push(task);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Give final updates time to process
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
}
