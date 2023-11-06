use std::{collections::HashMap, fmt, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{
    mpsc::{channel, Sender},
    RwLock,
};

use super::{Observer, ObserverValue};

#[derive(Clone, Debug)]
pub struct SledObserver {
    pub db: sled::Db,
    channel: Option<Sender<()>>,
    channels: Arc<RwLock<HashMap<String, Arc<Sender<ObserverValue>>>>>,
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
        {
            self.channels.write().await.insert(path.to_string(), sender);
        }

        // Check if task exists. Theree should only be one per observer
        if self.channel.is_none() {
            // Create channel for closing when unregistered
            let (tx, mut rx) = channel::<()>(1);

            // Subscriber to the device ID only
            let mut sub = self.db.watch_prefix(device_id);

            // Cloned id
            let id = device_id.to_string();
            let path = path.to_string();

            // Save channel
            self.channel = Some(tx);

            // Clones
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

                                        let channels = {
                                            channels.read().await
                                        };

                                        // Iterate through all subscribed channels
                                        for (path,sender) in channels.iter() {

                                            // Get the pointed value
                                            if let Some(value) = value.pointer(path) {

                                                let out = ObserverValue{
                                                    value: value.clone(),
                                                    path: path.clone()
                                                };

                                                // Send the value..
                                                let _ = sender.send(out).await;
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

        Ok(())
    }

    async fn unregister(&mut self, _device_id: &str, path: &str) -> Result<(), Self::Error> {
        // Remove single entry
        {
            self.channels.write().await.remove(path);
        }

        let channels = { self.channels.read().await };

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

        log::info!("New value: {:?}", new_value);

        // Check if we have a value and update the pointer
        let value = if let Ok(Some(value)) = self.db.get(device_id) {
            let value: Result<Value, _> = serde_json::from_slice(&value);

            match value {
                Ok(value) => {
                    let mut merged_value = value;

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

                log::info!("Got value: {:?}", value);

                // Get the value ad the indicated path
                let pointer_value = value.pointer(path).cloned();

                log::info!("Pointer value: {:?}", pointer_value);

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
            .contains_key("/observe_and_write"));
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
