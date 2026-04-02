use std::{fmt, sync::Arc};

use async_trait::async_trait;
use ciborium::value::Value;
use futures::future;
use redb::ReadableDatabase;
use tokio::sync::mpsc::{Sender, channel};

use super::{Observer, ObserverChannels, ObserverValue};

// Table definition for storing device data as CBOR bytes
const DATA_TABLE: redb::TableDefinition<&str, &[u8]> = redb::TableDefinition::new("device_data");

#[derive(Clone, Debug)]
pub struct RedbObserver {
    pub db: Arc<redb::Database>,
    channel: Option<Sender<()>>,
    /// Shared channel management for observer notifications.
    pub channels: ObserverChannels,
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
            channels: ObserverChannels::new(),
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
    CborError(String),
    IdNotSet,
    TaskJoinError(String),
}

impl fmt::Display for RedbObserverError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RedbObserverError::DatabaseError(err) => write!(f, "Database error: {}", err),
            RedbObserverError::TransactionError(err) => write!(f, "Transaction error: {}", err),
            RedbObserverError::TableError(err) => write!(f, "Table error: {}", err),
            RedbObserverError::StorageError(err) => write!(f, "Storage error: {}", err),
            RedbObserverError::CommitError(err) => write!(f, "Commit error: {}", err),
            RedbObserverError::CborError(err) => write!(f, "CBOR error: {}", err),
            RedbObserverError::IdNotSet => write!(f, "Device ID must be set before use"),
            RedbObserverError::TaskJoinError(msg) => write!(f, "Task join error: {}", msg),
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
            RedbObserverError::CborError(_) => None,
            RedbObserverError::IdNotSet => None,
            RedbObserverError::TaskJoinError(_) => None,
        }
    }
}

impl From<tokio::task::JoinError> for RedbObserverError {
    fn from(err: tokio::task::JoinError) -> RedbObserverError {
        RedbObserverError::TaskJoinError(err.to_string())
    }
}

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

fn cbor_serialize(value: &Value) -> Result<Vec<u8>, RedbObserverError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf)
        .map_err(|e| RedbObserverError::CborError(e.to_string()))?;
    Ok(buf)
}

fn cbor_deserialize(bytes: &[u8]) -> Result<Value, RedbObserverError> {
    ciborium::de::from_reader(bytes).map_err(|e| RedbObserverError::CborError(e.to_string()))
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
        self.channels.register(device_id, path, sender).await;

        // Spawn watcher task if not already running.
        // Note: redb doesn't have built-in change watching like sled,
        // so this task only handles cleanup when unregistered.
        // All change notifications are handled in the write() method.
        if self.channel.is_none() {
            let (tx, mut rx) = channel::<()>(1);
            let id = device_id.to_string();
            self.channel = Some(tx);

            tokio::spawn(async move {
                tokio::select! {
                    _ = async {
                        tracing::debug!("Starting redb watcher for device: {}", id);
                        future::pending::<()>().await;
                    } => {}
                    _ = rx.recv() => {
                        tracing::debug!("Terminating redb subscriber for device: {}", id);
                    }
                }
            });
        }

        Ok(())
    }

    async fn unregister(&mut self, device_id: &str, path: &str) -> Result<(), Self::Error> {
        let all_empty = self.channels.unregister(device_id, path).await;

        if all_empty {
            if let Some(channel) = &self.channel {
                let _ = channel.send(()).await;
            }
            self.channel = None;
        }

        Ok(())
    }

    async fn unregister_all(&mut self) -> Result<(), Self::Error> {
        self.channels.unregister_all().await;

        if let Some(channel) = &self.channel {
            let _ = channel.send(()).await;
            self.channel = None;
        }

        Ok(())
    }

    async fn unregister_device(&mut self, device_id: &str) -> Result<(), Self::Error> {
        let all_empty = self.channels.unregister_device(device_id).await;

        if all_empty {
            if let Some(channel) = &self.channel {
                let _ = channel.send(()).await;
            }
            self.channel = None;
        }

        Ok(())
    }

    async fn unregister_device_if_owned(
        &mut self,
        device_id: &str,
        owner: &Arc<Sender<ObserverValue>>,
    ) -> Result<(), Self::Error> {
        let all_empty = self
            .channels
            .unregister_device_if_owned(device_id, owner)
            .await;

        if all_empty {
            if let Some(channel) = &self.channel {
                let _ = channel.send(()).await;
            }
            self.channel = None;
        }

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

        // Phase 1: Read existing value and merge (blocking DB read)
        let db = self.db.clone();
        let did = device_id.to_string();
        let nv = new_value.clone();
        let (value, current_value) =
            tokio::task::spawn_blocking(move || -> Result<(Value, Value), RedbObserverError> {
                let mut current_value = Value::Null;
                let value = {
                    let read_txn = db.begin_read()?;
                    let table = read_txn.open_table(DATA_TABLE)?;

                    if let Some(stored_value) = table.get(did.as_str())? {
                        let stored_bytes = stored_value.value();
                        match cbor_deserialize(stored_bytes) {
                            Ok(stored_value) => {
                                current_value = stored_value.clone();
                                let mut merged_value = stored_value;
                                super::merge_cbor(&mut merged_value, &nv);
                                tracing::debug!("Merged value: {:?}", merged_value);
                                merged_value
                            }
                            Err(e) => {
                                tracing::warn!("Unable to deserialize. Err: {}", e);
                                nv
                            }
                        }
                    } else {
                        nv
                    }
                };
                Ok((value, current_value))
            })
            .await??;

        // Notify observers of changes
        self.channels
            .notify(device_id, &current_value, &value)
            .await;

        // Phase 3: Write merged value back (blocking DB write)
        let db = self.db.clone();
        let did = device_id.to_string();
        let cbor_bytes = cbor_serialize(&value)?;
        tokio::task::spawn_blocking(move || -> Result<(), RedbObserverError> {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(DATA_TABLE)?;
                table.insert(did.as_str(), cbor_bytes.as_slice())?;
            }
            write_txn.commit()?;
            tracing::debug!("Value successfully written to redb");
            Ok(())
        })
        .await??;

        Ok(())
    }

    async fn write_replace(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error> {
        let new_value = super::path_to_cbor(path, payload);

        // Phase 1: Read existing value for diffing (blocking DB read)
        let db = self.db.clone();
        let did = device_id.to_string();
        let current_value =
            tokio::task::spawn_blocking(move || -> Result<Value, RedbObserverError> {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(DATA_TABLE)?;
                if let Some(stored_value) = table.get(did.as_str())? {
                    Ok(cbor_deserialize(stored_value.value()).unwrap_or(Value::Null))
                } else {
                    Ok(Value::Null)
                }
            })
            .await??;

        // Notify observers of changes
        self.channels
            .notify(device_id, &current_value, &new_value)
            .await;

        // Phase 2: Write new value (blocking DB write)
        let db = self.db.clone();
        let did = device_id.to_string();
        let cbor_bytes = cbor_serialize(&new_value)?;
        tokio::task::spawn_blocking(move || -> Result<(), RedbObserverError> {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(DATA_TABLE)?;
                table.insert(did.as_str(), cbor_bytes.as_slice())?;
            }
            write_txn.commit()?;
            Ok(())
        })
        .await??;

        Ok(())
    }

    async fn read(&mut self, device_id: &str, path: &str) -> Result<Option<Value>, Self::Error> {
        let db = self.db.clone();
        let did = device_id.to_string();
        let p = path.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, RedbObserverError> {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(DATA_TABLE)?;

            match table.get(did.as_str())? {
                Some(value) => {
                    let value: Value = cbor_deserialize(value.value())?;
                    tracing::debug!("Got value for path");
                    let pointer_value = super::cbor_pointer(&value, &p).cloned();
                    tracing::debug!("Pointer value: {:?}", pointer_value);
                    Ok(pointer_value)
                }
                None => Ok(None),
            }
        })
        .await?
    }

    async fn clear(&mut self, device_id: &str) -> Result<(), Self::Error> {
        let db = self.db.clone();
        let did = device_id.to_string();
        tokio::task::spawn_blocking(move || -> Result<(), RedbObserverError> {
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(DATA_TABLE)?;
                table.remove(did.as_str())?;
            }
            write_txn.commit()?;
            Ok(())
        })
        .await??;

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

    #[tokio::test]
    async fn test_redb_observer_write_and_read() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let tempdir = tempfile::tempdir().unwrap();
        let db_path = tempdir.path().join("test_write_read.redb");
        let mut observer = RedbObserver::new(db_path.to_str().unwrap()).unwrap();

        observer.clear("123").await.unwrap();

        let test_val = cbor_map(&[("test_key", Value::Text("test_value".into()))]);

        observer
            .write("123", "/test_path", &test_val)
            .await
            .unwrap();

        let result = observer.read("123", "/test_path").await.unwrap();
        assert_eq!(result, Some(test_val.clone()));

        observer
            .write("123", "/test_path/second_level", &test_val)
            .await
            .unwrap();

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
    async fn test_redb_observer_observe_and_write() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let tempdir = tempfile::tempdir().unwrap();
        let db_path = tempdir.path().join("test_observe_write.redb");
        let mut observer = RedbObserver::new(db_path.to_str().unwrap()).unwrap();

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
        assert!(observer.channel.is_none());

        observer
            .register("123", "/observe_and_write", Arc::new(tx.clone()))
            .await
            .unwrap();

        observer.unregister_all().await.unwrap();
        assert!(observer.channels.is_empty().await);
        assert!(observer.channel.is_none());
    }
}
