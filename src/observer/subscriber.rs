use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{broadcast, Mutex};

use super::{merge_json, path_to_json};

/// Subscription manager
/// Channels consist of `device_id` with Hash of `path` with nested hash of 'subscriber_id'
#[derive(Default)]
struct Subscriptions {
    channels: HashMap<String, HashMap<String, HashMap<String, broadcast::Sender<Value>>>>,
}

impl Subscriptions {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn delete_subscription(&mut self, subscriber_id: &str, device_id: &str, path: &str) {
        if let Some(device) = self.channels.get_mut(device_id) {
            if let Some(path) = device.get_mut(path) {
                // Unsubscribed automatically
                path.remove(subscriber_id);
            }
        }
    }

    pub fn create_subscription(
        &mut self,
        subscriber_id: &str,
        device_id: &str,
        path: &str,
    ) -> broadcast::Receiver<Value> {
        if !self.channels.contains_key(device_id) {
            self.channels.insert(device_id.to_string(), HashMap::new());
        }
        let device_channels = self.channels.get_mut(device_id).unwrap();
        if !device_channels.contains_key(path) {
            device_channels.insert(path.to_string(), HashMap::new());
        }
        let path_channels = device_channels.get_mut(path).unwrap();
        if !path_channels.contains_key(subscriber_id) {
            let (tx, _rx) = broadcast::channel(100);
            path_channels.insert(subscriber_id.to_string(), tx);
        }

        path_channels[subscriber_id].subscribe()
    }

    pub fn notify_subscribers(&self, device_id: &str, path: &str, value: Value) {
        if let Some(device_channels) = self.channels.get(device_id) {
            if let Some(path_channels) = device_channels.get(path) {
                log::info!("Notifying to all listening to: {}", path);

                for (sub, tx) in path_channels {
                    log::info!("Sending to: {}", sub);
                    let _ = tx.send(value.clone());
                }
            }
        }
    }
}

#[async_trait]
pub trait SubscribableDatabase {
    type Error;

    async fn set(
        &mut self,
        device_id: &str,
        path: &str,
        new_value: Value,
    ) -> Result<(), Self::Error>;
    async fn subscribe(
        &self,
        subscriber_id: &str,
        device_id: &str,
        path: &str,
    ) -> broadcast::Receiver<Value>;
    async fn unsubscribe(&mut self, subscriber_id: &str, device_id: &str, path: &str);
}

#[derive(Clone)]
struct MemObserver {
    db: Arc<Mutex<HashMap<String, Value>>>,
    subscriptions: Arc<Mutex<Subscriptions>>,
}

impl MemObserver {
    #[allow(dead_code)]
    fn new() -> Result<Self, std::io::Error> {
        let db = HashMap::new();
        let subscriptions = Arc::new(Mutex::new(Subscriptions::new()));

        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            subscriptions,
        })
    }
}

#[async_trait]
impl SubscribableDatabase for MemObserver {
    type Error = std::io::Error;

    async fn set(
        &mut self,
        device_id: &str,
        path: &str,
        new_value: Value,
    ) -> Result<(), Self::Error> {
        // Check current value
        let current_value = {
            let db_lock = self.db.lock().await;
            db_lock.get(device_id).cloned()
        };

        // New value with path applied
        let new_value = path_to_json(path, &new_value);

        log::info!("New: {:?}", new_value);

        // If the current value is something...
        let new_value = if let Some(current_value) = current_value {
            // Merge
            let mut merged_value = current_value.clone();
            merge_json(&mut merged_value, &new_value);

            log::info!("Merged: {:?}", merged_value);

            // Compare the two
            if !current_value.eq(&new_value) {
                // Compare paths by iterating
                let subscriptions = self.subscriptions.lock().await;
                if let Some(paths) = subscriptions.channels.get(device_id) {
                    for p in paths.keys() {
                        log::info!("Path: {}", p);

                        // Get the pointers
                        let current_pointer = current_value.pointer(p);
                        let new_pointer = new_value.pointer(p);

                        // Compare (TODO: double check this works ok)
                        if current_pointer != new_pointer && new_pointer.is_some() {
                            log::info!("Notify: {} with: {:?}", p, new_pointer);

                            // Notify
                            subscriptions.notify_subscribers(
                                device_id,
                                p,
                                new_pointer.cloned().unwrap(),
                            );
                        }
                    }
                }
            }

            merged_value
        } else {
            let new_pointer = new_value.pointer(path).cloned().unwrap();

            log::info!("Notify: {} with: {:?}", path, new_pointer);

            self.subscriptions
                .lock()
                .await
                .notify_subscribers(device_id, path, new_pointer);

            new_value
        };

        // Write merged value
        {
            self.db
                .lock()
                .await
                .insert(device_id.to_string(), new_value.clone());
        }
        Ok(())
    }

    async fn unsubscribe(&mut self, subscriber_id: &str, device_id: &str, path: &str) {
        self.subscriptions
            .lock()
            .await
            .delete_subscription(subscriber_id, device_id, path)
    }

    async fn subscribe(
        &self,
        subscriber_id: &str,
        device_id: &str,
        path: &str,
    ) -> broadcast::Receiver<Value> {
        self.subscriptions
            .lock()
            .await
            .create_subscription(subscriber_id, device_id, path)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use serde_json::json;
    use tokio::sync::broadcast::error::RecvError;

    #[tokio::test]
    async fn test_subscribable_database() {
        let _ = env_logger::try_init();

        let mut db = MemObserver::new().unwrap();

        let device_id = "device1";
        let path = "/data";

        let mut receiver1 = db.subscribe("subscriber1", device_id, path).await;
        let mut receiver2 = db.subscribe("subscriber2", device_id, path).await;

        // Simulate a change to the subscribed data path
        let new_value = json!({
            "data": "new_value",
        });

        log::info!("Set to: {:?}", new_value);

        db.set(&device_id, &path, new_value.clone()).await.unwrap();

        let next = receiver1.recv().await.unwrap();
        assert_eq!(new_value, next);

        let next = receiver2.recv().await.unwrap();
        assert_eq!(new_value, next);

        // New new value
        let new_value = json!({
            "data": "new_new_value",
        });

        log::info!("Set to: {:?}", new_value);

        db.set(&device_id, &path, new_value.clone()).await.unwrap();

        let next = receiver1.recv().await.unwrap();
        assert_eq!(new_value, next);

        let next = receiver2.recv().await.unwrap();
        assert_eq!(new_value, next);

        log::info!("Set to: {:?}", new_value);

        // Should not notify
        db.set(&device_id, &path, new_value.clone()).await.unwrap();

        // Receeivers should be closed since we dropped Sender(s)
        db.unsubscribe("subscriber1", device_id, path).await;

        assert_eq!(receiver1.recv().await, Err(RecvError::Closed));

        db.unsubscribe("subscriber2", device_id, path).await;

        assert_eq!(receiver2.recv().await, Err(RecvError::Closed));
    }
}
