use crate::config::ConfigError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to bind UDP socket: {0}")]
    Bind(#[from] std::io::Error),

    #[error("server task panicked or was cancelled: {0}")]
    Join(#[from] tokio::task::JoinError),

    #[error("invalid configuration: {0}")]
    Config(#[from] ConfigError),

    #[error(
        "DTLS configuration missing — set config.dimpl_cfg or use serve_with_credential_store()"
    )]
    MissingDtlsConfig,

    #[error("client management not enabled — use Config::with_client_management() to enable")]
    ClientManagementDisabled,
}
