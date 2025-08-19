//! Tests for real-time client/key management functionality

use coapum::{
    ClientManagerError,
    router::{ClientCommand, ClientEntry, ClientManager, ClientMetadata, ClientStore},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

#[tokio::test]
async fn test_client_manager_add_remove() {
    let client_store: ClientStore = Arc::new(RwLock::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel(10);
    let client_manager = ClientManager::new(tx);

    // Spawn processor
    let store_clone = Arc::clone(&client_store);
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ClientCommand::AddClient {
                    identity,
                    key,
                    metadata,
                } => {
                    let mut store = store_clone.write().await;
                    store.insert(
                        identity,
                        ClientEntry {
                            key,
                            metadata: metadata.unwrap_or_default(),
                        },
                    );
                }
                ClientCommand::RemoveClient { identity } => {
                    let mut store = store_clone.write().await;
                    store.remove(&identity);
                }
                _ => {}
            }
        }
    });

    // Test adding clients
    client_manager.add_client("device1", b"key1").await.unwrap();
    client_manager.add_client("device2", b"key2").await.unwrap();

    // Give time for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Verify clients were added
    {
        let store = client_store.read().await;
        assert_eq!(store.len(), 2);
        assert!(store.contains_key("device1"));
        assert!(store.contains_key("device2"));
        assert_eq!(store.get("device1").unwrap().key, b"key1");
    }

    // Test removing a client
    client_manager.remove_client("device1").await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Verify client was removed
    {
        let store = client_store.read().await;
        assert_eq!(store.len(), 1);
        assert!(!store.contains_key("device1"));
        assert!(store.contains_key("device2"));
    }
}

#[tokio::test]
async fn test_client_manager_update_key() {
    let client_store: ClientStore = Arc::new(RwLock::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel(10);
    let client_manager = ClientManager::new(tx);

    // Initialize with a client
    {
        let mut store = client_store.write().await;
        store.insert(
            "device1".to_string(),
            ClientEntry {
                key: b"original_key".to_vec(),
                metadata: ClientMetadata::default(),
            },
        );
    }

    // Spawn processor
    let store_clone = Arc::clone(&client_store);
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            if let ClientCommand::UpdateKey { identity, key } = cmd {
                let mut store = store_clone.write().await;
                if let Some(entry) = store.get_mut(&identity) {
                    entry.key = key;
                }
            }
        }
    });

    // Update the key
    client_manager
        .update_key("device1", b"new_key")
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Verify key was updated
    {
        let store = client_store.read().await;
        assert_eq!(store.get("device1").unwrap().key, b"new_key");
    }
}

#[tokio::test]
async fn test_client_manager_metadata() {
    let client_store: ClientStore = Arc::new(RwLock::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel(10);
    let client_manager = ClientManager::new(tx);

    // Spawn processor
    let store_clone = Arc::clone(&client_store);
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ClientCommand::AddClient {
                    identity,
                    key,
                    metadata,
                } => {
                    let mut store = store_clone.write().await;
                    store.insert(
                        identity,
                        ClientEntry {
                            key,
                            metadata: metadata.unwrap_or_default(),
                        },
                    );
                }
                ClientCommand::UpdateMetadata { identity, metadata } => {
                    let mut store = store_clone.write().await;
                    if let Some(entry) = store.get_mut(&identity) {
                        entry.metadata = metadata;
                    }
                }
                ClientCommand::SetClientEnabled { identity, enabled } => {
                    let mut store = store_clone.write().await;
                    if let Some(entry) = store.get_mut(&identity) {
                        entry.metadata.enabled = enabled;
                    }
                }
                _ => {}
            }
        }
    });

    // Add client with metadata
    let metadata = ClientMetadata {
        name: Some("Temperature Sensor".to_string()),
        description: Some("Outdoor sensor".to_string()),
        enabled: true,
        tags: vec!["sensor".to_string(), "outdoor".to_string()],
        custom: HashMap::new(),
    };
    client_manager
        .add_client_with_metadata("sensor1", b"key1", metadata.clone())
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Verify metadata was stored
    {
        let store = client_store.read().await;
        let entry = store.get("sensor1").unwrap();
        assert_eq!(entry.metadata.name, Some("Temperature Sensor".to_string()));
        assert_eq!(entry.metadata.tags.len(), 2);
        assert!(entry.metadata.enabled);
    }

    // Update metadata
    let new_metadata = ClientMetadata {
        name: Some("Updated Sensor".to_string()),
        enabled: false,
        ..Default::default()
    };
    client_manager
        .update_metadata("sensor1", new_metadata)
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Verify metadata was updated
    {
        let store = client_store.read().await;
        let entry = store.get("sensor1").unwrap();
        assert_eq!(entry.metadata.name, Some("Updated Sensor".to_string()));
        assert!(!entry.metadata.enabled);
    }

    // Test enable/disable
    client_manager
        .set_client_enabled("sensor1", true)
        .await
        .unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    {
        let store = client_store.read().await;
        assert!(store.get("sensor1").unwrap().metadata.enabled);
    }
}

#[tokio::test]
async fn test_client_manager_list_clients() {
    let client_store: ClientStore = Arc::new(RwLock::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel(10);
    let client_manager = ClientManager::new(tx);

    // Initialize with some clients
    {
        let mut store = client_store.write().await;
        store.insert(
            "device1".to_string(),
            ClientEntry {
                key: b"key1".to_vec(),
                metadata: ClientMetadata::default(),
            },
        );
        store.insert(
            "device2".to_string(),
            ClientEntry {
                key: b"key2".to_vec(),
                metadata: ClientMetadata::default(),
            },
        );
        store.insert(
            "device3".to_string(),
            ClientEntry {
                key: b"key3".to_vec(),
                metadata: ClientMetadata::default(),
            },
        );
    }

    // Spawn processor
    let store_clone = Arc::clone(&client_store);
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            if let ClientCommand::ListClients { response } = cmd {
                let store = store_clone.read().await;
                let clients: Vec<String> = store.keys().cloned().collect();
                let _ = response.send(clients);
            }
        }
    });

    // List clients
    let clients = client_manager.list_clients().await.unwrap();
    assert_eq!(clients.len(), 3);
    assert!(clients.contains(&"device1".to_string()));
    assert!(clients.contains(&"device2".to_string()));
    assert!(clients.contains(&"device3".to_string()));
}

#[tokio::test]
async fn test_client_manager_concurrent_operations() {
    let client_store: ClientStore = Arc::new(RwLock::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel(100);
    let client_manager = ClientManager::new(tx);

    // Spawn processor
    let store_clone = Arc::clone(&client_store);
    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ClientCommand::AddClient {
                    identity,
                    key,
                    metadata,
                } => {
                    let mut store = store_clone.write().await;
                    store.insert(
                        identity,
                        ClientEntry {
                            key,
                            metadata: metadata.unwrap_or_default(),
                        },
                    );
                }
                ClientCommand::RemoveClient { identity } => {
                    let mut store = store_clone.write().await;
                    store.remove(&identity);
                }
                ClientCommand::UpdateKey { identity, key } => {
                    let mut store = store_clone.write().await;
                    if let Some(entry) = store.get_mut(&identity) {
                        entry.key = key;
                    }
                }
                _ => {}
            }
        }
    });

    // Spawn multiple tasks doing concurrent operations
    let mut handles = Vec::new();

    // Add clients concurrently
    for i in 0..10 {
        let manager = client_manager.clone();
        let handle = tokio::spawn(async move {
            manager
                .add_client(&format!("device{}", i), format!("key{}", i).as_bytes())
                .await
                .unwrap();
        });
        handles.push(handle);
    }

    // Update keys concurrently
    for i in 0..5 {
        let manager = client_manager.clone();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
            manager
                .update_key(&format!("device{}", i), format!("newkey{}", i).as_bytes())
                .await
                .unwrap();
        });
        handles.push(handle);
    }

    // Wait for all operations
    for handle in handles {
        handle.await.unwrap();
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Verify results
    let store = client_store.read().await;
    assert_eq!(store.len(), 10);

    // Check updated keys
    for i in 0..5 {
        assert_eq!(
            store.get(&format!("device{}", i)).unwrap().key,
            format!("newkey{}", i).as_bytes()
        );
    }

    // Check non-updated keys
    for i in 5..10 {
        assert_eq!(
            store.get(&format!("device{}", i)).unwrap().key,
            format!("key{}", i).as_bytes()
        );
    }
}

#[tokio::test]
async fn test_client_manager_error_handling() {
    // Test with a closed channel
    let (tx, rx) = mpsc::channel::<ClientCommand>(1);
    drop(rx); // Close the receiver

    let client_manager = ClientManager::new(tx);

    // All operations should return ChannelClosed error
    assert_eq!(
        client_manager.add_client("test", b"key").await.unwrap_err(),
        ClientManagerError::ChannelClosed
    );

    assert_eq!(
        client_manager.remove_client("test").await.unwrap_err(),
        ClientManagerError::ChannelClosed
    );

    assert_eq!(
        client_manager.update_key("test", b"key").await.unwrap_err(),
        ClientManagerError::ChannelClosed
    );
}
