use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{
    mpsc::{channel, Sender},
    RwLock,
};

use super::Observer;

#[derive(Clone)]
pub struct SledObserver {
    pub db: sled::Db,
    id: String,
    channel: Option<Sender<()>>,
    channels: Arc<RwLock<HashMap<String, Arc<Sender<Value>>>>>,
}

impl SledObserver {
    pub fn new(path: &str) -> Self {
        Self {
            db: sled::open(path).unwrap(),
            channel: None,
            channels: Arc::new(RwLock::new(HashMap::new())),
            id: String::new(),
        }
    }
}

#[async_trait]
impl Observer for SledObserver {
    async fn set_id(&mut self, id: String) {
        self.id = id;
    }

    async fn register(&mut self, path: String, sender: Arc<Sender<Value>>) {
        // Add to channels
        self.channels.write().await.insert(path.clone(), sender);

        // Check if task exists. Theree should only be one per observer
        if self.channel.is_none() {
            // Create channel for closing when unregistered
            let (tx, mut rx) = channel::<()>(1);

            log::info!("Watching id: {}", self.id);

            // Subscriber to the device ID only
            let mut sub = self.db.watch_prefix(self.id.clone());

            // Save channel
            self.channel = Some(tx);

            // Clones
            let id = self.id.clone();
            let channels = self.channels.clone();

            // Create a new thread for the observer
            tokio::spawn(async move {
                tokio::select! {
                    _ = async {

                        log::info!("Starting wait..");
                        while let Some(sled::Event::Insert { key, value }) = (&mut sub).await {

                            log::info!("Got event for {} with value: {}", String::from_utf8(key.to_vec()).unwrap(), String::from_utf8(value.to_vec()).unwrap());

                            // Check to make sure they're equal
                            if key == id
                            {
                                // To JSON
                                let value:  Result<Value, _> =  serde_json::from_slice(&value);

                                // Make sure the value is valid JSON
                                match value {
                                    Ok(value) => {
                                        // Iterate through all subscribed channels
                                        for (path,sender) in channels.read().await.iter() {

                                            // Get the pointed value
                                            if let Some(value) = value.pointer(&path) {
                                                // Send the value..
                                                let _ = sender.send(value.clone()).await;
                                            }

                                        }
                                    }
                                    Err(e)=>log::warn!("Unable to fetch value from db. Err: {}",e)
                                };
                            }
                        }

                        log::info!("thread done");
                    } => {}
                    _ = rx.recv() => {
                        log::info!("Terminating subscriber for: {}", path);
                    }
                }
            });
        }
    }

    async fn unregister(&mut self, path: String) {
        // Remove single entry
        self.channels.write().await.remove(&path);

        // If channels is empty stop task
        if self.channels.read().await.is_empty() {
            if let Some(channel) = &self.channel {
                let _ = channel.send(()).await;
            }

            self.channel = None;
        }
    }

    async fn unregister_all(&mut self) {
        // Remove all entries
        self.channels.write().await.clear();

        // Cancel task
        if let Some(channel) = &self.channel {
            let _ = channel.send(()).await;

            self.channel = None;
        }
    }

    /// Function includes a read and then write since we want to merge
    ///
    /// Assumes if there is a path then payload is the value at that path
    /// If no path it's the whole object..
    async fn write(&mut self, path: String, payload: Value) {
        // Set value at correct provided path
        let new_value = super::path_to_json(&path, &payload);

        log::info!("New value: {:?}", new_value);

        // Check if we have a value and update the pointer
        let value = if let Ok(Some(value)) = self.db.get(&self.id) {
            let value: Result<Value, _> = serde_json::from_slice(&value);

            match value {
                Ok(value) => {
                    let mut merged_value = value.clone();

                    // Perform merge
                    super::merge_json(&mut merged_value, &new_value);

                    log::info!("Merged value: {:?}", merged_value);

                    // Return merged result
                    merged_value
                }
                Err(e) => {
                    log::warn!("Unable to serialize. Err: {}", e);

                    new_value
                }
            }
        } else {
            new_value
        };

        log::info!("Value to write: {:?}", value);

        // Then write it back
        match serde_json::to_vec(&value) {
            Ok(v) => match self.db.insert(&self.id, v) {
                Ok(v) => {
                    log::debug!("Value set: {:?}", v);
                }
                Err(e) => {
                    log::error!("Error writing to sled: {}", e);
                }
            },
            Err(e) => log::warn!("Unable to convert payload to bytes. Err: {}", e),
        };
    }

    async fn read(&mut self, path: String) -> Option<Value> {
        match self.db.get(&self.id) {
            Ok(Some(value)) => {
                let value: Result<Value, _> = serde_json::from_slice(&value);

                match value {
                    Ok(value) => {
                        log::info!("Got value: {:?}", value);

                        // Get the value ad the indicated path
                        let pointer_value = value.pointer(&path).cloned();

                        log::info!("Pointer value: {:?}", pointer_value);

                        pointer_value
                    }
                    Err(e) => {
                        log::warn!("Unable to serialize. Err: {}", e);
                        None
                    }
                }
            }
            Ok(None) => None,
            Err(e) => {
                log::error!("Error reading from sled: {}", e);
                None
            }
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
                static ref OBSERVER: SledObserver = SledObserver::new("test_db");
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
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        let fut = tokio::spawn(async move {
            if let Some(r) = rx.recv().await {
                assert_eq!(r, json!({"test_key": "test_value"}));
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
