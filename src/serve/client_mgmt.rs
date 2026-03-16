use tokio::sync::mpsc;

use crate::{
    credential::CredentialStore,
    router::{ClientCommand, ClientManager},
};

/// Process a client command by delegating to a credential store.
pub(super) async fn process_client_command<C: CredentialStore>(
    cmd: ClientCommand,
    store: &C,
    disconnect_tx: &mpsc::Sender<String>,
) {
    match cmd {
        ClientCommand::AddClient {
            identity,
            key,
            metadata,
        } => {
            if let Err(e) = store.add_client(&identity, key, metadata).await {
                tracing::error!("Failed to add client {}: {:?}", identity, e);
            }
        }
        ClientCommand::RemoveClient { identity } => {
            if let Err(e) = store.remove_client(&identity).await {
                tracing::error!("Failed to remove client {}: {:?}", identity, e);
            }
        }
        ClientCommand::UpdateKey { identity, key } => {
            if let Err(e) = store.update_key(&identity, key).await {
                tracing::error!("Failed to update key for {}: {:?}", identity, e);
            }
        }
        ClientCommand::UpdateMetadata { identity, metadata } => {
            if let Err(e) = store.update_metadata(&identity, metadata).await {
                tracing::error!("Failed to update metadata for {}: {:?}", identity, e);
            }
        }
        ClientCommand::SetClientEnabled { identity, enabled } => {
            if let Err(e) = store.set_enabled(&identity, enabled).await {
                tracing::error!("Failed to set enabled for {}: {:?}", identity, e);
            }
        }
        ClientCommand::ListClients { response } => match store.list_clients().await {
            Ok(clients) => {
                let _ = response.send(clients);
            }
            Err(e) => {
                tracing::error!("Failed to list clients: {:?}", e);
                let _ = response.send(vec![]);
            }
        },
        ClientCommand::DisconnectClient { identity } => {
            if let Err(e) = disconnect_tx.send(identity.clone()).await {
                tracing::error!("Failed to send disconnect for {}: {}", identity, e);
            }
        }
    }
}

/// Create a client manager connected to a credential store.
///
/// This is useful when you want to manage clients from multiple places
/// or integrate with existing authentication systems.
pub fn create_client_manager<C: CredentialStore>(
    credential_store: C,
    buffer_size: usize,
) -> ClientManager {
    let (cmd_sender, mut cmd_receiver) = mpsc::channel(buffer_size);

    // Create a no-op disconnect channel (standalone managers aren't wired to a server)
    let (disconnect_tx, _disconnect_rx) = mpsc::channel::<String>(1);

    // Spawn command processor
    tokio::spawn(async move {
        while let Some(cmd) = cmd_receiver.recv().await {
            process_client_command(cmd, &credential_store, &disconnect_tx).await;
        }
    });

    ClientManager::new(cmd_sender)
}
