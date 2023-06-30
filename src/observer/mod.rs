use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde_json::{map::Entry, Value};
use tokio::sync::mpsc::Sender;

pub mod memory;
#[cfg(feature = "sled-observer")]
pub mod sled;

#[derive(Debug, Clone)]
pub struct ObserverValue {
    pub value: Value,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct ObserverRequest<E> {
    pub value: Value,
    pub path: String,
    pub source: E,
}

impl ObserverValue {
    pub fn to_request<E>(self, source: E) -> ObserverRequest<E> {
        ObserverRequest {
            value: self.value,
            path: self.path,
            source,
        }
    }
}

#[async_trait]
pub trait Observer: Clone {
    async fn set_id(&mut self, id: String);
    async fn register(&mut self, path: String, sender: Arc<Sender<ObserverValue>>);
    async fn unregister(&mut self, path: String);
    async fn unregister_all(&mut self);
    async fn write(&mut self, path: String, payload: Value);
    async fn read(&mut self, path: String) -> Option<Value>;
    async fn clear(&mut self);
}

#[async_trait]
impl Observer for () {
    async fn set_id(&mut self, _id: String) {}
    async fn register(&mut self, _path: String, _sender: Arc<Sender<ObserverValue>>) {}
    async fn unregister(&mut self, _path: String) {}
    async fn unregister_all(&mut self) {}
    async fn write(&mut self, _path: String, _payload: Value) {}
    async fn read(&mut self, _path: String) -> Option<Value> {
        None
    }
    async fn clear(&mut self) {}
}

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
