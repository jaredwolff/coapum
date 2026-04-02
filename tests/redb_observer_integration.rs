#[cfg(feature = "redb-observer")]
mod redb_integration_tests {
    use std::sync::Arc;
    use std::time::Duration;

    use ciborium::value::Value;
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;
    use tokio::time::sleep;

    use coapum::observer::{Observer, ObserverValue, redb::RedbObserver};

    fn cbor_map(pairs: &[(&str, Value)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(k, v)| (Value::Text(k.to_string()), v.clone()))
                .collect(),
        )
    }

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

        let temp_val = cbor_map(&[
            ("value", Value::Float(25.5)),
            ("unit", Value::Text("C".into())),
        ]);
        let settings_val = cbor_map(&[
            ("enabled", Value::Bool(true)),
            ("mode", Value::Text("auto".into())),
        ]);

        // First, write some data
        {
            let mut observer = RedbObserver::new(db_path).unwrap();

            observer
                .write("device_1", "/sensor/temperature", &temp_val)
                .await
                .unwrap();

            observer
                .write("device_2", "/config/settings", &settings_val)
                .await
                .unwrap();
        }

        // Then, create a new observer instance and verify data persists
        {
            let mut observer = RedbObserver::new(db_path).unwrap();

            let temp = observer
                .read("device_1", "/sensor/temperature")
                .await
                .unwrap();
            assert_eq!(temp, Some(temp_val));

            let settings = observer.read("device_2", "/config/settings").await.unwrap();
            assert_eq!(settings, Some(settings_val));
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

        let status_val = cbor_map(&[("online", Value::Bool(true))]);
        let data_val = cbor_map(&[("value", Value::Integer(42.into()))]);
        let config_val = cbor_map(&[("mode", Value::Text("test".into()))]);

        // Write to each device
        observer
            .write("device_1", "/status", &status_val)
            .await
            .unwrap();
        observer
            .write("device_2", "/data", &data_val)
            .await
            .unwrap();
        observer
            .write("device_3", "/config", &config_val)
            .await
            .unwrap();

        // Verify each observer received its notification
        let msg1 = rx1.recv().await.unwrap();
        assert_eq!(msg1.value, status_val);
        assert_eq!(msg1.path, "/status");

        let msg2 = rx2.recv().await.unwrap();
        assert_eq!(msg2.value, data_val);
        assert_eq!(msg2.path, "/data");

        let msg3 = rx3.recv().await.unwrap();
        assert_eq!(msg3.value, config_val);
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
                &cbor_map(&[
                    (
                        "sensors",
                        cbor_map(&[
                            (
                                "temperature",
                                cbor_map(&[
                                    ("value", Value::Float(25.0)),
                                    ("unit", Value::Text("C".into())),
                                ]),
                            ),
                            (
                                "humidity",
                                cbor_map(&[
                                    ("value", Value::Integer(60.into())),
                                    ("unit", Value::Text("%".into())),
                                ]),
                            ),
                        ]),
                    ),
                    (
                        "config",
                        cbor_map(&[("interval", Value::Integer(1000.into()))]),
                    ),
                ]),
            )
            .await
            .unwrap();

        // Read specific nested paths
        let temp = observer
            .read("device_1", "/sensors/temperature")
            .await
            .unwrap();
        assert_eq!(
            temp,
            Some(cbor_map(&[
                ("value", Value::Float(25.0)),
                ("unit", Value::Text("C".into())),
            ]))
        );

        let humidity = observer
            .read("device_1", "/sensors/humidity/value")
            .await
            .unwrap();
        assert_eq!(humidity, Some(Value::Integer(60.into())));

        let interval = observer.read("device_1", "/config/interval").await.unwrap();
        assert_eq!(interval, Some(Value::Integer(1000.into())));

        // Update a nested value
        observer
            .write(
                "device_1",
                "/sensors/temperature/value",
                &Value::Float(26.5),
            )
            .await
            .unwrap();

        // Verify the update
        let new_temp = observer
            .read("device_1", "/sensors/temperature/value")
            .await
            .unwrap();
        assert_eq!(new_temp, Some(Value::Float(26.5)));

        // Verify the structure is preserved
        let all_sensors = observer.read("device_1", "/sensors").await.unwrap();
        assert!(all_sensors.is_some());
        let sensors = all_sensors.unwrap();
        // Navigate with cbor_pointer
        let temp_val = coapum::cbor_pointer(&sensors, "/temperature/value");
        assert_eq!(temp_val, Some(&Value::Float(26.5)));
        let hum_val = coapum::cbor_pointer(&sensors, "/humidity/value");
        assert_eq!(hum_val, Some(&Value::Integer(60.into())));

        // temp_file is automatically cleaned up when dropped
    }

    #[tokio::test]
    async fn test_redb_observer_concurrent_access() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let temp_file = NamedTempFile::new().unwrap();
        let db_path = temp_file.path().to_str().unwrap();

        // Create ONE observer and clone it into tasks (redb enforces exclusive file access)
        let observer = RedbObserver::new(db_path).unwrap();
        let mut handles = vec![];

        for i in 0..5 {
            let mut obs = observer.clone();
            let handle = tokio::spawn(async move {
                // Each task writes to its own device
                let device_id = format!("device_{}", i);
                let val = cbor_map(&[
                    ("task_id", Value::Integer(i.into())),
                    ("timestamp", Value::Integer((i * 100).into())),
                ]);
                obs.write(&device_id, "/value", &val).await.unwrap();

                // Read back to verify
                let value = obs.read(&device_id, "/value").await.unwrap();
                assert_eq!(value, Some(val));
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all data is present via fresh clone
        let mut observer = observer.clone();
        for i in 0..5i64 {
            let device_id = format!("device_{}", i);
            let value = observer.read(&device_id, "/value").await.unwrap();
            let expected = cbor_map(&[
                ("task_id", Value::Integer(i.into())),
                ("timestamp", Value::Integer((i * 100).into())),
            ]);
            assert_eq!(value, Some(expected));
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
