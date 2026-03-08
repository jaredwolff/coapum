#[cfg(feature = "redb-observer")]
mod redb_integration_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;
    use tokio::time::sleep;

    use coapum::observer::{Observer, ObserverValue, redb::RedbObserver};

    // Named constants for test timing
    const REGISTRATION_DELAY: Duration = Duration::from_millis(100);

    #[tokio::test]
    async fn test_redb_observer_persistence() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        // Create a temporary database file that will be automatically cleaned up
        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_str().unwrap();

        // First, write some data
        {
            let mut observer = RedbObserver::new(db_path).unwrap();

            // Write data
            observer
                .write(
                    "device_1",
                    "/sensor/temperature",
                    &json!({"value": 25.5, "unit": "C"}),
                )
                .await
                .unwrap();

            observer
                .write(
                    "device_2",
                    "/config/settings",
                    &json!({"enabled": true, "mode": "auto"}),
                )
                .await
                .unwrap();
        }

        // Then, create a new observer instance and verify data persists
        {
            let mut observer = RedbObserver::new(db_path).unwrap();

            // Read back the data
            let temp = observer
                .read("device_1", "/sensor/temperature")
                .await
                .unwrap();
            assert_eq!(temp, Some(json!({"value": 25.5, "unit": "C"})));

            let settings = observer.read("device_2", "/config/settings").await.unwrap();
            assert_eq!(settings, Some(json!({"enabled": true, "mode": "auto"})));
        }

        // temp_file is automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_redb_observer_multiple_devices() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_str().unwrap();
        let mut observer = RedbObserver::new(db_path).unwrap();

        // Clear any existing data
        observer.clear("device_1").await.unwrap();
        observer.clear("device_2").await.unwrap();
        observer.clear("device_3").await.unwrap();

        // Set up channels for each device
        let (tx1, mut rx1) = mpsc::channel::<ObserverValue>(10);
        let (tx2, mut rx2) = mpsc::channel::<ObserverValue>(10);
        let (tx3, mut rx3) = mpsc::channel::<ObserverValue>(10);

        // Register observers for different paths on different devices
        observer
            .register("device_1", "/status", Arc::new(tx1))
            .await
            .unwrap();
        observer
            .register("device_2", "/data", Arc::new(tx2))
            .await
            .unwrap();
        observer
            .register("device_3", "/config", Arc::new(tx3))
            .await
            .unwrap();

        // Wait a bit for registrations to take effect
        sleep(REGISTRATION_DELAY).await;

        // Write to each device
        observer
            .write("device_1", "/status", &json!({"online": true}))
            .await
            .unwrap();
        observer
            .write("device_2", "/data", &json!({"value": 42}))
            .await
            .unwrap();
        observer
            .write("device_3", "/config", &json!({"mode": "test"}))
            .await
            .unwrap();

        // Verify each observer received its notification
        let msg1 = rx1.recv().await.unwrap();
        assert_eq!(msg1.value, json!({"online": true}));
        assert_eq!(msg1.path, "/status");

        let msg2 = rx2.recv().await.unwrap();
        assert_eq!(msg2.value, json!({"value": 42}));
        assert_eq!(msg2.path, "/data");

        let msg3 = rx3.recv().await.unwrap();
        assert_eq!(msg3.value, json!({"mode": "test"}));
        assert_eq!(msg3.path, "/config");

        // Clean up
        observer.unregister_all().await.unwrap();
        // temp_file is automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_redb_observer_nested_paths() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_str().unwrap();
        let mut observer = RedbObserver::new(db_path).unwrap();

        // Clear existing data
        observer.clear("device_1").await.unwrap();

        // Write nested data structure
        observer
            .write(
                "device_1",
                "/",
                &json!({
                    "sensors": {
                        "temperature": {
                            "value": 25.0,
                            "unit": "C"
                        },
                        "humidity": {
                            "value": 60,
                            "unit": "%"
                        }
                    },
                    "config": {
                        "interval": 1000
                    }
                }),
            )
            .await
            .unwrap();

        // Read specific nested paths
        let temp = observer
            .read("device_1", "/sensors/temperature")
            .await
            .unwrap();
        assert_eq!(temp, Some(json!({"value": 25.0, "unit": "C"})));

        let humidity = observer
            .read("device_1", "/sensors/humidity/value")
            .await
            .unwrap();
        assert_eq!(humidity, Some(json!(60)));

        let interval = observer.read("device_1", "/config/interval").await.unwrap();
        assert_eq!(interval, Some(json!(1000)));

        // Update a nested value
        observer
            .write("device_1", "/sensors/temperature/value", &json!(26.5))
            .await
            .unwrap();

        // Verify the update
        let new_temp = observer
            .read("device_1", "/sensors/temperature/value")
            .await
            .unwrap();
        assert_eq!(new_temp, Some(json!(26.5)));

        // Verify the structure is preserved
        let all_sensors = observer.read("device_1", "/sensors").await.unwrap();
        assert!(all_sensors.is_some());
        let sensors = all_sensors.unwrap();
        assert_eq!(sensors["temperature"]["value"], json!(26.5));
        assert_eq!(sensors["humidity"]["value"], json!(60));

        // temp_file is automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_redb_observer_concurrent_access() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_str().unwrap();

        // Create multiple tasks that will access the database concurrently
        let mut handles = vec![];

        for i in 0..5 {
            let db_path = db_path.to_string();
            let handle = tokio::spawn(async move {
                let mut observer = RedbObserver::new(&db_path).unwrap();

                // Each task writes to its own device
                let device_id = format!("device_{}", i);
                observer
                    .write(
                        &device_id,
                        "/value",
                        &json!({"task_id": i, "timestamp": i * 100}),
                    )
                    .await
                    .unwrap();

                // Read back to verify
                let value = observer.read(&device_id, "/value").await.unwrap();
                assert_eq!(value, Some(json!({"task_id": i, "timestamp": i * 100})));
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all data is present
        let mut observer = RedbObserver::new(db_path).unwrap();
        for i in 0..5 {
            let device_id = format!("device_{}", i);
            let value = observer.read(&device_id, "/value").await.unwrap();
            assert_eq!(value, Some(json!({"task_id": i, "timestamp": i * 100})));
        }

        // temp_file is automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_redb_observer_security_validation() {
        // Path validation is now centralized in observer::validate_observer_path()
        // and called by the server before paths reach any backend. Backends no longer
        // need their own validation. Test the centralized function instead.
        use coapum::validate_observer_path;

        // Path traversal
        assert!(validate_observer_path("../../../etc/passwd").is_err());

        // Excessive depth
        let deep = (0..11)
            .map(|i| format!("p{}", i))
            .collect::<Vec<_>>()
            .join("/");
        assert!(validate_observer_path(&deep).is_err());

        // Invalid chars
        assert!(validate_observer_path("path\x00hidden").is_err());

        // Valid paths still work
        assert_eq!(validate_observer_path("valid/path").unwrap(), "/valid/path");
    }
}
