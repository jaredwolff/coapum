use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use ciborium::value::Value;
use tokio::sync::mpsc::Sender;

use super::{Observer, ObserverChannels, ObserverValue};

/// A memory-based observer that stores data in a HashMap.
#[derive(Clone, Debug)]
pub struct MemObserver {
    db: HashMap<String, Value>,
    /// Shared channel management for observer notifications.
    pub channels: ObserverChannels,
}

impl MemObserver {
    /// Creates a new instance of `MemObserver`.
    pub fn new() -> Self {
        Self {
            db: HashMap::new(),
            channels: ObserverChannels::new(),
        }
    }
}

impl Default for MemObserver {
    fn default() -> Self {
        Self::new()
    }
}

use std::fmt;

#[derive(Debug)]
pub enum MemObserverError {
    IoError(std::io::Error),
    IdNotSet,
}

impl fmt::Display for MemObserverError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MemObserverError::IoError(err) => write!(f, "IO error: {}", err),
            MemObserverError::IdNotSet => write!(f, "Device ID must be set before use!"),
        }
    }
}

impl std::error::Error for MemObserverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MemObserverError::IoError(err) => Some(err),
            MemObserverError::IdNotSet => None,
        }
    }
}

// Converting a std::io::Error into a MemObserverError
impl From<std::io::Error> for MemObserverError {
    fn from(err: std::io::Error) -> MemObserverError {
        MemObserverError::IoError(err)
    }
}

#[async_trait]
impl Observer for MemObserver {
    type Error = MemObserverError;

    async fn register(
        &mut self,
        device_id: &str,
        path: &str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> Result<(), Self::Error> {
        self.channels.register(device_id, path, sender).await;
        Ok(())
    }

    async fn unregister(&mut self, device_id: &str, path: &str) -> Result<(), Self::Error> {
        self.channels.unregister(device_id, path).await;
        Ok(())
    }

    async fn unregister_all(&mut self) -> Result<(), Self::Error> {
        self.channels.unregister_all().await;
        Ok(())
    }

    async fn unregister_device(&mut self, device_id: &str) -> Result<(), Self::Error> {
        self.channels.unregister_device(device_id).await;
        Ok(())
    }

    async fn unregister_device_if_owned(
        &mut self,
        device_id: &str,
        owner: &Arc<Sender<ObserverValue>>,
    ) -> Result<(), Self::Error> {
        self.channels
            .unregister_device_if_owned(device_id, owner)
            .await;
        Ok(())
    }

    async fn write(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error> {
        let new_value = super::path_to_cbor(path, payload);

        tracing::debug!("New value: {:?} for path: {}", new_value, path);

        let current_value = self.db.get(device_id).cloned().unwrap_or(Value::Null);

        let value = if current_value != Value::Null {
            let mut merged_value = current_value.clone();
            super::merge_cbor(&mut merged_value, &new_value);
            tracing::debug!("Merged value: {:?}", merged_value);
            merged_value
        } else {
            new_value
        };

        // Notify observers of changes
        self.channels
            .notify(device_id, &current_value, &value)
            .await;

        // Write merged value
        self.db.insert(device_id.to_string(), value);

        Ok(())
    }

    async fn write_replace(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error> {
        let new_value = super::path_to_cbor(path, payload);
        let current_value = self.db.get(device_id).cloned().unwrap_or(Value::Null);

        self.channels
            .notify(device_id, &current_value, &new_value)
            .await;

        self.db.insert(device_id.to_string(), new_value);

        Ok(())
    }

    async fn read(&mut self, device_id: &str, path: &str) -> Result<Option<Value>, Self::Error> {
        match self.db.get(device_id) {
            Some(value) => {
                tracing::debug!("Got value: {:?}", value);
                let pointer_value = super::cbor_pointer(value, path).cloned();
                tracing::debug!("Pointer value: {:?}", pointer_value);
                Ok(pointer_value)
            }
            None => Ok(None),
        }
    }

    async fn clear(&mut self, device_id: &str) -> Result<(), Self::Error> {
        let _ = self.db.remove(device_id);
        Ok(())
    }

    async fn observer_count(&self, device_id: &str) -> usize {
        self.channels.device_observer_count(device_id).await
    }

    async fn notify(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error> {
        self.channels
            .notify_unconditional(device_id, path, payload)
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::sleep;

    use super::*;

    fn cbor_map(pairs: &[(&str, Value)]) -> Value {
        Value::Map(
            pairs
                .iter()
                .map(|(k, v)| (Value::Text(k.to_string()), v.clone()))
                .collect(),
        )
    }

    lazy_static! {
        // Create test DB
        static ref OBSERVER: MemObserver = MemObserver::new();
    }

    #[tokio::test]
    async fn test_mem_observer_write_and_read() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut observer = OBSERVER.clone();

        // Clear
        observer.clear("123").await.unwrap();

        let test_val = cbor_map(&[("test_key", Value::Text("test_value".into()))]);

        // Write data to path
        observer
            .write("123", "/test_path", &test_val)
            .await
            .unwrap();

        // Read the path
        let result = observer.read("123", "/test_path").await.unwrap();
        assert_eq!(result, Some(test_val.clone()));

        // Write data to nested path
        observer
            .write("123", "/test_path/second_level", &test_val)
            .await
            .unwrap();

        // Read the nested path
        let result = observer
            .read("123", "/test_path/second_level")
            .await
            .unwrap();
        assert_eq!(result, Some(test_val.clone()));

        let result = observer.read("123", "/test_path").await.unwrap();
        assert_eq!(
            result,
            Some(cbor_map(&[
                ("test_key", Value::Text("test_value".into())),
                ("second_level", test_val.clone()),
            ]))
        );
    }

    #[tokio::test]
    async fn test_mem_observer_observe_and_write() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let mut observer = OBSERVER.clone();
        observer.clear("123").await.unwrap();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<ObserverValue>(10);

        let expected = cbor_map(&[("test_key", Value::Text("test_value".into()))]);
        let fut = tokio::spawn(async move {
            if let Some(r) = rx.recv().await {
                assert_eq!(r.value, expected);
                assert_eq!(r.path, "/observe_and_write".to_string());
            }
        });

        sleep(Duration::from_secs(1)).await;

        observer
            .register("123", "/observe_and_write", Arc::new(tx.clone()))
            .await
            .unwrap();

        let test_val = cbor_map(&[("test_key", Value::Text("test_value".into()))]);
        observer
            .write("123", "/observe_and_write", &test_val)
            .await
            .unwrap();

        observer
            .write(
                "123",
                "/observe",
                &cbor_map(&[("test", Value::Text("mest".into()))]),
            )
            .await
            .unwrap();

        fut.await.unwrap();

        observer
            .unregister("123", "/observe_and_write")
            .await
            .unwrap();
        assert_eq!(observer.channels.device_observer_count("123").await, 0);

        observer
            .register("123", "/observe_and_write", Arc::new(tx.clone()))
            .await
            .unwrap();

        observer.unregister_all().await.unwrap();
        assert!(observer.channels.is_empty().await);
    }

    #[tokio::test]
    async fn test_notify_sends_duplicate_payloads() {
        let mut observer = MemObserver::new();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<ObserverValue>(10);

        observer
            .register("dev1", "/cmd", Arc::new(tx))
            .await
            .unwrap();

        let payload = cbor_map(&[("action", Value::Text("reboot".into()))]);

        observer.notify("dev1", "/cmd", &payload).await.unwrap();
        observer.notify("dev1", "/cmd", &payload).await.unwrap();

        let first = rx.recv().await.expect("first notification");
        assert_eq!(first.value, payload);
        assert_eq!(first.path, "/cmd");

        let second = rx.recv().await.expect("second notification");
        assert_eq!(second.value, payload);
        assert_eq!(second.path, "/cmd");
    }
}
