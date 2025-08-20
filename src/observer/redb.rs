use std::{collections::HashMap, fmt, sync::Arc};

use async_trait::async_trait;
use futures::future;
use redb::ReadableDatabase;
use serde_json::Value;
use tokio::sync::{
    RwLock,
    mpsc::{Sender, channel},
};

use super::{Observer, ObserverValue};

// Type aliases to reduce complexity warnings
type ObserverSender = Arc<Sender<ObserverValue>>;
type PathChannels = HashMap<String, ObserverSender>;
type DeviceChannels = HashMap<String, PathChannels>;

// Table definition for storing device data
const DATA_TABLE: redb::TableDefinition<&str, &str> = redb::TableDefinition::new("device_data");

/// Normalizes a path to JSON pointer format by ensuring it starts with '/'
fn normalize_json_pointer(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", path)
    }
}

/// Validates a JSON pointer path for security and correctness
fn validate_json_pointer_path(path: &str) -> Result<String, RedbObserverError> {
    // Reject dangerous patterns
    if path.contains("..") || path.contains('\x00') || path.contains('\\') {
        return Err(RedbObserverError::SecurityError(
            "Path traversal attempt detected".to_string(),
        ));
    }

    // Check for excessive depth (prevent DoS)
    let depth = path.split('/').filter(|s| !s.is_empty()).count();
    if depth > 10 {
        return Err(RedbObserverError::SecurityError(
            "Path too deep".to_string(),
        ));
    }

    // Normalize the path
    let normalized = normalize_json_pointer(path);

    // Additional validation for control characters
    if normalized.chars().any(|c| c.is_control() && c != '\t') {
        return Err(RedbObserverError::SecurityError(
            "Invalid characters in path".to_string(),
        ));
    }

    Ok(normalized)
}

#[derive(Clone, Debug)]
pub struct RedbObserver {
    pub db: Arc<redb::Database>,
    channel: Option<Sender<()>>,
    // Changed to store channels by device_id and then by path
    channels: Arc<RwLock<DeviceChannels>>, // device_id -> path -> channel
}

impl RedbObserver {
    pub fn new(path: &str) -> Result<Self, RedbObserverError> {
        let db = redb::Database::create(path)?;

        // Initialize the table
        {
            let write_txn = db.begin_write()?;
            {
                let _table = write_txn.open_table(DATA_TABLE)?;
            }
            write_txn.commit()?;
        }

        Ok(Self {
            db: Arc::new(db),
            channel: None,
            channels: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

#[derive(Debug)]
pub enum RedbObserverError {
    DatabaseError(redb::DatabaseError),
    TransactionError(redb::TransactionError),
    TableError(redb::TableError),
    StorageError(redb::StorageError),
    CommitError(redb::CommitError),
    JsonError(serde_json::Error),
    SecurityError(String),
    IdNotSet,
}

impl fmt::Display for RedbObserverError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RedbObserverError::DatabaseError(err) => write!(f, "Database error: {}", err),
            RedbObserverError::TransactionError(err) => write!(f, "Transaction error: {}", err),
            RedbObserverError::TableError(err) => write!(f, "Table error: {}", err),
            RedbObserverError::StorageError(err) => write!(f, "Storage error: {}", err),
            RedbObserverError::CommitError(err) => write!(f, "Commit error: {}", err),
            RedbObserverError::JsonError(err) => write!(f, "JSON error: {}", err),
            RedbObserverError::SecurityError(msg) => write!(f, "Security error: {}", msg),
            RedbObserverError::IdNotSet => write!(f, "Device ID must be set before use"),
        }
    }
}

impl std::error::Error for RedbObserverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RedbObserverError::DatabaseError(err) => Some(err),
            RedbObserverError::TransactionError(err) => Some(err),
            RedbObserverError::TableError(err) => Some(err),
            RedbObserverError::StorageError(err) => Some(err),
            RedbObserverError::CommitError(err) => Some(err),
            RedbObserverError::JsonError(err) => Some(err),
            RedbObserverError::SecurityError(_) => None,
            RedbObserverError::IdNotSet => None,
        }
    }
}

// Converting redb errors into RedbObserverError
impl From<redb::DatabaseError> for RedbObserverError {
    fn from(err: redb::DatabaseError) -> RedbObserverError {
        RedbObserverError::DatabaseError(err)
    }
}

impl From<redb::TransactionError> for RedbObserverError {
    fn from(err: redb::TransactionError) -> RedbObserverError {
        RedbObserverError::TransactionError(err)
    }
}

impl From<redb::TableError> for RedbObserverError {
    fn from(err: redb::TableError) -> RedbObserverError {
        RedbObserverError::TableError(err)
    }
}

impl From<redb::StorageError> for RedbObserverError {
    fn from(err: redb::StorageError) -> RedbObserverError {
        RedbObserverError::StorageError(err)
    }
}

impl From<redb::CommitError> for RedbObserverError {
    fn from(err: redb::CommitError) -> RedbObserverError {
        RedbObserverError::CommitError(err)
    }
}

// Converting a serde_json::Error into a RedbObserverError
impl From<serde_json::Error> for RedbObserverError {
    fn from(err: serde_json::Error) -> RedbObserverError {
        RedbObserverError::JsonError(err)
    }
}

#[async_trait]
impl Observer for RedbObserver {
    type Error = RedbObserverError;

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

        // Check if task exists. There should only be one per observer
        if self.channel.is_none() {
            // Create channel for closing when unregistered
            let (tx, mut rx) = channel::<()>(1);

            // Cloned id for the watcher task
            let id = device_id.to_string();

            // Save channel
            self.channel = Some(tx);

            // Clones for the spawned task
            let _channels = self.channels.clone();

            // Create a new task for the observer
            // Note: redb doesn't have built-in change watching like sled,
            // so this task only handles cleanup when unregistered.
            // All change notifications are handled in the write() method.
            tokio::spawn(async move {
                tokio::select! {
                    _ = async {
                        log::debug!("Starting redb watcher for device: {}", id);

                        // Wait for shutdown signal - no polling needed since redb
                        // doesn't support external change watching and all changes
                        // go through our write() method which handles notifications directly.

                        future::pending::<()>().await;
                    } => {}
                    _ = rx.recv() => {
                        log::debug!("Terminating redb subscriber for device: {}", id);
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
        let value = {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(DATA_TABLE)?;

            if let Some(stored_value) = table.get(device_id)? {
                let stored_str = stored_value.value();
                let stored_value: Result<Value, _> = serde_json::from_str(stored_str);

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
                        log::warn!("Unable to deserialize. Err: {}", e);
                        new_value
                    }
                }
            } else {
                new_value
            }
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
                let json_pointer = match validate_json_pointer_path(obs_path) {
                    Ok(path) => path,
                    Err(e) => {
                        log::warn!("Invalid observer path '{}': {}", obs_path, e);
                        continue;
                    }
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
        let value_str = serde_json::to_string(&value)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(DATA_TABLE)?;
            table.insert(device_id, value_str.as_str())?;
        }
        write_txn.commit()?;

        log::debug!("Value successfully written to redb");

        Ok(())
    }

    async fn read(&mut self, device_id: &str, path: &str) -> Result<Option<Value>, Self::Error> {
        // Validate path for security
        let validated_path = validate_json_pointer_path(path)?;

        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DATA_TABLE)?;

        match table.get(device_id)? {
            Some(value) => {
                let value_str = value.value();
                let value: Value = serde_json::from_str(value_str)?;

                log::debug!("Got value for validated path");

                // Get the value at the indicated path
                let pointer_value = value.pointer(&validated_path).cloned();

                log::debug!("Pointer value: {:?}", pointer_value);

                Ok(pointer_value)
            }
            None => Ok(None),
        }
    }

    async fn clear(&mut self, device_id: &str) -> Result<(), Self::Error> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(DATA_TABLE)?;
            table.remove(device_id)?;
        }
        write_txn.commit()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn test_redb_observer_write_and_read() {
        let _ = env_logger::try_init();

        let mut observer = RedbObserver::new("test_write_read.redb").unwrap();

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
    async fn test_redb_observer_observe_and_write() {
        let _ = env_logger::try_init();

        // Create test DB
        let mut observer = RedbObserver::new("test_observe_write.redb").unwrap();

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
        assert!(
            !observer
                .channels
                .read()
                .await
                .get("123")
                .map(|device_channels| device_channels.contains_key("/observe_and_write"))
                .unwrap_or(false)
        );
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
