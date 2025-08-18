//! Enhanced server with dynamic client management support
//!
//! This module provides a server implementation that supports real-time
//! client authentication management, allowing adding/removing/updating
//! clients without server restart.

use crate::{
    config::Config,
    observer::Observer,
    router::{CoapRouter, ClientStore, ClientEntry, ClientCommand, ClientManager, ClientMetadata},
    serve::serve as base_serve,
};
use std::{collections::HashMap, sync::Arc};
use std::fmt::Debug;
use tokio::sync::{mpsc, RwLock};
use webrtc_dtls::Error;

/// Enhanced server configuration with client management
pub struct EnhancedConfig {
    /// Base configuration
    pub base_config: Config,
    /// Initial client store (identity -> PSK)
    pub initial_clients: HashMap<String, Vec<u8>>,
    /// Buffer size for client management commands
    pub client_command_buffer: usize,
}

impl Default for EnhancedConfig {
    fn default() -> Self {
        Self {
            base_config: Config::default(),
            initial_clients: HashMap::new(),
            client_command_buffer: 1000,
        }
    }
}

/// Start a CoAP server with dynamic client management capability
/// 
/// This function enhances the standard serve() function by adding support for
/// real-time client authentication management through the ClientManager handle.
/// 
/// # Returns
/// 
/// Returns a tuple of:
/// - A ClientManager handle for managing clients
/// - A JoinHandle for the server task
/// 
/// # Example
/// 
/// ```rust,no_run
/// # use coapum::{RouterBuilder, observer::memory::MemObserver};
/// # use coapum::serve_with_client_manager::{serve_with_client_manager, EnhancedConfig};
/// # use std::collections::HashMap;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # #[derive(Clone, Debug)]
/// # struct AppState {}
/// # let state = AppState {};
/// # let observer = MemObserver::new();
/// # let router = RouterBuilder::new(state, observer).build();
/// 
/// // Configure initial clients
/// let mut initial_clients = HashMap::new();
/// initial_clients.insert("device_001".to_string(), b"secret_key_123".to_vec());
/// 
/// let config = EnhancedConfig {
///     initial_clients,
///     ..Default::default()
/// };
/// 
/// // Start server with client management
/// let (client_manager, server_handle) = serve_with_client_manager(
///     "0.0.0.0:5683".to_string(),
///     config,
///     router
/// ).await?;
/// 
/// // Add a new client dynamically
/// client_manager.add_client("device_002", b"new_secret").await?;
/// 
/// // Update an existing client's key
/// client_manager.update_key("device_001", b"rotated_key").await?;
/// 
/// // Remove a client
/// client_manager.remove_client("device_001").await?;
/// 
/// // List all clients
/// let clients = client_manager.list_clients().await?;
/// println!("Active clients: {:?}", clients);
/// # Ok(())
/// # }
/// ```
pub async fn serve_with_client_manager<O, S>(
    addr: String,
    config: EnhancedConfig,
    router: CoapRouter<O, S>,
) -> Result<(ClientManager, tokio::task::JoinHandle<Result<(), String>>), Box<dyn std::error::Error>>
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + 'static,
{
    // Initialize client store with initial clients
    let mut store_map = HashMap::new();
    for (identity, key) in config.initial_clients {
        store_map.insert(identity, ClientEntry {
            key,
            metadata: ClientMetadata {
                enabled: true,
                ..Default::default()
            },
        });
    }
    let client_store: ClientStore = Arc::new(RwLock::new(store_map));
    
    // Create client management channel
    let (cmd_sender, mut cmd_receiver) = mpsc::channel(config.client_command_buffer);
    let client_manager = ClientManager::new(cmd_sender);
    
    // Clone for the command processor
    let store_for_processor = Arc::clone(&client_store);
    
    // Spawn client command processor
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            match cmd {
                ClientCommand::AddClient { identity, key, metadata } => {
                    let mut store = store_for_processor.write().await;
                    let entry = ClientEntry {
                        key,
                        metadata: metadata.unwrap_or_else(|| ClientMetadata {
                            enabled: true,
                            ..Default::default()
                        }),
                    };
                    store.insert(identity.clone(), entry);
                    log::info!("Added client: {}", identity);
                }
                ClientCommand::RemoveClient { identity } => {
                    let mut store = store_for_processor.write().await;
                    if store.remove(&identity).is_some() {
                        log::info!("Removed client: {}", identity);
                    } else {
                        log::warn!("Client not found for removal: {}", identity);
                    }
                }
                ClientCommand::UpdateKey { identity, key } => {
                    let mut store = store_for_processor.write().await;
                    if let Some(entry) = store.get_mut(&identity) {
                        entry.key = key;
                        log::info!("Updated key for client: {}", identity);
                    } else {
                        log::warn!("Client not found for key update: {}", identity);
                    }
                }
                ClientCommand::UpdateMetadata { identity, metadata } => {
                    let mut store = store_for_processor.write().await;
                    if let Some(entry) = store.get_mut(&identity) {
                        entry.metadata = metadata;
                        log::info!("Updated metadata for client: {}", identity);
                    } else {
                        log::warn!("Client not found for metadata update: {}", identity);
                    }
                }
                ClientCommand::SetClientEnabled { identity, enabled } => {
                    let mut store = store_for_processor.write().await;
                    if let Some(entry) = store.get_mut(&identity) {
                        entry.metadata.enabled = enabled;
                        log::info!("Set client {} enabled: {}", identity, enabled);
                    } else {
                        log::warn!("Client not found for enable/disable: {}", identity);
                    }
                }
                ClientCommand::ListClients { response } => {
                    let store = store_for_processor.read().await;
                    let clients: Vec<String> = store.keys().cloned().collect();
                    let _ = response.send(clients);
                }
            }
        }
    });
    
    // Create DTLS config with dynamic PSK callback
    let store_for_psk = Arc::clone(&client_store);
    let mut dtls_cfg = config.base_config.dtls_cfg.clone();
    
    // Set up PSK callback that uses our dynamic client store
    dtls_cfg.psk = Some(Arc::new(move |hint: &[u8]| -> Result<Vec<u8>, Error> {
        let hint_str = String::from_utf8(hint.to_vec())
            .map_err(|_| Error::ErrIdentityNoPsk)?;
        
        log::debug!("PSK callback for identity: {}", hint_str);
        
        // Use blocking read since we're in a sync context
        let store = store_for_psk.blocking_read();
        
        if let Some(entry) = store.get(&hint_str) {
            if entry.metadata.enabled {
                log::info!("PSK found for identity: {}", hint_str);
                Ok(entry.key.clone())
            } else {
                log::warn!("Client {} is disabled", hint_str);
                Err(Error::ErrIdentityNoPsk)
            }
        } else {
            log::warn!("PSK not found for identity: {}", hint_str);
            Err(Error::ErrIdentityNoPsk)
        }
    }));
    
    // Update the base config with our enhanced DTLS config
    let mut final_config = config.base_config;
    final_config.dtls_cfg = dtls_cfg;
    
    // Spawn the server task
    let server_handle = tokio::spawn(async move {
        base_serve(addr, final_config, router).await
            .map_err(|e| format!("Server error: {}", e))
    });
    
    Ok((client_manager, server_handle))
}

/// Create a client manager connected to an existing client store
/// 
/// This is useful when you want to manage clients from multiple places
/// or integrate with existing authentication systems.
pub fn create_client_manager(
    client_store: ClientStore,
    buffer_size: usize,
) -> ClientManager {
    let (cmd_sender, mut cmd_receiver) = mpsc::channel(buffer_size);
    
    // Spawn command processor
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            match cmd {
                ClientCommand::AddClient { identity, key, metadata } => {
                    let mut store = client_store.write().await;
                    let entry = ClientEntry {
                        key,
                        metadata: metadata.unwrap_or_default(),
                    };
                    store.insert(identity, entry);
                }
                ClientCommand::RemoveClient { identity } => {
                    let mut store = client_store.write().await;
                    store.remove(&identity);
                }
                ClientCommand::UpdateKey { identity, key } => {
                    let mut store = client_store.write().await;
                    if let Some(entry) = store.get_mut(&identity) {
                        entry.key = key;
                    }
                }
                ClientCommand::UpdateMetadata { identity, metadata } => {
                    let mut store = client_store.write().await;
                    if let Some(entry) = store.get_mut(&identity) {
                        entry.metadata = metadata;
                    }
                }
                ClientCommand::SetClientEnabled { identity, enabled } => {
                    let mut store = client_store.write().await;
                    if let Some(entry) = store.get_mut(&identity) {
                        entry.metadata.enabled = enabled;
                    }
                }
                ClientCommand::ListClients { response } => {
                    let store = client_store.read().await;
                    let clients: Vec<String> = store.keys().cloned().collect();
                    let _ = response.send(clients);
                }
            }
        }
    });
    
    ClientManager::new(cmd_sender)
}