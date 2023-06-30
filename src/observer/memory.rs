use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{mpsc::Sender, RwLock};

use super::{Observer, ObserverValue};

#[derive(Clone)]
pub struct MemObserver {
    db: HashMap<String, Value>,
    id: String,
    channels: Arc<RwLock<HashMap<String, Arc<Sender<ObserverValue>>>>>,
}

impl MemObserver {
    pub fn new() -> Self {
        Self {
            db: HashMap::new(),
            channels: Arc::new(RwLock::new(HashMap::new())),
            id: String::new(),
        }
    }
}

#[async_trait]
impl Observer for MemObserver {
    async fn set_id(&mut self, id: String) {
        self.id = id;
    }

    async fn register(&mut self, path: String, sender: Arc<Sender<ObserverValue>>) {
        // Add to channels
        self.channels.write().await.insert(path.clone(), sender);
    }

    async fn unregister(&mut self, path: String) {
        // Remove single entry
        self.channels.write().await.remove(&path);
    }

    async fn unregister_all(&mut self) {
        // Remove all entries
        self.channels.write().await.clear();
    }

    /// Function includes a read and then write since we want to merge
    ///
    /// Assumes if there is a path then payload is the value at that path
    /// If no path it's the whole object..
    async fn write(&mut self, path: String, payload: Value) {
        // Set value at correct provided path
        let new_value = super::path_to_json(&path, &payload);

        log::info!("New value: {:?}", new_value);

        let mut current_value = Value::Null;

        // Check if we have a value and update the pointer
        let value = if let Some(value) = self.db.get(&self.id) {
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

        // Callback if there were differences
        for (path, sender) in self.channels.read().await.iter() {
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
                let _ = sender
                    .send(ObserverValue {
                        path: path.clone(),
                        value,
                    })
                    .await;
            }
        }

        // Then write it back
        let _ = self.db.insert(self.id.clone(), value);
    }

    async fn read(&mut self, path: String) -> Option<Value> {
        match self.db.get(&self.id) {
            Some(value) => {
                log::info!("Got value: {:?}", value);

                // Get the value ad the indicated path
                let pointer_value = value.pointer(&path).cloned();

                log::info!("Pointer value: {:?}", pointer_value);

                pointer_value
            }
            None => None,
        }
    }

    async fn clear(&mut self) {
        let _ = self.db.remove(&self.id);
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

        // Set ID for "device"
        observer.set_id("test_id".to_string()).await;

        // Clear
        observer.clear().await;

        // Write data to path
        observer
            .write("/test_path".to_string(), json!({"test_key": "test_value"}))
            .await;

        // Read the path
        let result = observer.read("/test_path".to_string()).await;
        assert_eq!(result, Some(json!({"test_key": "test_value"})));

        // Write data to path
        observer
            .write(
                "/test_path/second_level".to_string(),
                json!({"test_key": "test_value"}),
            )
            .await;

        // Read the path
        let result = observer.read("/test_path/second_level".to_string()).await;
        assert_eq!(result, Some(json!({"test_key": "test_value"})));

        let result = observer.read("/test_path".to_string()).await;
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

        // Set ID for "device"
        observer.set_id("test_id".to_string()).await;

        // Clear before work
        observer.clear().await;

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
            .register("/observe_and_write".to_string(), Arc::new(tx.clone()))
            .await;

        // Write data to path
        observer
            .write(
                "/observe_and_write".to_string(),
                json!({"test_key": "test_value"}),
            )
            .await;

        observer
            .write("/observe".to_string(), json!({"test": "mest"}))
            .await;

        let _ = fut.await;
    }
}
