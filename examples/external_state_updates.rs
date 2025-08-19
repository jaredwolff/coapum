//! Example demonstrating external state updates for CoAP server
//!
//! This example shows how external components can update the server's
//! shared state using StateUpdateHandle, enabling real-time state
//! modifications without direct router access.
//!
//! Run with: cargo run --example external_state_updates

use coapum::{
    StateUpdateHandle, config::Config, observer::memory::MemObserver, router::RouterBuilder,
    serve::serve,
};
use std::collections::HashMap;
use tokio::time::{Duration, interval};

#[derive(Clone, Debug)]
struct AppState {
    // Simulated sensor readings
    temperature: f32,
    humidity: f32,
    // Device status
    devices: HashMap<String, DeviceStatus>,
    // System status
    uptime_seconds: u64,
    request_count: u64,
}

#[derive(Clone, Debug)]
struct DeviceStatus {
    online: bool,
    last_seen: u64,
    battery_level: u8,
}

impl AppState {
    fn new() -> Self {
        let mut devices = HashMap::new();
        devices.insert(
            "sensor_01".to_string(),
            DeviceStatus {
                online: true,
                last_seen: 0,
                battery_level: 100,
            },
        );
        devices.insert(
            "sensor_02".to_string(),
            DeviceStatus {
                online: true,
                last_seen: 0,
                battery_level: 85,
            },
        );

        AppState {
            temperature: 22.5,
            humidity: 45.0,
            devices,
            uptime_seconds: 0,
            request_count: 0,
        }
    }
}

// Note: In a real application, you would have proper handlers that access state
// For this demo, we focus on the external state update mechanism

// Simulate external sensor updates
async fn simulate_sensor_updates(state_handle: StateUpdateHandle<AppState>) {
    let mut interval = interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        // Simulate temperature and humidity changes
        let temp_change = (rand::random::<f32>() - 0.5) * 2.0;
        let humidity_change = (rand::random::<f32>() - 0.5) * 5.0;

        state_handle
            .update(move |state: &mut AppState| {
                state.temperature = (state.temperature + temp_change).clamp(15.0, 35.0);
                state.humidity = (state.humidity + humidity_change).clamp(20.0, 80.0);
                println!(
                    "Sensor update: temp={:.1}°C, humidity={:.1}%",
                    state.temperature, state.humidity
                );
            })
            .await
            .unwrap();
    }
}

// Simulate device status updates
async fn simulate_device_updates(state_handle: StateUpdateHandle<AppState>) {
    let mut interval = interval(Duration::from_secs(5));

    loop {
        interval.tick().await;

        let device_id = if rand::random::<bool>() {
            "sensor_01"
        } else {
            "sensor_02"
        };
        let battery_drain = rand::random::<u8>() % 3;

        state_handle
            .update(move |state: &mut AppState| {
                if let Some(device) = state.devices.get_mut(device_id) {
                    device.last_seen = state.uptime_seconds;
                    device.battery_level = device.battery_level.saturating_sub(battery_drain);

                    // Simulate device going offline at low battery
                    if device.battery_level < 10 {
                        device.online = false;
                        println!(
                            "Device {} went offline (low battery: {}%)",
                            device_id, device.battery_level
                        );
                    } else {
                        println!(
                            "Device {} status: battery={}%",
                            device_id, device.battery_level
                        );
                    }
                }
            })
            .await
            .unwrap();
    }
}

// Update system statistics
async fn update_system_stats(state_handle: StateUpdateHandle<AppState>) {
    let mut interval = interval(Duration::from_secs(1));

    loop {
        interval.tick().await;

        state_handle
            .update(|state: &mut AppState| {
                state.uptime_seconds += 1;

                // Print stats every 10 seconds
                if state.uptime_seconds % 10 == 0 {
                    println!(
                        "System uptime: {}s, requests handled: {}",
                        state.uptime_seconds, state.request_count
                    );
                }
            })
            .await
            .unwrap();
    }
}

// External monitoring system
async fn external_monitoring(state_handle: StateUpdateHandle<AppState>) {
    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await;

        println!("=== External Monitor Report ===");

        // Perform system health checks and updates
        state_handle
            .update(|state: &mut AppState| {
                // Check and potentially reset offline devices
                for (id, device) in state.devices.iter_mut() {
                    if !device.online && device.battery_level > 50 {
                        device.online = true;
                        println!("Monitor: Bringing device {} back online", id);
                    }
                }

                // Log current state summary
                let online_count = state.devices.values().filter(|d| d.online).count();
                println!(
                    "Monitor: {} of {} devices online",
                    online_count,
                    state.devices.len()
                );
                println!(
                    "Monitor: Environment - temp={:.1}°C, humidity={:.1}%",
                    state.temperature, state.humidity
                );
            })
            .await
            .unwrap();

        println!("==============================\n");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Create initial application state
    let state = AppState::new();
    let observer = MemObserver::new();

    // Create router with state update capability
    let mut builder = RouterBuilder::new(state, observer);

    // Enable external state updates with a buffer of 1000 messages
    let state_handle = builder.enable_state_updates(1000);

    // Build router
    // Note: In a real application, you would add handlers here
    // that can access the state via State extractor
    let router = builder.build();

    println!("Starting CoAP server with external state updates on 0.0.0.0:5683");
    println!("The server simulates:");
    println!("- Sensor readings (temperature/humidity) updating every 2s");
    println!("- Device status updates every 5s");
    println!("- System statistics every 1s");
    println!("- External monitoring every 30s");
    println!();
    println!("Example requests:");
    println!("  GET /status - Get system status");
    println!();

    // Spawn external update tasks
    let sensor_handle = state_handle.clone();
    tokio::spawn(async move {
        simulate_sensor_updates(sensor_handle).await;
    });

    let device_handle = state_handle.clone();
    tokio::spawn(async move {
        simulate_device_updates(device_handle).await;
    });

    let stats_handle = state_handle.clone();
    tokio::spawn(async move {
        update_system_stats(stats_handle).await;
    });

    let monitor_handle = state_handle.clone();
    tokio::spawn(async move {
        external_monitoring(monitor_handle).await;
    });

    // Start the CoAP server
    let config = Config::default();
    serve("0.0.0.0:5683".to_string(), config, router).await
}

// Helper function for random f32 generation (simplified)
mod rand {
    pub fn random<T>() -> T
    where
        T: RandomValue,
    {
        T::random()
    }

    pub trait RandomValue {
        fn random() -> Self;
    }

    impl RandomValue for f32 {
        fn random() -> Self {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            ((now % 1000) as f32) / 1000.0
        }
    }

    impl RandomValue for bool {
        fn random() -> Self {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            (now % 2) == 0
        }
    }

    impl RandomValue for u8 {
        fn random() -> Self {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            (now % 256) as u8
        }
    }
}
