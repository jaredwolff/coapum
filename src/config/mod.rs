#[derive(Clone)]
pub struct Config {
    /// DTLS configuration
    pub dtls_cfg: webrtc_dtls::config::Config,

    /// Timeout in seconds
    pub timeout: u64,

    /// Buffer size for incoming messages (default: 8192 bytes)
    /// Security: Limited to prevent memory exhaustion attacks
    pub buffer_size: usize,
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
                write!(f, "Invalid buffer size: {} (must be between {} and {})", size, min, max)
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dtls_cfg: Default::default(),
            timeout: 60,
            buffer_size: Self::DEFAULT_BUFFER_SIZE,
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
            Err(ConfigError::InvalidBufferSize { size: 256, min: 512, max: 65536 })
        );
        
        // Too large
        assert_eq!(
            config.set_buffer_size(100000),
            Err(ConfigError::InvalidBufferSize { size: 100000, min: 512, max: 65536 })
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
