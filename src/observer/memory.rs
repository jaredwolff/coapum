use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{mpsc::Sender, RwLock};

use super::{Observer, ObserverValue};

/// A memory-based observer that stores data in a HashMap.
#[derive(Clone, Debug)]
pub struct MemObserver {
    db: HashMap<String, Value>, // The HashMap that stores the data.
    channels: Arc<RwLock<HashMap<String, Arc<Sender<ObserverValue>>>>>, // The channels that the observer is registered to.
}

impl MemObserver {
    /// Creates a new instance of `MemObserver`.
    pub fn new() -> Self {
        Self {
            db: HashMap::new(),
            channels: Arc::new(RwLock::new(HashMap::new())),
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

    /// Registers the observer to a channel.
    async fn register(
        &mut self,
        _device_id: &str,
        path: &str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> Result<(), Self::Error> {
        // Add to channels
        self.channels.write().await.insert(path.to_string(), sender);

        Ok(())
    }

    /// Unregisters the observer from a channel.
    async fn unregister(&mut self, _device_id: &str, path: &str) -> Result<(), Self::Error> {
        // Remove single entry
        self.channels.write().await.remove(path);

        Ok(())
    }

    /// Unregisters the observer from all channels.
    async fn unregister_all(&mut self) -> Result<(), Self::Error> {
        // Remove all entries
        self.channels.write().await.clear();

        Ok(())
    }

    /// Writes data to the observer.
    ///
    /// Assumes if there is a path then payload is the value at that path
    /// If no path it's the whole object.
    async fn write(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error> {
        // Set value at correct provided path
        let new_value = super::path_to_json(path, payload);

        log::info!("New value: {:?}", new_value);

        let mut current_value = Value::Null;

        // Check if we have a value and update the pointer
        let value = if let Some(value) = self.db.get(device_id) {
            current_value = value.clone();

            // Create merged path
            let mut merged_value = value.clone();

            // Perform merge
            super::merge_json(&mut merged_value, &new_value);

            log::info!("Merged value: {:?}", merged_value);

            // Return merged result
            merged_value
        } else {
            new_value
        };

        let channels = { self.channels.read().await };

        // Callback if there were differences
        for (path, sender) in channels.iter() {
            let current_value = current_value.pointer(path);
            let incoming_value = value.pointer(path);

            log::info!("Comparing paths!");

            // Get the pointed value
            if current_value != incoming_value {
                log::info!("Different!");

                let value = match incoming_value {
                    Some(value) => value.clone(),
                    None => Value::Null,
                };

                // If not equal then send the value from the path
                sender
                    .send(ObserverValue {
                        path: path.clone(),
                        value,
                    })
                    .await
                    .unwrap();
            }
        }

        // Then write it back
        self.db.insert(device_id.to_string(), value);

        Ok(())
    }

    /// Reads data from the observer.
    async fn read(&mut self, device_id: &str, path: &str) -> Result<Option<Value>, Self::Error> {
        match self.db.get(device_id) {
            Some(value) => {
                log::info!("Got value: {:?}", value);

                // Get the value ad the indicated path
                let pointer_value = value.pointer(path).cloned();

                log::info!("Pointer value: {:?}", pointer_value);

                Ok(pointer_value)
            }
            None => Ok(None),
        }
    }

    /// Clears the observer.
    async fn clear(&mut self, device_id: &str) -> Result<(), Self::Error> {
        let _ = self.db.remove(device_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use tokio::time::sleep;

    use super::*;

    lazy_static! {
        // Create test DB
        static ref OBSERVER: MemObserver = MemObserver::new();
    }

    #[tokio::test]
    async fn test_sled_observer_write_and_read() {
        let _ = env_logger::try_init();

        let mut observer = OBSERVER.clone();

        // Clear
        observer.clear("123").await.unwrap();

        // Write data to path
        observer
            .write("123", "/test_path", &json!({"test_key": "test_value"}))
            .await
            .unwrap();

        // Read the path
        let result = observer.read("123", "/test_path").await.unwrap();
        assert_eq!(result, Some(json!({"test_key": "test_value"})));

        // Write data to path
        observer
            .write(
                "123",
                "/test_path/second_level",
                &json!({"test_key": "test_value"}),
            )
            .await
            .unwrap();

        // Read the path
        let result = observer
            .read("123", "/test_path/second_level")
            .await
            .unwrap();
        assert_eq!(result, Some(json!({"test_key": "test_value"})));

        let result = observer.read("123", "/test_path").await.unwrap();
        assert_eq!(
            result,
            Some(json!({"test_key": "test_value", "second_level": {"test_key": "test_value"}}))
        );
    }

    #[tokio::test]
    async fn test_sled_observer_observe_and_write() {
        let _ = env_logger::try_init();

        // Create test DB
        let mut observer = OBSERVER.clone();

        // Clear before work
        observer.clear("123").await.unwrap();

        // Channel and register
        let (tx, mut rx) = tokio::sync::mpsc::channel::<ObserverValue>(10);

        let fut = tokio::spawn(async move {
            if let Some(r) = rx.recv().await {
                assert_eq!(r.value, json!({"test_key": "test_value"}));
                assert_eq!(r.path, "/observe_and_write".to_string());
            }
        });

        sleep(Duration::from_secs(1)).await;

        observer
            .register("123", "/observe_and_write", Arc::new(tx.clone()))
            .await
            .unwrap();

        // Write data to path
        observer
            .write(
                "123",
                "/observe_and_write",
                &json!({"test_key": "test_value"}),
            )
            .await
            .unwrap();

        observer
            .write("123", "/observe", &json!({"test": "mest"}))
            .await
            .unwrap();

        fut.await.unwrap();

        // Unregister
        observer
            .unregister("123", "/observe_and_write")
            .await
            .unwrap();
        assert!(!observer
            .channels
            .read()
            .await
            .contains_key(&"/observe_and_write".to_string()));

        observer
            .register("123", "/observe_and_write", Arc::new(tx.clone()))
            .await
            .unwrap();

        // Unregister all
        observer.unregister_all().await.unwrap();
        assert!(observer.channels.read().await.is_empty());
    }
}
