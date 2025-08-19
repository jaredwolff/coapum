//! # CBOR Server Example
//!
//! This example demonstrates a comprehensive CoAP server implementation using CBOR
//! (Concise Binary Object Representation) for payload serialization. It showcases:
//!
//! - DTLS security with PSK (Pre-Shared Key) authentication
//! - RESTful API design with path parameters
//! - Observer pattern for real-time notifications
//! - Structured error handling with custom response types
//! - Device state management for IoT use cases
//!
//! ## Features
//!
//! - **Device Management**: CRUD operations for device states
//! - **Stream Handling**: Data ingestion endpoints
//! - **Real-time Updates**: CoAP observe pattern for notifications
//! - **Security**: DTLS encryption with PSK authentication
//! - **Persistence**: Sled database for observer storage
//!
//! ## API Endpoints
//!
//! - `POST .d/{device_id}` - Update device state
//! - `GET .d/{device_id}` - Get device state
//! - `DELETE .d/{device_id}` - Delete device state
//! - `OBSERVE .d/{device_id}` - Subscribe to device state changes
//! - `POST .s/{stream_id}` - Handle stream data
//! - `PUT echo` - Echo payload back
//! - `GET hello` - Echo payload back
//! - `ANY /` - Ping endpoint
//!
//! ## Usage
//!
//! ```bash
//! # Start the server
//! cargo run --example cbor_server
//!
//! # In another terminal, test with the client
//! cargo run --example cbor_client
//! ```
//!
//! ## Security
//!
//! The server uses DTLS with PSK authentication. The default PSK is configured
//! for the identity "goobie!" with the key "63ef2024b1de6417f856fab7005d38f6".
//! In production, use strong, randomly generated keys.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use coapum::{
    dtls::{
        cipher_suite::CipherSuiteId,
        config::{Config, ExtendedMasterSecretType},
        Error,
    },
    extract::{Cbor, Identity, Path, State, StatusCode},
    observer::memory::MemObserver,
    router::RouterBuilder,
    serve,
};
use serde::{Deserialize, Serialize};

type PskStore = Arc<RwLock<HashMap<String, Vec<u8>>>>;

const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6".as_bytes();

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DeviceState {
    temperature: f32,
    humidity: f32,
    battery_level: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ApiResponse {
    status: String,
    message: String,
}

#[derive(Clone, Debug)]
struct AppState {
    device_states: Arc<tokio::sync::Mutex<HashMap<String, DeviceState>>>,
}

impl AsRef<AppState> for AppState {
    fn as_ref(&self) -> &AppState {
        self
    }
}

// Handler for updating device state - POST .d/{device_id}
async fn update_device_state(
    Path(device_id): Path<String>,
    Cbor(new_state): Cbor<DeviceState>,
    Identity(client_id): Identity,
    State(app_state): State<AppState>,
) -> Result<Cbor<ApiResponse>, StatusCode> {
    log::info!(
        "Updating device {} state from client {}: temp={}°C, humidity={}%, battery={}%",
        device_id,
        client_id,
        new_state.temperature,
        new_state.humidity,
        new_state.battery_level
    );

    // Store the device state in our application state
    let mut states = app_state.device_states.lock().await;
    states.insert(device_id.clone(), new_state.clone());
    let response = ApiResponse {
        status: "success".to_string(),
        message: format!("Device {} state updated", device_id),
    };

    Ok(Cbor(response))
}

// Handler for getting device state - GET .d/{device_id}
async fn get_device_state(
    Path(device_id): Path<String>,
    Identity(client_id): Identity,
    State(app_state): State<AppState>,
) -> Result<Cbor<DeviceState>, StatusCode> {
    log::info!(
        "Getting device {} state for client {}",
        device_id,
        client_id
    );

    // Fetch the device state from our application state
    let states = app_state.device_states.lock().await;
    let state = states.get(&device_id).cloned().unwrap_or(DeviceState {
        temperature: 23.5,
        humidity: 45.2,
        battery_level: 85,
    });

    Ok(Cbor(state))
}

// Handler for device state notifications (observer pattern)
async fn notify_device_state(
    Path(device_id): Path<String>,
    Identity(client_id): Identity,
    State(app_state): State<AppState>,
) -> Cbor<DeviceState> {
    log::info!(
        "Sending notification for device {} to client {}",
        device_id,
        client_id
    );

    // Get the current device state for notifications
    let states = app_state.device_states.lock().await;
    let state = states.get(&device_id).cloned().unwrap_or(DeviceState {
        temperature: 24.1,
        humidity: 43.8,
        battery_level: 84,
    });

    Cbor(state)
}

// Handler for deleting device state - DELETE .d/{device_id}
async fn delete_device_state(
    Path(device_id): Path<String>,
    Identity(client_id): Identity,
    State(app_state): State<AppState>,
) -> Result<StatusCode, StatusCode> {
    log::info!(
        "Deleting device {} state for client {}",
        device_id,
        client_id
    );

    // Remove the device state from our application state
    let mut states = app_state.device_states.lock().await;
    states.remove(&device_id);
    Ok(StatusCode::Deleted)
}

// Handler for stream data - POST .s/{stream_id}
async fn handle_stream_data(
    Path(stream_id): Path<String>,
    Identity(client_id): Identity,
) -> StatusCode {
    log::info!(
        "Received stream data for {} from client {}",
        stream_id,
        client_id
    );

    StatusCode::Valid
}

// Simple echo handler - PUT echo
async fn echo_handler(payload: coapum::extract::Bytes) -> coapum::extract::Bytes {
    log::info!("Echoing {} bytes", payload.len());
    payload
}

// Simple ping handler - any method on root
async fn ping_handler(Identity(client_id): Identity) -> StatusCode {
    log::info!("Ping from {}", client_id);
    StatusCode::Valid
}

#[tokio::main]
async fn main() {
    env_logger::init();

    log::info!("Starting ergonomic CoAP server!");

    // Set up PSK store
    let psk_store: PskStore = Arc::new(RwLock::new(HashMap::new()));
    {
        let mut psk_store_write = psk_store.write().unwrap();
        psk_store_write.insert("goobie!".to_string(), PSK.to_vec());
    }

    // Create application state
    let app_state = AppState {
        device_states: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    };

    // Create observer database
    let observer = MemObserver::new();

    // Build router with ergonomic API
    let router = RouterBuilder::new(app_state, observer)
        // Device state routes with path parameters
        .post(".d/:device_id", update_device_state)
        .get(".d/:device_id", get_device_state)
        .observe(".d/:device_id", get_device_state, notify_device_state)
        .delete(".d/:device_id", delete_device_state)
        // Stream routes
        .post(".s/:stream_id", handle_stream_data)
        // Utility routes
        .put("echo", echo_handler)
        .get("hello", echo_handler)
        .get("", ping_handler)
        .build();

    // Setup DTLS configuration
    let addr = "127.0.0.1:5684";
    let dtls_cfg = Config {
        psk: Some(Arc::new(move |hint: &[u8]| -> Result<Vec<u8>, Error> {
            let hint = String::from_utf8(hint.to_vec()).unwrap();
            log::info!("Client's hint: {}", hint);

            if let Some(psk) = psk_store.read().unwrap().get(&hint) {
                Ok(psk.clone())
            } else {
                log::info!("Hint {} not found in store", hint);
                Err(Error::ErrIdentityNoPsk)
            }
        })),
        psk_identity_hint: Some("coapum ergonomic server".as_bytes().to_vec()),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
        extended_master_secret: ExtendedMasterSecretType::Require,
        ..Default::default()
    };

    // Server configuration
    let cfg = coapum::config::Config {
        dtls_cfg,
        ..Default::default()
    };

    log::info!("Server listening on {}", addr);
    log::info!("Routes configured:");
    log::info!("  POST   .d/:device_id  - Update device state");
    log::info!("  GET    .d/:device_id  - Get device state");
    log::info!("  DELETE .d/:device_id  - Delete device state");
    log::info!("  POST   .s/:stream_id  - Handle stream data");
    log::info!("  PUT    echo           - Echo payload");
    log::info!("  GET    hello          - Echo payload");
    log::info!("  ANY    /              - Ping");

    // Start the server
    if let Err(e) = serve::serve(addr.to_string(), cfg, router).await {
        log::error!("Server error: {}", e);
    }
}
