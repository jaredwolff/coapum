use std::{collections::HashMap, fmt::Debug, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::{Value, map::Entry};
use tokio::sync::{RwLock, mpsc::Sender};

pub mod memory;
#[cfg(feature = "redb-observer")]
pub mod redb;
#[cfg(feature = "sled-observer")]
pub mod sled;
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

/// A trait for pluggable device state storage backends.
///
/// Implement this trait to provide a custom storage backend (e.g., PostgreSQL,
/// Redis) for device state and observer notifications. See [`memory::MemObserver`]
/// for a reference implementation.
#[async_trait]
pub trait Observer: Clone + Debug + Send + Sync + 'static {
    type Error: Debug + Send + Sync;

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
    /// Unregisters all paths for a specific device.
    async fn unregister_device(&mut self, device_id: &str) -> Result<(), Self::Error>;
    /// Unregisters only paths for a device that are owned by the given sender.
    /// Defaults to `unregister_device` for backends that don't track ownership.
    async fn unregister_device_if_owned(
        &mut self,
        device_id: &str,
        owner: &Arc<Sender<ObserverValue>>,
    ) -> Result<(), Self::Error> {
        let _ = owner;
        self.unregister_device(device_id).await
    }
    /// Writes a value to a path, merging with any existing value.
    async fn write(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error>;
    /// Writes a value to a path, fully replacing any existing value (no merge).
    async fn write_replace(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error>;
    /// Reads a value from a path.
    async fn read(&mut self, device_id: &str, path: &str) -> Result<Option<Value>, Self::Error>;
    /// Clears all values from the observer.
    async fn clear(&mut self, device_id: &str) -> Result<(), Self::Error>;

    /// Returns the number of observer registrations for a device.
    /// Used by the server to enforce per-device observer limits.
    /// Default returns 0 (no limit enforcement).
    async fn observer_count(&self, _device_id: &str) -> usize {
        0
    }

    /// Send a notification to observers without persisting or diffing.
    /// Use for ephemeral events (commands) where every call must notify.
    async fn notify(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error> {
        self.write(device_id, path, payload).await
    }
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
    async fn unregister_device(&mut self, _device_id: &str) -> Result<(), Self::Error> {
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
    async fn write_replace(
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
    async fn notify(
        &mut self,
        _device_id: &str,
        _path: &str,
        _payload: &Value,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Errors from observer path validation.
#[derive(Debug, PartialEq)]
pub enum PathValidationError {
    /// Path contains traversal patterns (`..`, `./`, `\`)
    TraversalAttempt,
    /// Path exceeds maximum depth (10 components)
    PathTooDeep,
    /// Path contains non-ASCII or disallowed characters
    InvalidCharacters,
    /// Path is empty
    EmptyPath,
}

impl std::fmt::Display for PathValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathValidationError::TraversalAttempt => write!(f, "Path traversal attempt detected"),
            PathValidationError::PathTooDeep => write!(f, "Path too deep (max 10 components)"),
            PathValidationError::InvalidCharacters => {
                write!(f, "Path contains invalid characters")
            }
            PathValidationError::EmptyPath => write!(f, "Path is empty"),
        }
    }
}

impl std::error::Error for PathValidationError {}

/// Maximum allowed depth for observer paths.
const MAX_PATH_DEPTH: usize = 10;

/// Validate and normalize an observer path to prevent injection attacks.
///
/// This function is called by the server before registering observers.
/// Observer backend implementations do **not** need to perform their own
/// path validation — paths passed to [`Observer::register`] and
/// [`Observer::write`] have already been validated.
///
/// # Rules
///
/// - Rejects empty paths
/// - Rejects traversal patterns (`..`, `./`, `\`)
/// - Limits path depth to 10 components
/// - Only allows ASCII alphanumeric characters, `_`, and `-` in path components
/// - Returns a normalized path with a leading `/`
///
/// # Example
///
/// ```
/// use coapum::observer::validate_observer_path;
///
/// assert_eq!(validate_observer_path("sensors/temp").unwrap(), "/sensors/temp");
/// assert!(validate_observer_path("../etc/passwd").is_err());
/// assert!(validate_observer_path("").is_err());
/// ```
pub fn validate_observer_path(path: &str) -> Result<String, PathValidationError> {
    if path.is_empty() {
        return Err(PathValidationError::EmptyPath);
    }

    // Reject paths containing dangerous patterns
    if path.contains("..") || path.contains("./") || path.contains('\\') {
        return Err(PathValidationError::TraversalAttempt);
    }

    // Normalize and validate path components
    let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if components.len() > MAX_PATH_DEPTH {
        return Err(PathValidationError::PathTooDeep);
    }

    // Validate each path component for safe characters only
    for component in &components {
        if !component
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(PathValidationError::InvalidCharacters);
        }
    }

    // Return normalized path
    if components.is_empty() {
        Ok("/".to_string())
    } else {
        Ok(format!("/{}", components.join("/")))
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
        (&mut Value::Object(ref mut a), Value::Object(b)) => {
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

// Type aliases for observer channel management.
/// Sender wrapped in Arc for shared ownership across tasks.
pub type ObserverSender = Arc<Sender<ObserverValue>>;
/// Maps observer path → sender channel.
pub type PathChannels = HashMap<String, ObserverSender>;
/// Maps device ID → path channels.
pub type DeviceChannels = HashMap<String, PathChannels>;

/// Default notification send timeout.
const DEFAULT_NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(1);

/// Shared observer channel management for register/unregister/notify operations.
///
/// This struct encapsulates the common logic shared across all observer backends:
/// channel registration, unregistration, and notification dispatch with value diffing.
///
/// Backend implementations should embed this struct and delegate channel operations
/// to it, only handling their own persistence logic.
///
/// # Example
///
/// ```rust,no_run
/// use coapum::observer::ObserverChannels;
///
/// #[derive(Clone, Debug)]
/// struct MyObserver {
///     channels: ObserverChannels,
///     // ... your storage fields
/// }
/// ```
#[derive(Clone, Debug)]
pub struct ObserverChannels {
    channels: Arc<RwLock<DeviceChannels>>,
    notification_timeout: Duration,
}

impl Default for ObserverChannels {
    fn default() -> Self {
        Self::new()
    }
}

impl ObserverChannels {
    /// Create a new channel manager with the default notification timeout (1 second).
    pub fn new() -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            notification_timeout: DEFAULT_NOTIFICATION_TIMEOUT,
        }
    }

    /// Create a new channel manager with a custom notification timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            notification_timeout: timeout,
        }
    }

    /// Register an observer channel for a device/path pair.
    pub async fn register(&self, device_id: &str, path: &str, sender: Arc<Sender<ObserverValue>>) {
        let mut channels = self.channels.write().await;
        channels
            .entry(device_id.to_string())
            .or_default()
            .insert(path.to_string(), sender);

        tracing::debug!(
            "Registered observer for device '{}' at path '{}'",
            device_id,
            path
        );
    }

    /// Unregister an observer for a specific device/path pair.
    /// Returns `true` if all observers for all devices are now empty.
    pub async fn unregister(&self, device_id: &str, path: &str) -> bool {
        let mut channels = self.channels.write().await;
        if let Some(device_channels) = channels.get_mut(device_id) {
            device_channels.remove(path);
            if device_channels.is_empty() {
                channels.remove(device_id);
            }
        }
        channels.is_empty()
    }

    /// Unregister all observers across all devices.
    pub async fn unregister_all(&self) {
        self.channels.write().await.clear();
    }

    /// Unregister all observers for a specific device.
    /// Returns `true` if all observers for all devices are now empty.
    pub async fn unregister_device(&self, device_id: &str) -> bool {
        let mut channels = self.channels.write().await;
        channels.remove(device_id);
        channels.is_empty()
    }

    /// Unregister only observers for a device that are owned by the given sender.
    /// Returns `true` if all observers for all devices are now empty.
    pub async fn unregister_device_if_owned(
        &self,
        device_id: &str,
        owner: &Arc<Sender<ObserverValue>>,
    ) -> bool {
        let mut channels = self.channels.write().await;
        if let Some(device_channels) = channels.get_mut(device_id) {
            device_channels.retain(|_path, sender| !Arc::ptr_eq(sender, owner));
            if device_channels.is_empty() {
                channels.remove(device_id);
            }
        }
        channels.is_empty()
    }

    /// Check if there are any registered observers.
    pub async fn is_empty(&self) -> bool {
        self.channels.read().await.is_empty()
    }

    /// Get the number of observer registrations for a specific device.
    pub async fn device_observer_count(&self, device_id: &str) -> usize {
        self.channels
            .read()
            .await
            .get(device_id)
            .map_or(0, |c| c.len())
    }

    /// Notify observers unconditionally for a device at a specific path.
    ///
    /// Sends the payload to all registered observers for the given path
    /// without comparing old and new values. Use for ephemeral events
    /// (commands) where every call must notify.
    pub async fn notify_unconditional(&self, device_id: &str, path: &str, payload: &Value) {
        let channels = self.channels.read().await;

        let device_channels = match channels.get(device_id) {
            Some(dc) => dc,
            None => {
                tracing::debug!("No observers found for device '{}'", device_id);
                return;
            }
        };

        if let Some(sender) = device_channels.get(path) {
            let notification = ObserverValue {
                path: path.to_string(),
                value: payload.clone(),
            };

            match tokio::time::timeout(self.notification_timeout, sender.send(notification)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::warn!(
                        "Failed to send observer notification for device {} path {}: {}",
                        device_id,
                        path,
                        e
                    );
                }
                Err(_) => {
                    tracing::warn!(
                        "Notification timeout for device {} path {} ({}ms)",
                        device_id,
                        path,
                        self.notification_timeout.as_millis()
                    );
                }
            }
        }
    }

    /// Notify observers of value changes for a device.
    ///
    /// Compares `current_value` (before write) with `new_value` (after write)
    /// at each registered observer path. Only sends notifications when values
    /// actually changed. Uses a configurable timeout to prevent slow clients
    /// from blocking other notifications.
    pub async fn notify(&self, device_id: &str, current_value: &Value, new_value: &Value) {
        let channels = self.channels.read().await;

        let device_channels = match channels.get(device_id) {
            Some(dc) => dc,
            None => {
                tracing::debug!("No observers found for device '{}'", device_id);
                return;
            }
        };

        tracing::debug!(
            "Found device '{}' with {} observers",
            device_id,
            device_channels.len()
        );

        for (obs_path, sender) in device_channels.iter() {
            let json_pointer = normalize_json_pointer(obs_path);
            let current_at_path = current_value.pointer(&json_pointer);
            let incoming_at_path = new_value.pointer(&json_pointer);

            if current_at_path != incoming_at_path {
                tracing::debug!(
                    "Value changed at path: {} for device: {}",
                    obs_path,
                    device_id
                );

                let notification_value = match incoming_at_path {
                    Some(value) => value.clone(),
                    None => Value::Null,
                };

                let notification = ObserverValue {
                    path: obs_path.clone(),
                    value: notification_value,
                };

                match tokio::time::timeout(self.notification_timeout, sender.send(notification))
                    .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!(
                            "Failed to send observer notification for device {} path {}: {}",
                            device_id,
                            obs_path,
                            e
                        );
                    }
                    Err(_) => {
                        tracing::warn!(
                            "Notification timeout for device {} path {} ({}ms)",
                            device_id,
                            obs_path,
                            self.notification_timeout.as_millis()
                        );
                    }
                }
            }
        }
    }
}

/// Normalizes a path to JSON pointer format by ensuring it starts with '/'.
fn normalize_json_pointer(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_observer_path_valid() {
        assert_eq!(
            validate_observer_path("sensors/temp").unwrap(),
            "/sensors/temp"
        );
        assert_eq!(
            validate_observer_path("/sensors/temp").unwrap(),
            "/sensors/temp"
        );
        assert_eq!(validate_observer_path("a-b_c/d123").unwrap(), "/a-b_c/d123");
        assert_eq!(validate_observer_path("/").unwrap(), "/");
    }

    #[test]
    fn test_validate_observer_path_rejects_traversal() {
        assert_eq!(
            validate_observer_path("../etc").unwrap_err(),
            PathValidationError::TraversalAttempt
        );
        assert_eq!(
            validate_observer_path("./hidden").unwrap_err(),
            PathValidationError::TraversalAttempt
        );
        assert_eq!(
            validate_observer_path("a\\b").unwrap_err(),
            PathValidationError::TraversalAttempt
        );
    }

    #[test]
    fn test_validate_observer_path_rejects_deep() {
        let deep = (0..11)
            .map(|i| format!("p{}", i))
            .collect::<Vec<_>>()
            .join("/");
        assert_eq!(
            validate_observer_path(&deep).unwrap_err(),
            PathValidationError::PathTooDeep
        );
    }

    #[test]
    fn test_validate_observer_path_rejects_invalid_chars() {
        assert_eq!(
            validate_observer_path("a/b c").unwrap_err(),
            PathValidationError::InvalidCharacters
        );
        assert_eq!(
            validate_observer_path("a/@b").unwrap_err(),
            PathValidationError::InvalidCharacters
        );
    }

    #[test]
    fn test_validate_observer_path_rejects_empty() {
        assert_eq!(
            validate_observer_path("").unwrap_err(),
            PathValidationError::EmptyPath
        );
    }

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

    #[tokio::test]
    async fn test_unregister_device_if_owned_only_removes_owned() {
        let channels = ObserverChannels::new();

        let (tx_old, _rx_old) = tokio::sync::mpsc::channel::<ObserverValue>(1);
        let (tx_new, _rx_new) = tokio::sync::mpsc::channel::<ObserverValue>(1);
        let old_sender = Arc::new(tx_old);
        let new_sender = Arc::new(tx_new);

        // Old connection registers on /a
        channels.register("dev1", "/a", old_sender.clone()).await;
        // New connection registers on /b
        channels.register("dev1", "/b", new_sender.clone()).await;

        assert_eq!(channels.device_observer_count("dev1").await, 2);

        // Old connection cleans up — should only remove /a
        channels
            .unregister_device_if_owned("dev1", &old_sender)
            .await;

        assert_eq!(channels.device_observer_count("dev1").await, 1);

        // New connection's registration on /b survives
        let guard = channels.channels.read().await;
        let dev = guard.get("dev1").expect("device entry should exist");
        assert!(dev.contains_key("/b"));
        assert!(!dev.contains_key("/a"));
    }

    #[tokio::test]
    async fn test_unregister_device_if_owned_removes_device_when_all_owned() {
        let channels = ObserverChannels::new();

        let (tx, _rx) = tokio::sync::mpsc::channel::<ObserverValue>(1);
        let sender = Arc::new(tx);

        channels.register("dev1", "/a", sender.clone()).await;
        channels.register("dev1", "/b", sender.clone()).await;

        let all_empty = channels.unregister_device_if_owned("dev1", &sender).await;

        assert!(all_empty);
        assert_eq!(channels.device_observer_count("dev1").await, 0);
    }
}
