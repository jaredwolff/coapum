//! Example demonstrating dynamic client management for CoAP server
//!
//! This example shows how to add, remove, and update client authentication
//! credentials in real-time without restarting the server.
//!
//! Run with: cargo run --example dynamic_client_management

use coapum::{
    RouterBuilder,
    observer::memory::MemObserver,
    serve::serve_with_client_management,
    config::Config,
    ClientManager,
};
use std::collections::HashMap;
use tokio::time::{interval, Duration};

#[derive(Clone, Debug)]
struct AppState {
    message: String,
}

// Simulate a client management system
async fn simulate_client_lifecycle(client_manager: ClientManager) {
    println!("\n=== Starting Client Management Simulation ===\n");
    
    // Initial clients
    println!("Initial clients:");
    let clients = client_manager.list_clients().await.unwrap();
    for client in &clients {
        println!("  - {}", client);
    }
    
    // Wait a bit
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Add new clients
    println!("\nAdding new clients...");
    for i in 1..=3 {
        let device_id = format!("new_device_{:03}", i);
        let key = format!("dynamic_key_{}", i);
        
        client_manager.add_client(&device_id, key.as_bytes()).await.unwrap();
        println!("  Added: {} with key: {}", device_id, key);
    }
    
    // List all clients
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("\nAll clients after additions:");
    let clients = client_manager.list_clients().await.unwrap();
    for client in &clients {
        println!("  - {}", client);
    }
    
    // Update some keys
    tokio::time::sleep(Duration::from_secs(5)).await;
    println!("\nRotating keys for existing clients...");
    
    for i in 1..=2 {
        let device_id = format!("initial_device_{:03}", i);
        let new_key = format!("rotated_key_{}_{}", i, std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs());
        
        if client_manager.update_key(&device_id, new_key.as_bytes()).await.is_ok() {
            println!("  Rotated key for: {}", device_id);
        }
    }
    
    // Add clients with metadata
    tokio::time::sleep(Duration::from_secs(5)).await;
    println!("\nAdding clients with metadata...");
    
    let sensor_metadata = coapum::ClientMetadata {
        name: Some("Temperature Sensor - Living Room".to_string()),
        description: Some("DHT22 sensor monitoring temperature and humidity".to_string()),
        enabled: true,
        tags: vec!["sensor".to_string(), "temperature".to_string(), "indoor".to_string()],
        custom: {
            let mut map = HashMap::new();
            map.insert("location".to_string(), "living_room".to_string());
            map.insert("model".to_string(), "DHT22".to_string());
            map
        },
    };
    
    client_manager.add_client_with_metadata(
        "sensor_living_room",
        b"sensor_secret_key_123",
        sensor_metadata
    ).await.unwrap();
    println!("  Added: sensor_living_room (Temperature Sensor)");
    
    // Disable a client
    tokio::time::sleep(Duration::from_secs(5)).await;
    println!("\nDisabling a client...");
    client_manager.set_client_enabled("initial_device_001", false).await.unwrap();
    println!("  Disabled: initial_device_001");
    
    // Remove some clients
    tokio::time::sleep(Duration::from_secs(5)).await;
    println!("\nRemoving inactive clients...");
    
    for i in 1..=2 {
        let device_id = format!("new_device_{:03}", i);
        client_manager.remove_client(&device_id).await.unwrap();
        println!("  Removed: {}", device_id);
    }
    
    // Final client list
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("\nFinal client list:");
    let clients = client_manager.list_clients().await.unwrap();
    for client in &clients {
        println!("  - {}", client);
    }
    
    println!("\n=== Client Management Simulation Complete ===\n");
}

// Simulate periodic key rotation
async fn periodic_key_rotation(client_manager: ClientManager) {
    let mut interval = interval(Duration::from_secs(30));
    let mut rotation_count = 0;
    
    loop {
        interval.tick().await;
        rotation_count += 1;
        
        println!("\n[Key Rotation #{}] Starting periodic key rotation...", rotation_count);
        
        let clients = client_manager.list_clients().await.unwrap();
        for client in clients.iter().filter(|c| c.starts_with("rotate_")) {
            let new_key = format!("rotated_key_{}_v{}", client, rotation_count);
            if client_manager.update_key(client, new_key.as_bytes()).await.is_ok() {
                println!("  Rotated key for: {}", client);
            }
        }
        
        println!("[Key Rotation #{}] Complete", rotation_count);
    }
}

// Monitor client activity (simulated)
async fn monitor_clients(client_manager: ClientManager) {
    let mut interval = interval(Duration::from_secs(20));
    
    loop {
        interval.tick().await;
        
        let clients = client_manager.list_clients().await.unwrap();
        println!("\n[Monitor] Active clients: {}", clients.len());
        
        if clients.len() > 10 {
            println!("[Monitor] Warning: High number of clients detected!");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    // Create application state
    let state = AppState {
        message: "CoAP server with dynamic client management".to_string(),
    };
    
    // Create observer
    let observer = MemObserver::new();
    
    // Build router
    let router = RouterBuilder::new(state, observer).build();
    
    // Configure initial clients
    let mut initial_clients = HashMap::new();
    initial_clients.insert("initial_device_001".to_string(), b"initial_key_001".to_vec());
    initial_clients.insert("initial_device_002".to_string(), b"initial_key_002".to_vec());
    initial_clients.insert("rotate_device_001".to_string(), b"rotate_key_001".to_vec());
    initial_clients.insert("rotate_device_002".to_string(), b"rotate_key_002".to_vec());
    
    // Configure server with client management
    let mut config = Config::default().with_client_management(initial_clients);
    config.set_client_command_buffer(1000);
    
    println!("Starting CoAP server with dynamic client management on 0.0.0.0:5683");
    println!("Features demonstrated:");
    println!("- Adding new clients dynamically");
    println!("- Removing clients");
    println!("- Updating client keys");
    println!("- Managing client metadata");
    println!("- Enabling/disabling clients");
    println!("- Periodic key rotation");
    println!();
    
    // Setup client management and get server future
    let (client_manager, server_future) = serve_with_client_management(
        "0.0.0.0:5683".to_string(),
        config,
        router
    ).await?;
    
    // Spawn the server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server_future.await {
            log::error!("Server error: {}", e);
        }
    });
    
    // Spawn client management tasks
    let lifecycle_manager = client_manager.clone();
    tokio::spawn(async move {
        simulate_client_lifecycle(lifecycle_manager).await;
    });
    
    let rotation_manager = client_manager.clone();
    tokio::spawn(async move {
        periodic_key_rotation(rotation_manager).await;
    });
    
    let monitor_manager = client_manager.clone();
    tokio::spawn(async move {
        monitor_clients(monitor_manager).await;
    });
    
    // Wait for server or handle shutdown
    server_handle.await.unwrap_or_else(|e| {
        log::error!("Server task failed: {}", e);
    });
    
    Ok(())
}

// Demonstrates dynamic client management without external dependencies