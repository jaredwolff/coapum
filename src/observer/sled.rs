use std::{fmt, sync::Arc};

use async_trait::async_trait;
use ciborium::value::Value;
use tokio::sync::mpsc::{Sender, channel};

use super::{Observer, ObserverChannels, ObserverValue};

#[derive(Clone, Debug)]
pub struct SledObserver {
    pub db: sled::Db,
    channel: Option<Sender<()>>,
    /// Shared channel management for observer notifications.
    pub channels: ObserverChannels,
}

impl SledObserver {
    pub fn new(path: &str) -> Self {
        Self {
            db: sled::open(path).unwrap(),
            channel: None,
            channels: ObserverChannels::new(),
        }
    }
}

#[derive(Debug)]
pub enum SledObserverError {
    SledError(sled::Error),
    CborError(String),
    IdNotSet,
    TaskJoinError(String),
}

impl fmt::Display for SledObserverError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SledObserverError::SledError(err) => write!(f, "Sled error: {}", err),
            SledObserverError::CborError(err) => write!(f, "CBOR error: {}", err),
            SledObserverError::IdNotSet => write!(f, "Device ID must be set before use!"),
            SledObserverError::TaskJoinError(msg) => write!(f, "Task join error: {}", msg),
        }
    }
}

impl std::error::Error for SledObserverError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SledObserverError::SledError(err) => Some(err),
            SledObserverError::CborError(_) => None,
            SledObserverError::IdNotSet => None,
            SledObserverError::TaskJoinError(_) => None,
        }
    }
}

impl From<tokio::task::JoinError> for SledObserverError {
    fn from(err: tokio::task::JoinError) -> SledObserverError {
        SledObserverError::TaskJoinError(err.to_string())
    }
}

impl From<sled::Error> for SledObserverError {
    fn from(err: sled::Error) -> SledObserverError {
        SledObserverError::SledError(err)
    }
}

fn cbor_serialize(value: &Value) -> Result<Vec<u8>, SledObserverError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf)
        .map_err(|e| SledObserverError::CborError(e.to_string()))?;
    Ok(buf)
}

fn cbor_deserialize(bytes: &[u8]) -> Result<Value, SledObserverError> {
    ciborium::de::from_reader(bytes).map_err(|e| SledObserverError::CborError(e.to_string()))
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
        self.channels.register(device_id, path, sender).await;

        // Spawn watcher task if not already running.
        // All change notifications are handled in write().
        // This task only exists for cleanup when unregistered.
        if self.channel.is_none() {
            let (tx, mut rx) = channel::<()>(1);
            let id = device_id.to_string();
            self.channel = Some(tx);

            tokio::spawn(async move {
                tokio::select! {
                    _ = async {
                        tracing::debug!("Starting sled watcher for device: {}", id);
                        futures::future::pending::<()>().await;
                    } => {}
                    _ = rx.recv() => {
                        tracing::debug!("Terminating sled subscriber for device: {}", id);
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
        let (value, current_value) = tokio::task::spawn_blocking(move || {
            let mut current_value = Value::Null;
            let value = if let Ok(Some(stored_value)) = db.get(did.as_bytes()) {
                match cbor_deserialize(&stored_value) {
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
            };
            (value, current_value)
        })
        .await?;

        // Notify observers of changes
        self.channels
            .notify(device_id, &current_value, &value)
            .await;

        // Phase 3: Write merged value back (blocking DB write)
        let db = self.db.clone();
        let did = device_id.to_string();
        let val = value.clone();
        tokio::task::spawn_blocking(move || -> Result<(), SledObserverError> {
            let v = cbor_serialize(&val)?;
            db.insert(did.as_bytes(), v)?;
            tracing::debug!("Value successfully written to sled");
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
        let current_value = tokio::task::spawn_blocking(move || {
            if let Ok(Some(stored_value)) = db.get(did.as_bytes()) {
                cbor_deserialize(&stored_value).unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        })
        .await?;

        // Notify observers of changes
        self.channels
            .notify(device_id, &current_value, &new_value)
            .await;

        // Phase 2: Write new value (blocking DB write)
        let db = self.db.clone();
        let did = device_id.to_string();
        let val = new_value;
        tokio::task::spawn_blocking(move || -> Result<(), SledObserverError> {
            let v = cbor_serialize(&val)?;
            db.insert(did.as_bytes(), v)?;
            Ok(())
        })
        .await??;

        Ok(())
    }

    async fn read(&mut self, device_id: &str, path: &str) -> Result<Option<Value>, Self::Error> {
        let db = self.db.clone();
        let did = device_id.to_string();
        let p = path.to_string();
        tokio::task::spawn_blocking(move || -> Result<Option<Value>, SledObserverError> {
            match db.get(did.as_bytes()) {
                Ok(Some(value)) => {
                    let value: Value = cbor_deserialize(&value)?;
                    tracing::debug!("Got value: {:?}", value);
                    let pointer_value = super::cbor_pointer(&value, &p).cloned();
                    tracing::debug!("Pointer value: {:?}", pointer_value);
                    Ok(pointer_value)
                }
                Ok(None) => Ok(None),
                Err(e) => {
                    tracing::error!("Error reading from sled: {}", e);
                    Err(e.into())
                }
            }
        })
        .await?
    }

    async fn clear(&mut self, device_id: &str) -> Result<(), Self::Error> {
        let db = self.db.clone();
        let did = device_id.to_string();
        tokio::task::spawn_blocking(move || {
            let _ = db.remove(did.as_bytes());
        })
        .await
        .map_err(SledObserverError::from)?;

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
    async fn test_sled_observer_write_and_read() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let tempdir = tempfile::tempdir().unwrap();
        let db_path = tempdir.path().join("sled_db");
        let mut observer = SledObserver::new(db_path.to_str().unwrap());

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
    async fn test_sled_observer_observe_and_write() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();

        let tempdir = tempfile::tempdir().unwrap();
        let db_path = tempdir.path().join("sled_db");
        let mut observer = SledObserver::new(db_path.to_str().unwrap());

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
