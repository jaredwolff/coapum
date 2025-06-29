#[derive(Clone)]
pub struct Config {
    /// DTLS configuration
    pub dtls_cfg: webrtc_dtls::config::Config,

    /// Timeout in seconds
    pub timeout: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            dtls_cfg: Default::default(),
            timeout: 60,
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
    }
}
