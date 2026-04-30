use crate::config::ConfigError;

#[derive(Debug)]
pub enum Error {
    Bind(std::io::Error),
    Join(tokio::task::JoinError),
    Config(ConfigError),
    MissingDtlsConfig,
    ClientManagementDisabled,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Bind(e) => write!(f, "failed to bind UDP socket: {}", e),
            Error::Join(e) => write!(f, "server task panicked or was cancelled: {}", e),
            Error::Config(e) => write!(f, "invalid configuration: {}", e),
            Error::MissingDtlsConfig => f.write_str(
                "DTLS configuration missing — set config.dimpl_cfg or use serve_with_credential_store()",
            ),
            Error::ClientManagementDisabled => f.write_str(
                "client management not enabled — use Config::with_client_management() to enable",
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Bind(e) => Some(e),
            Error::Join(e) => Some(e),
            Error::Config(e) => Some(e),
            Error::MissingDtlsConfig | Error::ClientManagementDisabled => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Bind(e)
    }
}

impl From<tokio::task::JoinError> for Error {
    fn from(e: tokio::task::JoinError) -> Self {
        Error::Join(e)
    }
}

impl From<ConfigError> for Error {
    fn from(e: ConfigError) -> Self {
        Error::Config(e)
    }
}
