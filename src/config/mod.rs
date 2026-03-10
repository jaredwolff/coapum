use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

#[derive(Clone)]
pub struct Config {
    /// DTLS configuration. Must be set before serving.
    ///
    /// Build with `dimpl::Config::builder()` and wrap in `Arc`.
    /// When using `serve_with_credential_store()`, this is built automatically
    /// from the credential store.
    pub dimpl_cfg: Option<Arc<dimpl::Config>>,

    /// PSK identity hint sent by the server during handshake.
    /// Used when building dimpl config from a credential store.
    pub psk_identity_hint: Option<Vec<u8>>,

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

    /// Maximum duration a DTLS session may remain active before the server
    /// forces a reconnect. Mitigates DTLS 1.2 key wear-out on long-lived
    /// or high-frequency connections — DTLS 1.2 has no key update mechanism,
    /// so the only way to rotate key material is to tear down and re-establish
    /// the session. The client is expected to reconnect automatically.
    /// Default: `None` (no limit).
    pub max_session_lifetime: Option<Duration>,

    /// RFC 7252 §4.8 ACK_TIMEOUT: base retransmission timeout for Confirmable messages.
    /// The actual initial timeout is randomized between `ack_timeout` and
    /// `ack_timeout * ack_random_factor`.
    /// Default: 2 seconds.
    pub ack_timeout: Duration,

    /// RFC 7252 §4.8 ACK_RANDOM_FACTOR: randomization factor for initial retransmission
    /// timeout. Must be >= 1.0.
    /// Default: 1.5.
    pub ack_random_factor: f64,

    /// RFC 7252 §4.8 MAX_RETRANSMIT: maximum number of retransmissions for a
    /// Confirmable message before giving up.
    /// Default: 4.
    pub max_retransmit: u32,

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

    /// Set the maximum DTLS session lifetime.
    ///
    /// After this duration, the server will disconnect the client, forcing
    /// a fresh DTLS handshake with new key material. This mitigates key
    /// wear-out in DTLS 1.2 which has no rekeying mechanism.
    pub fn set_max_session_lifetime(&mut self, lifetime: Duration) {
        self.max_session_lifetime = Some(lifetime);
    }

    /// Set a shutdown signal receiver for graceful shutdown.
    ///
    /// When the corresponding [`watch::Sender`] sends a value or is dropped,
    /// the server will stop accepting new connections and exit.
    pub fn set_shutdown(&mut self, rx: watch::Receiver<()>) {
        self.shutdown = Some(rx);
    }

    /// Set the ACK timeout for Confirmable message retransmission.
    pub fn set_ack_timeout(&mut self, timeout: Duration) {
        self.ack_timeout = timeout;
    }

    /// Set the ACK random factor. Must be >= 1.0.
    pub fn set_ack_random_factor(&mut self, factor: f64) {
        self.ack_random_factor = factor.max(1.0);
    }

    /// Set the maximum number of retransmissions for Confirmable messages.
    pub fn set_max_retransmit(&mut self, max: u32) {
        self.max_retransmit = max;
    }

    /// RFC 7252 §4.8.2 EXCHANGE_LIFETIME: time from first transmission of a
    /// CON message to when the message ID can be safely reused.
    pub fn exchange_lifetime(&self) -> Duration {
        // EXCHANGE_LIFETIME = MAX_TRANSMIT_SPAN + MAX_TRANSMIT_WAIT
        // MAX_TRANSMIT_SPAN = ACK_TIMEOUT * ((2 ** MAX_RETRANSMIT) - 1) * ACK_RANDOM_FACTOR
        // MAX_TRANSMIT_WAIT = ACK_TIMEOUT * ((2 ** (MAX_RETRANSMIT + 1)) - 1) * ACK_RANDOM_FACTOR
        // Simplified: EXCHANGE_LIFETIME = ACK_TIMEOUT * ((2 ** (MAX_RETRANSMIT + 1)) - 1) * ACK_RANDOM_FACTOR + PROCESSING_DELAY
        // With defaults: 2 * 63 * 1.5 + 2 = 191s. RFC says ~247s total.
        let max_transmit_span = self
            .ack_timeout
            .mul_f64(((1u64 << self.max_retransmit) - 1) as f64 * self.ack_random_factor);
        // PROCESSING_DELAY per RFC 7252 = ACK_TIMEOUT
        max_transmit_span + self.ack_timeout
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dimpl_cfg: None,
            psk_identity_hint: None,
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
            max_session_lifetime: None,
            ack_timeout: Duration::from_secs(2),
            ack_random_factor: 1.5,
            max_retransmit: 4,
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
        assert!(config.dimpl_cfg.is_none());
        assert!(config.max_session_lifetime.is_none());
    }

    #[test]
    fn test_max_session_lifetime_setter() {
        let mut config = Config::default();
        config.set_max_session_lifetime(Duration::from_secs(3600));
        assert_eq!(config.max_session_lifetime, Some(Duration::from_secs(3600)));
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
