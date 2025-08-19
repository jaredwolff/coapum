use std::{collections::HashMap, fmt, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{
    mpsc::{channel, Sender},
    RwLock,
};

use super::{Observer, ObserverValue};

// Type aliases to reduce complexity warnings
type ObserverSender = Arc<Sender<ObserverValue>>;
type PathChannels = HashMap<String, ObserverSender>;
type DeviceChannels = HashMap<String, PathChannels>;

#[derive(Clone, Debug)]
pub struct SledObserver {
    pub db: sled::Db,
    channel: Option<Sender<()>>,
    // Changed to store channels by device_id and then by path
    channels: Arc<RwLock<DeviceChannels>>, // device_id -> path -> channel
}

impl SledObserver {
    pub fn new(path: &str) -> Self {
        Self {
            db: sled::open(path).unwrap(),
            channel: None,
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[derive(Debug)]
pub enum SledObserverError {
    SledError(sled::Error),
    JsonError(serde_json::Error),
    IdNotSet,
}

impl fmt::Display for SledObserverError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SledObserverError::SledError(err) => write!(f, "Sled error: {}", err),
            SledObserverError::JsonError(err) => write!(f, "JSON error: {}", err),
            SledObserverError::IdNotSet => write!(f, "Device ID must be set before use!"),
        }
    }
}

impl std::error::Error for SledObserverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SledObserverError::SledError(err) => Some(err),
            SledObserverError::JsonError(err) => Some(err),
            SledObserverError::IdNotSet => None,
        }
    }
}

// Converting a std::io::Error into a SledObserverError
impl From<sled::Error> for SledObserverError {
    fn from(err: sled::Error) -> SledObserverError {
        SledObserverError::SledError(err)
    }
}

// Converting a serde_json::Error into a SledObserverError
impl From<serde_json::Error> for SledObserverError {
    fn from(err: serde_json::Error) -> SledObserverError {
        SledObserverError::JsonError(err)
    }
}

#[async_trait]
impl Observer for SledObserver {
    type Error = SledObserverError;

    async fn register(
        &mut self,
        device_id: &str,
        path: &str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> Result<(), Self::Error> {
        // Add to channels
        let mut channels = self.channels.write().await;
        channels
            .entry(device_id.to_string())
            .or_insert_with(HashMap::new)
            .insert(path.to_string(), sender);

        log::debug!(
            "Registered observer for device '{}' at path '{}'",
            device_id,
            path
        );

        // Check if task exists. Theree should only be one per observer
        if self.channel.is_none() {
            // Create channel for closing when unregistered
            let (tx, mut rx) = channel::<()>(1);

            // Subscriber to the device ID only
            let mut sub = self.db.watch_prefix(device_id);

            // Cloned id
            let id = device_id.to_string();

            // Save channel
            self.channel = Some(tx);

            // Clones
            let channels = self.channels.clone();

            // Create a new thread for the observer
            tokio::spawn(async move {
                tokio::select! {
                    _ = async {

                        log::debug!("Starting sled watcher for device: {}", id);
                        while let Some(sled::Event::Insert { key, value }) = (&mut sub).await {

                            log::debug!("Got sled event for {} with value: {}", String::from_utf8(key.to_vec()).unwrap(), String::from_utf8(value.to_vec()).unwrap());

                            // Check to make sure they're equal
                            if key == id
                            {
                                // To JSON
                                let value:  Result<Value, _> =  serde_json::from_slice(&value);

                                // Make sure the value is valid JSON
                                match value {
                                    Ok(value) => {

                                        let channels = channels.read().await;

                                        log::debug!("Looking for observers for device '{}'", id);

                                        if let Some(device_channels) = channels.get(&id) {
                                            log::debug!("Found device '{}' with {} observers", id, device_channels.len());
                                            // Iterate through all subscribed channels for this device
                                            for (obs_path, sender) in device_channels.iter() {
                                                // Convert path to JSON pointer format (add leading slash if not present)
                                                let json_pointer = if obs_path.starts_with('/') {
                                                    obs_path.clone()
                                                } else {
                                                    format!("/{}", obs_path)
                                                };

                                                // Get the pointed value
                                                if let Some(pointed_value) = value.pointer(&json_pointer) {
                                                    let out = ObserverValue {
                                                        value: pointed_value.clone(),
                                                        path: obs_path.clone(),
                                                    };

                                                    // Send the value..
                                                    let _ = sender.send(out).await;
                                                }
                                            }
                                        } else {
                                            log::warn!("No observers found for device '{}'", id);
                                        }
                                    }
                                    Err(e)=>log::warn!("Unable to fetch value from db. Err: {}",e)
                                };
                            }
                        }

                        log::debug!("sled watcher thread done for device: {}", id);
                    } => {}
                    _ = rx.recv() => {
                        log::debug!("Terminating sled subscriber for device: {}", id);
                    }
                }
            });
        }

        Ok(())
    }

    async fn unregister(&mut self, device_id: &str, path: &str) -> Result<(), Self::Error> {
        // Remove single entry
        let mut channels = self.channels.write().await;
        if let Some(device_channels) = channels.get_mut(device_id) {
            device_channels.remove(path);
            if device_channels.is_empty() {
                channels.remove(device_id);
            }
        }

        // If channels is empty stop task
        if channels.is_empty() {
            if let Some(channel) = &self.channel {
                let _ = channel.send(()).await;
            }

            self.channel = None;
        }

        Ok(())
    }

    async fn unregister_all(&mut self) -> Result<(), Self::Error> {
        // Remove all entries
        {
            self.channels.write().await.clear();
        }

        // Cancel task
        if let Some(channel) = &self.channel {
            let _ = channel.send(()).await;

            self.channel = None;
        }

        Ok(())
    }

    /// Function includes a read and then write since we want to merge
    ///
    /// Assumes if there is a path then payload is the value at that path
    /// If no path it's the whole object..
    async fn write(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error> {
        // Set value at correct provided path
        let new_value = super::path_to_json(path, payload);

        log::debug!("New value: {:?} for path: {}", new_value, path);

        let mut current_value = Value::Null;

        // Check if we have a value and update the pointer
        let value = if let Ok(Some(stored_value)) = self.db.get(device_id) {
            let stored_value: Result<Value, _> = serde_json::from_slice(&stored_value);

            match stored_value {
                Ok(stored_value) => {
                    current_value = stored_value.clone();

                    // Create merged path
                    let mut merged_value = stored_value;

                    // Perform merge
                    super::merge_json(&mut merged_value, &new_value);

                    log::debug!("Merged value: {:?}", merged_value);

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

        // Only check observers for this specific device
        let channels = self.channels.read().await;

        log::debug!(
            "Looking for observers for device '{}' with write to path '{}'",
            device_id,
            path
        );
        log::debug!(
            "Currently registered devices: {:?}",
            channels.keys().collect::<Vec<_>>()
        );

        if let Some(device_channels) = channels.get(device_id) {
            log::debug!(
                "Found device '{}' with {} observers",
                device_id,
                device_channels.len()
            );
            // Callback if there were differences
            for (obs_path, sender) in device_channels.iter() {
                // Check if the observer path is affected by this write
                // Convert path to JSON pointer format (add leading slash if not present)
                let json_pointer = if obs_path.starts_with('/') {
                    obs_path.clone()
                } else {
                    format!("/{}", obs_path)
                };
                let current_value_at_path = current_value.pointer(&json_pointer);
                let incoming_value_at_path = value.pointer(&json_pointer);

                log::debug!("Comparing paths: {} for device: {}", obs_path, device_id);
                log::debug!("Current value at path: {:?}", current_value_at_path);
                log::debug!("Incoming value at path: {:?}", incoming_value_at_path);

                // Get the pointed value
                if current_value_at_path != incoming_value_at_path {
                    log::debug!(
                        "Value changed at path: {} for device: {}",
                        obs_path,
                        device_id
                    );

                    let notification_value = match incoming_value_at_path {
                        Some(value) => value.clone(),
                        None => Value::Null,
                    };

                    // If not equal then send the value from the path
                    if let Err(e) = sender
                        .send(ObserverValue {
                            path: obs_path.clone(),
                            value: notification_value,
                        })
                        .await
                    {
                        log::warn!(
                            "Failed to send observer notification for device {} path {}: {}",
                            device_id,
                            obs_path,
                            e
                        );
                    }
                }
            }
        } else {
            log::warn!("No observers found for device '{}'", device_id);
        }

        log::debug!("Value to write: {:?}", value);

        // Then write it back
        match serde_json::to_vec(&value) {
            Ok(v) => match self.db.insert(device_id, v) {
                Ok(v) => {
                    log::debug!("Value set: {:?}", v);
                }
                Err(e) => {
                    log::error!("Error writing to sled: {}", e);
                }
            },
            Err(e) => log::warn!("Unable to convert payload to bytes. Err: {}", e),
        };

        Ok(())
    }

    async fn read(&mut self, device_id: &str, path: &str) -> Result<Option<Value>, Self::Error> {
        match self.db.get(device_id) {
            Ok(Some(value)) => {
                let value: Value = serde_json::from_slice(&value)?;

                log::debug!("Got value: {:?}", value);

                // Get the value ad the indicated path
                let pointer_value = value.pointer(path).cloned();

                log::debug!("Pointer value: {:?}", pointer_value);

                Ok(pointer_value)
            }
            Ok(None) => Ok(None),
            Err(e) => {
                log::error!("Error reading from sled: {}", e);
                Err(e.into())
            }
        }
    }

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
                static ref OBSERVER: SledObserver = SledObserver::new("test.db");
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
            .get("123")
            .map(|device_channels| device_channels.contains_key("/observe_and_write"))
            .unwrap_or(false));
        assert!(observer.channel.is_none());

        observer
            .register("123", "/observe_and_write", Arc::new(tx.clone()))
            .await
            .unwrap();

        // Unregister all
        observer.unregister_all().await.unwrap();
        assert!(observer.channels.read().await.is_empty());
        assert!(observer.channel.is_none());
    }
}
