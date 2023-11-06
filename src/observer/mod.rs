use std::{collections::HashMap, fmt::Debug, sync::Arc};

use async_trait::async_trait;
use serde_json::{map::Entry, Value};
use tokio::sync::mpsc::Sender;

pub mod memory;
#[cfg(feature = "sled-observer")]
pub mod sled;
pub mod subscriber;

/// A struct representing an observer value.
#[derive(Debug, Clone)]
pub struct ObserverValue {
    pub value: Value,
    pub path: String,
}

/// A struct representing an observer request.
#[derive(Debug, Clone)]
pub struct ObserverRequest<E> {
    pub value: Value,
    pub path: String,
    pub source: E,
}

impl ObserverValue {
    /// Converts an observer value to an observer request.
    pub fn to_request<E>(self, source: E) -> ObserverRequest<E> {
        ObserverRequest {
            value: self.value,
            path: self.path,
            source,
        }
    }
}

/// A trait representing an observer.
#[async_trait]
pub trait Observer: Clone + Debug {
    type Error: Debug;

    /// Registers a path with the observer.
    async fn register(
        &mut self,
        device_id: &str,
        path: &str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> Result<(), Self::Error>;
    /// Unregisters a path from the observer.
    async fn unregister(&mut self, device_id: &str, path: &str) -> Result<(), Self::Error>;
    /// Unregisters all paths from the observer.
    async fn unregister_all(&mut self) -> Result<(), Self::Error>;
    /// Writes a value to a path.
    async fn write(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error>;
    /// Reads a value from a path.
    async fn read(&mut self, device_id: &str, path: &str) -> Result<Option<Value>, Self::Error>;
    /// Clears all values from the observer.
    async fn clear(&mut self, device_id: &str) -> Result<(), Self::Error>;
}

#[async_trait]
impl Observer for () {
    type Error = ();

    async fn register(
        &mut self,
        _device_id: &str,
        _path: &str,
        _sender: Arc<Sender<ObserverValue>>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn unregister(&mut self, _device_id: &str, _path: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn unregister_all(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn write(
        &mut self,
        _device_id: &str,
        _path: &str,
        _payload: &Value,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn read(&mut self, _device_id: &str, _path: &str) -> Result<Option<Value>, Self::Error> {
        Ok(None)
    }
    async fn clear(&mut self, _device_id: &str) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Converts a path and value to a JSON object.
///
/// # Arguments
///
/// * `path` - A string slice representing the path to be converted.
/// * `value` - A reference to a `serde_json::Value` object representing the value to be converted.
///
/// # Returns
///
/// A `serde_json::Value` object representing the JSON object created from the path and value.
pub fn path_to_json(path: &str, value: &Value) -> Value {
    let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let mut current_value = value.clone();

    for component in components.into_iter().rev() {
        let mut obj = HashMap::new();
        obj.insert(component.to_string(), current_value);
        current_value = serde_json::json!(obj);
    }

    current_value
}

/// Merges two JSON objects.
///
/// # Arguments
///
/// * `a` - A mutable reference to a `serde_json::Value` object representing the first JSON object to be merged.
/// * `b` - A reference to a `serde_json::Value` object representing the second JSON object to be merged.
pub fn merge_json(a: &mut Value, b: &Value) {
    match (a, b) {
        (&mut Value::Object(ref mut a), Value::Object(ref b)) => {
            for (k, v) in b {
                match a.entry(k.clone()) {
                    Entry::Vacant(e) => {
                        e.insert(v.clone());
                    }
                    Entry::Occupied(mut e) => merge_json(e.get_mut(), v),
                }
            }
        }
        (a, b) => *a = b.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_json() {
        let value = serde_json::json!({"test_key": "test_value"});
        let result = path_to_json("test/path", &value);
        let expected = serde_json::json!({"test": {"path": {"test_key": "test_value"}}});
        assert_eq!(result, expected);
    }

    #[test]
    fn test_merge_json() {
        let mut a = serde_json::json!({"test_key": "test_value"});
        let b = serde_json::json!({"test_key_2": "test_value_2"});
        merge_json(&mut a, &b);
        let expected = serde_json::json!({"test_key": "test_value", "test_key_2": "test_value_2"});
        assert_eq!(a, expected);
    }
}
