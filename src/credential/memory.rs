//! In-memory credential store implementation.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::router::{ClientEntry, ClientMetadata};

use super::{CredentialStore, PskEntry};

/// In-memory credential store backed by a `HashMap`.
///
/// Suitable for development, testing, and single-instance deployments.
/// For persistent or shared credential storage, implement [`CredentialStore`]
/// with your preferred backend.
#[derive(Clone, Debug)]
pub struct MemoryCredentialStore {
    store: Arc<RwLock<HashMap<String, ClientEntry>>>,
}

impl MemoryCredentialStore {
    /// Create an empty credential store.
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a credential store pre-populated with clients.
    ///
    /// Each entry maps an identity string to a PSK key. All clients
    /// are enabled by default with default metadata.
    pub fn from_clients(clients: &HashMap<String, Vec<u8>>) -> Self {
        let mut store = HashMap::new();
        for (identity, key) in clients {
            store.insert(
                identity.clone(),
                ClientEntry {
                    key: key.clone(),
                    metadata: ClientMetadata {
                        enabled: true,
                        ..Default::default()
                    },
                },
            );
        }
        Self {
            store: Arc::new(RwLock::new(store)),
        }
    }
}

impl Default for MemoryCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for MemoryCredentialStore {
    type Error = std::convert::Infallible;

    fn lookup_psk(&self, identity: &str) -> Result<Option<PskEntry>, Self::Error> {
        let store = self.store.blocking_read();
        Ok(store.get(identity).map(|entry| PskEntry {
            key: entry.key.clone(),
            enabled: entry.metadata.enabled,
        }))
    }

    async fn add_client(
        &self,
        identity: &str,
        key: Vec<u8>,
        metadata: Option<ClientMetadata>,
    ) -> Result<(), Self::Error> {
        let mut store = self.store.write().await;
        let entry = ClientEntry {
            key,
            metadata: metadata.unwrap_or(ClientMetadata {
                enabled: true,
                ..Default::default()
            }),
        };
        store.insert(identity.to_string(), entry);
        tracing::info!("Added client: {}", identity);
        Ok(())
    }

    async fn remove_client(&self, identity: &str) -> Result<bool, Self::Error> {
        let mut store = self.store.write().await;
        let existed = store.remove(identity).is_some();
        if existed {
            tracing::info!("Removed client: {}", identity);
        } else {
            tracing::warn!("Client not found for removal: {}", identity);
        }
        Ok(existed)
    }

    async fn update_key(&self, identity: &str, key: Vec<u8>) -> Result<bool, Self::Error> {
        let mut store = self.store.write().await;
        if let Some(entry) = store.get_mut(identity) {
            entry.key = key;
            tracing::info!("Updated key for client: {}", identity);
            Ok(true)
        } else {
            tracing::warn!("Client not found for key update: {}", identity);
            Ok(false)
        }
    }

    async fn update_metadata(
        &self,
        identity: &str,
        metadata: ClientMetadata,
    ) -> Result<bool, Self::Error> {
        let mut store = self.store.write().await;
        if let Some(entry) = store.get_mut(identity) {
            entry.metadata = metadata;
            tracing::info!("Updated metadata for client: {}", identity);
            Ok(true)
        } else {
            tracing::warn!("Client not found for metadata update: {}", identity);
            Ok(false)
        }
    }

    async fn set_enabled(&self, identity: &str, enabled: bool) -> Result<bool, Self::Error> {
        let mut store = self.store.write().await;
        if let Some(entry) = store.get_mut(identity) {
            entry.metadata.enabled = enabled;
            tracing::info!("Set client {} enabled: {}", identity, enabled);
            Ok(true)
        } else {
            tracing::warn!("Client not found for enable/disable: {}", identity);
            Ok(false)
        }
    }

    async fn list_clients(&self) -> Result<Vec<String>, Self::Error> {
        let store = self.store.read().await;
        Ok(store.keys().cloned().collect())
    }
}
