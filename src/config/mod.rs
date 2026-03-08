use std::time::Duration;

use tokio::sync::watch;

#[derive(Clone)]
pub struct Config {
    /// DTLS configuration
    pub dtls_cfg: webrtc_dtls::config::Config,

    /// Timeout in seconds
    pub timeout: u64,

    /// Buffer size for incoming messages (default: 8192 bytes)
    /// Security: Limited to prevent memory exhaustion attacks
    pub buffer_size: usize,

    /// Optional initial client store (identity -> PSK) for dynamic client management
    pub initial_clients: Option<std::collections::HashMap<String, Vec<u8>>>,

    /// Buffer size for client management commands (only used if initial_clients is Some)
    pub client_command_buffer: usize,

    /// Maximum total CoAP message size for block-wise transfer (RFC 7959).
    /// Messages larger than this are automatically fragmented.
    /// Default: 1152 bytes (RFC 7252).
    pub max_message_size: usize,

    /// Cache expiry duration for block-wise transfer state.
    /// Default: 120 seconds.
    pub block_cache_expiry: Duration,

    /// Maximum number of observer registrations per device.
    /// Prevents memory exhaustion from a single device registering unlimited observers.
    /// Default: 100.
    pub max_observers_per_device: usize,

    /// Maximum number of concurrent connections.
    /// Prevents DoS attacks using many unique device identities.
    /// Default: 1000.
    pub max_connections: usize,

    /// Timeout in milliseconds for sending observer notifications.
    /// Prevents slow clients from blocking notifications to other observers.
    /// Default: 1000ms.
    pub notification_timeout_ms: u64,

    /// Minimum interval between reconnection attempts from the same identity.
    /// Rapid reconnections within this window are rate-limited.
    /// Default: 5 seconds.
    pub min_reconnect_interval: Duration,

    /// Maximum reconnection attempts before blocking an identity.
    /// Default: 10.
    pub max_reconnect_attempts: usize,

    /// Optional shutdown signal. When the sender is dropped or a value is sent,
    /// the server stops accepting new connections and exits gracefully.
    /// Default: `None` (server runs until the process is killed).
    pub shutdown: Option<watch::Receiver<()>>,
}

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    InvalidBufferSize { size: usize, min: usize, max: usize },
    InvalidTimeout(u64),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidBufferSize { size, min, max } => {
                write!(
                    f,
                    "Invalid buffer size: {} (must be between {} and {})",
                    size, min, max
                )
            }
            ConfigError::InvalidTimeout(timeout) => {
                write!(f, "Invalid timeout: {} (must be > 0)", timeout)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Minimum allowed buffer size (512 bytes)
    pub const MIN_BUFFER_SIZE: usize = 512;
    /// Maximum allowed buffer size (64KB) to prevent memory exhaustion
    pub const MAX_BUFFER_SIZE: usize = 65536;
    /// Default buffer size (8KB)
    pub const DEFAULT_BUFFER_SIZE: usize = 8192;

    /// Get the current buffer size
    pub fn buffer_size(&self) -> usize {
        self.buffer_size
    }

    /// Set buffer size with validation
    pub fn set_buffer_size(&mut self, size: usize) -> Result<(), ConfigError> {
        if !(Self::MIN_BUFFER_SIZE..=Self::MAX_BUFFER_SIZE).contains(&size) {
            return Err(ConfigError::InvalidBufferSize {
                size,
                min: Self::MIN_BUFFER_SIZE,
                max: Self::MAX_BUFFER_SIZE,
            });
        }
        self.buffer_size = size;
        Ok(())
    }

    /// Set timeout with validation
    pub fn set_timeout(&mut self, timeout: u64) -> Result<(), ConfigError> {
        if timeout == 0 {
            return Err(ConfigError::InvalidTimeout(timeout));
        }
        self.timeout = timeout;
        Ok(())
    }

    /// Enable client management with initial clients
    pub fn with_client_management(
        mut self,
        initial_clients: std::collections::HashMap<String, Vec<u8>>,
    ) -> Self {
        self.initial_clients = Some(initial_clients);
        self
    }

    /// Set client command buffer size
    pub fn set_client_command_buffer(&mut self, size: usize) {
        self.client_command_buffer = size;
    }

    /// Check if client management is enabled
    pub fn has_client_management(&self) -> bool {
        self.initial_clients.is_some()
    }

    /// Set the maximum total CoAP message size for block-wise transfer.
    pub fn set_max_message_size(&mut self, size: usize) {
        self.max_message_size = size;
    }

    /// Set the cache expiry duration for block-wise transfer state.
    pub fn set_block_cache_expiry(&mut self, duration: Duration) {
        self.block_cache_expiry = duration;
    }

    /// Set the maximum number of observer registrations per device.
    pub fn set_max_observers_per_device(&mut self, max: usize) {
        self.max_observers_per_device = max;
    }

    /// Set the maximum number of concurrent connections.
    pub fn set_max_connections(&mut self, max: usize) {
        self.max_connections = max;
    }

    /// Set the notification send timeout in milliseconds.
    pub fn set_notification_timeout_ms(&mut self, timeout_ms: u64) {
        self.notification_timeout_ms = timeout_ms;
    }

    /// Set the minimum interval between reconnection attempts.
    pub fn set_min_reconnect_interval(&mut self, interval: Duration) {
        self.min_reconnect_interval = interval;
    }

    /// Set the maximum number of reconnection attempts before blocking.
    pub fn set_max_reconnect_attempts(&mut self, max: usize) {
        self.max_reconnect_attempts = max;
    }

    /// Set a shutdown signal receiver for graceful shutdown.
    ///
    /// When the corresponding [`watch::Sender`] sends a value or is dropped,
    /// the server will stop accepting new connections and exit.
    pub fn set_shutdown(&mut self, rx: watch::Receiver<()>) {
        self.shutdown = Some(rx);
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dtls_cfg: Default::default(),
            timeout: 60,
            buffer_size: Self::DEFAULT_BUFFER_SIZE,
            initial_clients: None,
            client_command_buffer: 1000,
            max_message_size: 1152,
            block_cache_expiry: Duration::from_secs(120),
            max_observers_per_device: 100,
            max_connections: 1000,
            notification_timeout_ms: 1000,
            min_reconnect_interval: Duration::from_secs(5),
            max_reconnect_attempts: 10,
            shutdown: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.timeout, 60);
        assert_eq!(config.buffer_size(), Config::DEFAULT_BUFFER_SIZE);
    }

    #[test]
    fn test_buffer_size_validation() {
        let mut config = Config::default();

        // Valid size
        assert!(config.set_buffer_size(1024).is_ok());
        assert_eq!(config.buffer_size(), 1024);

        // Too small
        assert_eq!(
            config.set_buffer_size(256),
            Err(ConfigError::InvalidBufferSize {
                size: 256,
                min: 512,
                max: 65536
            })
        );

        // Too large
        assert_eq!(
            config.set_buffer_size(100000),
            Err(ConfigError::InvalidBufferSize {
                size: 100000,
                min: 512,
                max: 65536
            })
        );
    }

    #[test]
    fn test_timeout_validation() {
        let mut config = Config::default();

        // Valid timeout
        assert!(config.set_timeout(30).is_ok());
        assert_eq!(config.timeout, 30);

        // Invalid timeout
        assert_eq!(config.set_timeout(0), Err(ConfigError::InvalidTimeout(0)));
    }
}
