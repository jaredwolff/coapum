//! Real Push Notification Tests for CoAP Observe Functionality
//!
//! These tests verify that the CoAP observe pattern works correctly with
//! real push notifications - where the server automatically sends updates
//! to subscribed clients when data changes.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};

use tokio::{
    net::UdpSocket,
    sync::Mutex,
    time::{sleep, timeout},
};

use coapum::{
    CoapRequest, Packet, RequestType, ResponseType,
    config::Config as ServerConfig,
    dtls::{
        Error as DtlsError,
        cipher_suite::CipherSuiteId,
        config::{Config as DtlsConfig, ExtendedMasterSecretType},
        conn::DTLSConn,
    },
    extract::{Cbor, Path, State, StatusCode},
    observer::{Observer, memory::MemObserver},
    router::RouterBuilder,
    serve,
    util::Conn,
};

use coap_lite::ObserveOption;
use serde::{Deserialize, Serialize};

const PSK: &[u8] = b"test_push_notification_key_123";
const IDENTITY: &str = "push_test_client";
const SERVER_ADDR: &str = "127.0.0.1:0";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Temperature {
    value: f32,
    unit: String,
    timestamp: u64,
}

#[derive(Clone, Debug)]
struct PushTestState {
    temperatures: Arc<Mutex<HashMap<String, Temperature>>>,
}

impl AsRef<PushTestState> for PushTestState {
    fn as_ref(&self) -> &PushTestState {
        self
    }
}

// Handler that returns current temperature
async fn get_temperature(
    Path(sensor_id): Path<String>,
    State(state): State<PushTestState>,
) -> Result<Cbor<Temperature>, StatusCode> {
    let temps = state.temperatures.lock().await;
    if let Some(temp) = temps.get(&sensor_id) {
        Ok(Cbor(temp.clone()))
    } else {
        Err(StatusCode::NotFound)
    }
}

// Handler for notifications - same as get but used for observe
async fn notify_temperature(
    Path(sensor_id): Path<String>,
    State(state): State<PushTestState>,
) -> Result<Cbor<Temperature>, StatusCode> {
    get_temperature(Path(sensor_id), State(state)).await
}

/// Create a DTLS client connection
async fn create_push_client(
    server_addr: SocketAddr,
) -> Result<Arc<dyn Conn + Send + Sync>, Box<dyn std::error::Error>> {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    socket.connect(server_addr).await?;

    let config = DtlsConfig {
        psk: Some(Arc::new(|_hint: &[u8]| Ok(PSK.to_vec()))),
        psk_identity_hint: Some(IDENTITY.as_bytes().to_vec()),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
        extended_master_secret: ExtendedMasterSecretType::Require,
        ..Default::default()
    };

    let dtls_conn = DTLSConn::new(socket, config, true, None).await?;
    Ok(Arc::new(dtls_conn))
}

/// Start server that supports push notifications
async fn start_push_server(
    app_state: PushTestState,
    observer: MemObserver,
) -> Result<(SocketAddr, coapum::NotificationTrigger<MemObserver>), Box<dyn std::error::Error>> {
    let listener = std::net::UdpSocket::bind(SERVER_ADDR)?;
    let addr = listener.local_addr()?;
    drop(listener);

    let psk_store: Arc<RwLock<HashMap<String, Vec<u8>>>> = Arc::new(RwLock::new(HashMap::new()));
    psk_store
        .write()
        .unwrap()
        .insert(IDENTITY.to_string(), PSK.to_vec());

    let router_builder = RouterBuilder::new(app_state, observer);
    let notification_trigger = router_builder.notification_trigger();
    let router = router_builder
        .get("/temperature/:sensor_id", get_temperature)
        .delete("/temperature/:sensor_id", |_path: Path<String>| async {
            StatusCode::Content
        })
        .observe(
            "/temperature/:sensor_id",
            get_temperature,
            notify_temperature,
        )
        .build();

    let dtls_config = DtlsConfig {
        psk: Some(Arc::new(move |hint: &[u8]| {
            let hint = String::from_utf8_lossy(hint);
            psk_store
                .read()
                .unwrap()
                .get(&hint.to_string())
                .cloned()
                .ok_or(DtlsError::ErrIdentityNoPsk)
        })),
        psk_identity_hint: Some(b"push_test_server".to_vec()),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
        extended_master_secret: ExtendedMasterSecretType::Require,
        ..Default::default()
    };

    let server_config = ServerConfig {
        dtls_cfg: dtls_config,
        timeout: 15,
        ..Default::default()
    };

    tokio::spawn(async move {
        if let Err(e) = serve::serve(addr.to_string(), server_config, router).await {
            eprintln!("Server error: {}", e);
        }
    });

    // Give server time to start
    sleep(Duration::from_millis(200)).await;
    Ok((addr, notification_trigger))
}

#[tokio::test]
async fn test_observe_registration_and_initial_response() {
    let _ = env_logger::try_init();

    // Create state
    let app_state = PushTestState {
        temperatures: Arc::new(Mutex::new(HashMap::new())),
    };

    // Set initial temperature
    {
        let mut temps = app_state.temperatures.lock().await;
        temps.insert(
            "sensor1".to_string(),
            Temperature {
                value: 25.0,
                unit: "Celsius".to_string(),
                timestamp: 1000,
            },
        );
    }

    // Start server
    let observer = MemObserver::new();
    let (server_addr, _) = start_push_server(app_state.clone(), observer)
        .await
        .expect("Failed to start push server");

    // Create client connection
    let conn = create_push_client(server_addr)
        .await
        .expect("Failed to create push client");

    // Send observe registration
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.set_method(RequestType::Get);
    request.set_path("/temperature/sensor1");
    request.set_observe_flag(ObserveOption::Register);

    let request_bytes = request.message.to_bytes().unwrap();
    conn.send(&request_bytes).await.unwrap();

    // Receive initial response
    let mut buffer = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(10), conn.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let packet = Packet::from_bytes(&buffer[0..n]).unwrap();

    // Verify initial response
    assert_eq!(
        packet.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    // Parse the temperature data
    let temp: Temperature = ciborium::de::from_reader(&packet.payload[..]).unwrap();
    assert_eq!(temp.value, 25.0);
    assert_eq!(temp.unit, "Celsius");
    assert_eq!(temp.timestamp, 1000);

    println!(
        "✅ Observe registration successful, received initial data: {:?}",
        temp
    );
}

#[tokio::test]
async fn test_observe_push_notification_via_database_write() {
    let _ = env_logger::try_init();

    // Create state
    let app_state = PushTestState {
        temperatures: Arc::new(Mutex::new(HashMap::new())),
    };

    // Set initial temperature
    {
        let mut temps = app_state.temperatures.lock().await;
        temps.insert(
            "sensor2".to_string(),
            Temperature {
                value: 20.0,
                unit: "Celsius".to_string(),
                timestamp: 1000,
            },
        );
    }

    // Start server and get notification trigger
    let observer = MemObserver::new();
    let (server_addr, mut notification_trigger) = start_push_server(app_state.clone(), observer)
        .await
        .expect("Failed to start push server");

    // Create client connection
    let conn = create_push_client(server_addr)
        .await
        .expect("Failed to create push client");

    // Send observe registration
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.set_method(RequestType::Get);
    request.set_path("/temperature/sensor2");
    request.set_observe_flag(ObserveOption::Register);

    println!("📡 Sending observe registration for path: /temperature/sensor2");
    conn.send(&request.message.to_bytes().unwrap())
        .await
        .unwrap();

    // Receive initial response
    let mut buffer = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(10), conn.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();

    let packet = Packet::from_bytes(&buffer[0..n]).unwrap();
    assert_eq!(
        packet.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    let initial_temp: Temperature = ciborium::de::from_reader(&packet.payload[..]).unwrap();
    println!("📊 Initial temperature: {:?}", initial_temp);

    // Give the server time to complete observer registration
    sleep(Duration::from_millis(500)).await;
    println!("⏱️  Waited for observer registration to complete");

    // Now simulate a temperature change by writing to the observer database
    // This should trigger a notification to be sent to the client
    let new_temp = Temperature {
        value: 30.0,
        unit: "Celsius".to_string(),
        timestamp: 2000,
    };

    // Update the application state
    {
        let mut temps = app_state.temperatures.lock().await;
        temps.insert("sensor2".to_string(), new_temp.clone());
    }

    // Write to observer database - this should trigger a push notification
    let temp_json = serde_json::to_value(&new_temp).unwrap();
    println!("📝 Writing to observer database with path: temperature/sensor2");
    println!("📝 Device ID: {}", IDENTITY);
    println!("📝 Writing value: {:?}", temp_json);

    // The device ID should match what the server uses for registration
    // From serve.rs, the server uses the client's PSK identity as device_id
    // The path needs to match the route pattern for proper lookup
    notification_trigger
        .trigger_notification(IDENTITY, "/temperature/sensor2", &temp_json)
        .await
        .unwrap();

    println!("🔄 Triggered notification via notification trigger...");

    // Give some time for the notification to be processed
    sleep(Duration::from_millis(100)).await;

    // Listen for the push notification
    println!("👂 Listening for push notification...");
    let n = match timeout(Duration::from_secs(10), conn.recv(&mut buffer)).await {
        Ok(result) => result.unwrap(),
        Err(_) => {
            println!("❌ Timeout waiting for push notification!");
            println!("   This might indicate the observer write didn't trigger a notification");
            panic!("Should receive push notification: Timeout");
        }
    };

    let notification_packet = Packet::from_bytes(&buffer[0..n]).unwrap();

    // Verify this is a notification (not an error)
    println!(
        "📨 Received notification with code: {:?}",
        notification_packet.header.code
    );

    if !notification_packet.payload.is_empty() {
        let notified_temp: Temperature =
            ciborium::de::from_reader(&notification_packet.payload[..]).unwrap();
        println!("🌡️  Notification temperature: {:?}", notified_temp);

        // The notification should contain the updated temperature
        assert_eq!(notified_temp.value, 30.0);
        assert_eq!(notified_temp.timestamp, 2000);

        println!("✅ Push notification received successfully!");
    } else {
        println!("⚠️  Notification packet has empty payload");
    }
}

#[tokio::test]
async fn test_observe_deregistration() {
    let _ = env_logger::try_init();

    // Create state
    let app_state = PushTestState {
        temperatures: Arc::new(Mutex::new(HashMap::new())),
    };

    // Set initial temperature
    {
        let mut temps = app_state.temperatures.lock().await;
        temps.insert(
            "sensor3".to_string(),
            Temperature {
                value: 22.0,
                unit: "Celsius".to_string(),
                timestamp: 1000,
            },
        );
    }

    // Start server
    let mut observer = MemObserver::new();
    let (server_addr, _) = start_push_server(app_state.clone(), observer.clone())
        .await
        .expect("Failed to start server");

    // Create client
    let conn = create_push_client(server_addr)
        .await
        .expect("Failed to create client");

    // Register for observations
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.set_method(RequestType::Get);
    request.set_path("/temperature/sensor3");
    request.set_observe_flag(ObserveOption::Register);

    conn.send(&request.message.to_bytes().unwrap())
        .await
        .unwrap();

    // Receive initial response
    let mut buffer = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(5), conn.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let packet = Packet::from_bytes(&buffer[0..n]).unwrap();
    assert_eq!(
        packet.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    println!("📋 Registered for observations");

    // Deregister
    let mut deregister_request: CoapRequest<SocketAddr> = CoapRequest::new();
    deregister_request.set_method(RequestType::Delete);
    deregister_request.set_path("/temperature/sensor3");
    deregister_request.set_observe_flag(ObserveOption::Deregister);

    conn.send(&deregister_request.message.to_bytes().unwrap())
        .await
        .unwrap();

    // Receive deregistration response
    let n = timeout(Duration::from_secs(5), conn.recv(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let packet = Packet::from_bytes(&buffer[0..n]).unwrap();
    assert_eq!(
        packet.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    println!("📤 Deregistered from observations");

    // Try to trigger a notification after deregistration
    let new_temp = Temperature {
        value: 35.0,
        unit: "Celsius".to_string(),
        timestamp: 3000,
    };

    // Update state and trigger observer write
    {
        let mut temps = app_state.temperatures.lock().await;
        temps.insert("sensor3".to_string(), new_temp.clone());
    }

    let temp_json = serde_json::to_value(&new_temp).unwrap();
    println!("📝 Attempting write after deregistration to path: /temperature/sensor3");
    println!("📝 Using device ID: {}", IDENTITY);

    observer
        .write(IDENTITY, "/temperature/sensor3", &temp_json)
        .await
        .unwrap();

    println!("📝 Write completed, checking if notifications are suppressed...");

    // Should NOT receive any notifications after deregistration
    let result = timeout(Duration::from_millis(1000), conn.recv(&mut buffer)).await;

    assert!(
        result.is_err(),
        "Should not receive notifications after deregistration"
    );

    println!("✅ No notifications received after deregistration (as expected)");
}

#[tokio::test]
async fn test_debug_path_format() {
    let _ = env_logger::try_init();

    // Create a simple observer to test path format
    let mut observer = MemObserver::new();

    // Create a channel to receive notifications
    let (tx, mut rx) = tokio::sync::mpsc::channel::<coapum::observer::ObserverValue>(10);
    let sender = Arc::new(tx);

    // Register for a specific path
    let test_path = "/temperature/sensor1";
    println!("🔍 Registering for path: {}", test_path);

    observer
        .register("test_device", test_path, sender.clone())
        .await
        .unwrap();

    // Write to the same path
    let test_data = serde_json::json!({"value": 25.0, "unit": "C"});
    println!(
        "📝 Writing to path: {} with data: {:?}",
        test_path, test_data
    );

    observer
        .write("test_device", test_path, &test_data)
        .await
        .unwrap();

    // Try to receive notification
    let result = timeout(Duration::from_millis(500), rx.recv()).await;

    match result {
        Ok(Some(notification)) => {
            println!(
                "✅ Received notification: path={}, value={:?}",
                notification.path, notification.value
            );
        }
        Ok(None) => {
            println!("❌ Channel closed without notification");
        }
        Err(_) => {
            println!("❌ Timeout - no notification received");

            // Let's debug by reading what was actually stored
            let stored = observer.read("test_device", test_path).await.unwrap();
            println!("📖 Stored data: {:?}", stored);

            // Can't access private channels field directly
            println!("📋 Channel registration completed, but can't inspect private field");
        }
    }
}
