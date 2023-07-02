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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_subscription(
        &mut self,
        subscriber_id: String,
        device_id: String,
        path: String,
    ) -> broadcast::Receiver<Value> {
        if !self.channels.contains_key(&device_id) {
            self.channels.insert(device_id.clone(), HashMap::new());
        }
        let device_channels = self.channels.get_mut(&device_id).unwrap();
        if !device_channels.contains_key(&path) {
            device_channels.insert(path.clone(), HashMap::new());
        }
        let path_channels = device_channels.get_mut(&path).unwrap();
        if !path_channels.contains_key(&subscriber_id) {
            let (tx, _rx) = broadcast::channel(100);
            path_channels.insert(subscriber_id.clone(), tx);
        }

        path_channels[&subscriber_id].subscribe()
    }

    pub fn notify_subscribers(&self, device_id: &str, path: &str, value: Value) {
        if let Some(device_channels) = self.channels.get(device_id) {
            if let Some(path_channels) = device_channels.get(path) {
                for tx in path_channels.values() {
                    let _ = tx.send(value.clone());
                }
            }
        }
    }
}

#[async_trait]
pub trait SubscribableDatabase<D> {
    type Error;

    async fn new(path: D) -> Result<Self, Self::Error>
    where
        Self: Sized;
    async fn set(
        &mut self,
        device_id: &str,
        path: &str,
        new_value: Value,
    ) -> Result<(), Self::Error>;
    async fn subscribe(
        &self,
        subscriber_id: String,
        device_id: String,
        path: String,
    ) -> broadcast::Receiver<Value>;
}

struct MemObserver {
    db: Arc<Mutex<HashMap<String, Value>>>,
    subscriptions: Arc<Mutex<Subscriptions>>,
}

#[async_trait]
impl SubscribableDatabase<()> for MemObserver {
    type Error = sled::Error;

    async fn new(_: ()) -> Result<Self, Self::Error> {
        let db = HashMap::new();
        let subscriptions = Arc::new(Mutex::new(Subscriptions::new()));

        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            subscriptions,
        })
    }

    async fn set(
        &mut self,
        device_id: &str,
        path: &str,
        new_value: Value,
    ) -> Result<(), Self::Error> {
        // Check current value
        let db_lock = self.db.lock().await;
        let current_value = db_lock.get(device_id);

        // New value with path applied
        let new_value = path_to_json(&path, &new_value);

        // If the current value is something...
        let new_value = if let Some(current_value) = current_value.cloned() {
            // Merge
            let mut merged_value = current_value.clone();
            merge_json(&mut merged_value, &new_value);

            // Compare the two
            if !current_value.eq(&new_value) {
                // Compare paths by iterating
                if let Some(paths) = self.subscriptions.lock().await.channels.get(device_id) {
                    for (p, _v) in paths {
                        // Get the pointers
                        let current_pointer = current_value.pointer(p);
                        let new_pointer = new_value.pointer(p);

                        // Compare (TODO: double check this works ok)
                        if current_pointer != new_pointer {
                            // Push to DB
                            self.subscriptions.lock().await.notify_subscribers(
                                device_id,
                                p,
                                merged_value.clone(),
                            );
                        }
                    }
                }
            }

            merged_value
        } else {
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

    async fn subscribe(
        &self,
        subscriber_id: String,
        device_id: String,
        path: String,
    ) -> broadcast::Receiver<Value> {
        self.subscriptions
            .lock()
            .await
            .create_subscription(subscriber_id, device_id, path)
    }
}
