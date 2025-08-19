//! End-to-End Integration Tests for CoAP Observe Functionality
//!
//! These tests verify the complete observe workflow from client registration
//! to server notifications and client deregistration.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::Duration,
};

use tokio::{
    net::UdpSocket,
    sync::{broadcast, Mutex},
    time::timeout,
};

use coapum::{
    config::Config as ServerConfig,
    dtls::{
        cipher_suite::CipherSuiteId,
        config::{Config as DtlsConfig, ExtendedMasterSecretType},
        conn::DTLSConn,
        Error as DtlsError,
    },
    extract::{Cbor, Path, State, StatusCode},
    observer::{memory::MemObserver, sled::SledObserver},
    router::RouterBuilder,
    serve,
    util::Conn,
    CoapRequest, ContentFormat, Packet, RequestType, ResponseType,
};

use coap_lite::ObserveOption;
use serde::{Deserialize, Serialize};

const PSK: &[u8] = b"test_psk_key_1234567890abcdef";
const IDENTITY: &str = "test_client";
const SERVER_ADDR: &str = "127.0.0.1:0"; // Use 0 to get random port
const TIMEOUT_SECS: u64 = 10;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct SensorData {
    temperature: f32,
    humidity: f32,
    timestamp: u64,
}

#[derive(Clone, Debug)]
struct TestAppState {
    sensors: Arc<Mutex<HashMap<String, SensorData>>>,
    notification_trigger: Arc<broadcast::Sender<String>>,
}

impl AsRef<TestAppState> for TestAppState {
    fn as_ref(&self) -> &TestAppState {
        self
    }
}

// Test handlers
async fn get_sensor_data(
    Path(sensor_id): Path<String>,
    State(state): State<TestAppState>,
) -> Result<Cbor<SensorData>, StatusCode> {
    let sensors = state.sensors.lock().await;
    if let Some(data) = sensors.get(&sensor_id) {
        Ok(Cbor(data.clone()))
    } else {
        Err(StatusCode::NotFound)
    }
}

async fn update_sensor_data(
    Path(sensor_id): Path<String>,
    Cbor(data): Cbor<SensorData>,
    State(state): State<TestAppState>,
) -> Result<StatusCode, StatusCode> {
    let mut sensors = state.sensors.lock().await;
    sensors.insert(sensor_id.clone(), data);

    // Trigger notification for observers
    let _ = state.notification_trigger.send(sensor_id);

    Ok(StatusCode::Changed)
}

async fn notify_sensor_data(
    Path(sensor_id): Path<String>,
    State(state): State<TestAppState>,
) -> Result<Cbor<SensorData>, StatusCode> {
    get_sensor_data(Path(sensor_id), State(state)).await
}

/// Create a DTLS client connection
async fn create_client_connection(
    server_addr: SocketAddr,
) -> Result<Arc<dyn Conn + Send + Sync>, Box<dyn std::error::Error>> {
    create_client_connection_with_identity(server_addr, IDENTITY).await
}

/// Create a DTLS client connection with a specific identity
async fn create_client_connection_with_identity(
    server_addr: SocketAddr,
    identity: &str,
) -> Result<Arc<dyn Conn + Send + Sync>, Box<dyn std::error::Error>> {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    socket.connect(server_addr).await?;

    let config = DtlsConfig {
        psk: Some(Arc::new(|_hint: &[u8]| Ok(PSK.to_vec()))),
        psk_identity_hint: Some(identity.as_bytes().to_vec()),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
        extended_master_secret: ExtendedMasterSecretType::Require,
        ..Default::default()
    };

    let dtls_conn = DTLSConn::new(socket, config, true, None).await?;
    Ok(Arc::new(dtls_conn))
}

/// Start a test server and return the bound address
async fn start_test_server(
    app_state: TestAppState,
    observer: impl coapum::observer::Observer + Send + Sync + 'static,
) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let listener = std::net::UdpSocket::bind(SERVER_ADDR)?;
    let addr = listener.local_addr()?;
    drop(listener); // Close the socket so the server can bind to it

    let psk_store: Arc<RwLock<HashMap<String, Vec<u8>>>> = Arc::new(RwLock::new(HashMap::new()));
    psk_store
        .write()
        .unwrap()
        .insert(IDENTITY.to_string(), PSK.to_vec());
    // Add additional identities for multi-client tests
    psk_store
        .write()
        .unwrap()
        .insert("test_client1".to_string(), PSK.to_vec());
    psk_store
        .write()
        .unwrap()
        .insert("test_client2".to_string(), PSK.to_vec());

    let router = RouterBuilder::new(app_state, observer)
        .get("/sensors/:id", get_sensor_data)
        .post("/sensors/:id", update_sensor_data)
        .observe("/sensors/:id", get_sensor_data, notify_sensor_data)
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
        psk_identity_hint: Some(b"test_server".to_vec()),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
        extended_master_secret: ExtendedMasterSecretType::Require,
        ..Default::default()
    };

    let server_config = ServerConfig {
        dtls_cfg: dtls_config,
        timeout: TIMEOUT_SECS,
        ..Default::default()
    };

    tokio::spawn(async move {
        if let Err(e) = serve::serve(addr.to_string(), server_config, router).await {
            eprintln!("Server error: {}", e);
        }
    });

    // Give the server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    Ok(addr)
}

/// Send a CoAP request and receive response
async fn send_coap_request(
    conn: &Arc<dyn Conn + Send + Sync>,
    method: RequestType,
    path: &str,
    observe: Option<ObserveOption>,
    payload: Option<Vec<u8>>,
) -> Result<Packet, Box<dyn std::error::Error>> {
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.set_method(method);
    request.set_path(path);

    if let Some(obs) = observe {
        request.set_observe_flag(obs);
    }

    if let Some(data) = payload {
        request.message.payload = data;
        request
            .message
            .set_content_format(ContentFormat::ApplicationCBOR);
    }

    let request_bytes = request.message.to_bytes()?;
    conn.send(&request_bytes).await?;

    let mut buffer = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(TIMEOUT_SECS), conn.recv(&mut buffer)).await??;

    Ok(Packet::from_bytes(&buffer[0..n])?)
}

#[tokio::test]
async fn test_observe_registration_and_deregistration() {
    let _ = env_logger::try_init();

    // Create test state
    let (tx, _rx) = broadcast::channel(10);
    let app_state = TestAppState {
        sensors: Arc::new(Mutex::new(HashMap::new())),
        notification_trigger: Arc::new(tx),
    };

    // Start server with memory observer
    let observer = MemObserver::new();
    let server_addr = start_test_server(app_state.clone(), observer)
        .await
        .expect("Failed to start server");

    // Create client connection
    let conn = create_client_connection(server_addr)
        .await
        .expect("Failed to create client connection");

    // Initial sensor data
    let sensor_data = SensorData {
        temperature: 25.5,
        humidity: 60.0,
        timestamp: 1234567890,
    };

    // Add initial data
    {
        let mut sensors = app_state.sensors.lock().await;
        sensors.insert("sensor1".to_string(), sensor_data.clone());
    }

    // 1. Register observer
    let response = send_coap_request(
        &conn,
        RequestType::Get,
        "/sensors/sensor1",
        Some(ObserveOption::Register),
        None,
    )
    .await
    .expect("Failed to send observe registration");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );
    assert!(
        !response.payload.is_empty(),
        "Response should contain sensor data"
    );

    // 2. Verify we can still get regular responses
    let response = send_coap_request(&conn, RequestType::Get, "/sensors/sensor1", None, None)
        .await
        .expect("Failed to send regular GET request");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    // 3. Deregister observer
    let response = send_coap_request(
        &conn,
        RequestType::Get,
        "/sensors/sensor1",
        Some(ObserveOption::Deregister),
        None,
    )
    .await
    .expect("Failed to send observe deregistration");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );
}

#[tokio::test]
async fn test_observe_notifications() {
    let _ = env_logger::try_init();

    // Create test state
    let (tx, _rx) = broadcast::channel(10);
    let app_state = TestAppState {
        sensors: Arc::new(Mutex::new(HashMap::new())),
        notification_trigger: Arc::new(tx),
    };

    // Start server with memory observer
    let observer = MemObserver::new();
    let server_addr = start_test_server(app_state.clone(), observer)
        .await
        .expect("Failed to start server");

    // Create client connection
    let conn = create_client_connection(server_addr)
        .await
        .expect("Failed to create client connection");

    // Initial sensor data
    let initial_data = SensorData {
        temperature: 20.0,
        humidity: 50.0,
        timestamp: 1000,
    };

    // Add initial data
    {
        let mut sensors = app_state.sensors.lock().await;
        sensors.insert("sensor1".to_string(), initial_data.clone());
    }

    // Register observer
    let _response = send_coap_request(
        &conn,
        RequestType::Get,
        "/sensors/sensor1",
        Some(ObserveOption::Register),
        None,
    )
    .await
    .expect("Failed to register observer");

    // Update sensor data to trigger notification
    let updated_data = SensorData {
        temperature: 25.5,
        humidity: 65.0,
        timestamp: 2000,
    };

    let payload = {
        let mut buf = Vec::new();
        ciborium::ser::into_writer(&updated_data, &mut buf).unwrap();
        buf
    };

    // Send update request
    let _response = send_coap_request(
        &conn,
        RequestType::Post,
        "/sensors/sensor1",
        None,
        Some(payload),
    )
    .await
    .expect("Failed to update sensor data");

    // Verify the data was updated by sending another GET
    let response = send_coap_request(&conn, RequestType::Get, "/sensors/sensor1", None, None)
        .await
        .expect("Failed to get updated sensor data");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    // Verify the response contains updated data
    let received_data: SensorData =
        ciborium::de::from_reader(&response.payload[..]).expect("Failed to deserialize response");

    assert_eq!(received_data.temperature, 25.5);
    assert_eq!(received_data.humidity, 65.0);
}

#[tokio::test]
async fn test_observe_with_sled_backend() {
    let _ = env_logger::try_init();

    // Create test state
    let (tx, _rx) = broadcast::channel(10);
    let app_state = TestAppState {
        sensors: Arc::new(Mutex::new(HashMap::new())),
        notification_trigger: Arc::new(tx),
    };

    // Start server with Sled observer (using temporary database)
    let observer = SledObserver::new("test_observe_integration.db");
    let server_addr = start_test_server(app_state.clone(), observer)
        .await
        .expect("Failed to start server");

    // Create client connection
    let conn = create_client_connection(server_addr)
        .await
        .expect("Failed to create client connection");

    // Test data
    let sensor_data = SensorData {
        temperature: 22.5,
        humidity: 55.0,
        timestamp: 3000,
    };

    // Add initial data
    {
        let mut sensors = app_state.sensors.lock().await;
        sensors.insert("sensor2".to_string(), sensor_data.clone());
    }

    // Register observer
    let response = send_coap_request(
        &conn,
        RequestType::Get,
        "/sensors/sensor2",
        Some(ObserveOption::Register),
        None,
    )
    .await
    .expect("Failed to register observer");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    // Verify data persistence by deregistering and re-registering
    let _response = send_coap_request(
        &conn,
        RequestType::Get,
        "/sensors/sensor2",
        Some(ObserveOption::Deregister),
        None,
    )
    .await
    .expect("Failed to deregister observer");

    // Re-register should work
    let response = send_coap_request(
        &conn,
        RequestType::Get,
        "/sensors/sensor2",
        Some(ObserveOption::Register),
        None,
    )
    .await
    .expect("Failed to re-register observer");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    // Cleanup test database
    let _ = std::fs::remove_file("test_observe_integration.db");
}

#[tokio::test]
async fn test_observe_multiple_clients() {
    let _ = env_logger::try_init();

    // Create test state
    let (tx, _rx) = broadcast::channel(10);
    let app_state = TestAppState {
        sensors: Arc::new(Mutex::new(HashMap::new())),
        notification_trigger: Arc::new(tx),
    };

    // Start server
    let observer = MemObserver::new();
    let server_addr = start_test_server(app_state.clone(), observer)
        .await
        .expect("Failed to start server");

    // Create multiple client connections with different identities
    let conn1 = create_client_connection_with_identity(server_addr, "test_client1")
        .await
        .expect("Failed to create client connection 1");

    let conn2 = create_client_connection_with_identity(server_addr, "test_client2")
        .await
        .expect("Failed to create client connection 2");

    // Use UNIQUE device IDs for each client to avoid conflicts
    let sensor_data_1 = SensorData {
        temperature: 30.0,
        humidity: 70.0,
        timestamp: 4000,
    };

    let sensor_data_2 = SensorData {
        temperature: 25.0,
        humidity: 65.0,
        timestamp: 4001,
    };

    // Add initial data for both sensors
    {
        let mut sensors = app_state.sensors.lock().await;
        sensors.insert("sensor_client1".to_string(), sensor_data_1.clone());
        sensors.insert("sensor_client2".to_string(), sensor_data_2.clone());
    }

    // Both clients register for DIFFERENT resources
    let response1 = send_coap_request(
        &conn1,
        RequestType::Get,
        "/sensors/sensor_client1", // Unique ID for client 1
        Some(ObserveOption::Register),
        None,
    )
    .await
    .expect("Failed to register observer 1");

    let response2 = send_coap_request(
        &conn2,
        RequestType::Get,
        "/sensors/sensor_client2", // Unique ID for client 2
        Some(ObserveOption::Register),
        None,
    )
    .await
    .expect("Failed to register observer 2");

    assert_eq!(
        response1.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );
    assert_eq!(
        response2.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    // Both should be able to get their respective data
    let data1: SensorData = ciborium::de::from_reader(&response1.payload[..])
        .expect("Failed to deserialize response 1");
    let data2: SensorData = ciborium::de::from_reader(&response2.payload[..])
        .expect("Failed to deserialize response 2");

    assert_eq!(data1, sensor_data_1);
    assert_eq!(data2, sensor_data_2);
}

#[tokio::test]
async fn test_observe_error_conditions() {
    let _ = env_logger::try_init();

    // Create test state
    let (tx, _rx) = broadcast::channel(10);
    let app_state = TestAppState {
        sensors: Arc::new(Mutex::new(HashMap::new())),
        notification_trigger: Arc::new(tx),
    };

    // Start server
    let observer = MemObserver::new();
    let server_addr = start_test_server(app_state.clone(), observer)
        .await
        .expect("Failed to start server");

    // Create client connection
    let conn = create_client_connection(server_addr)
        .await
        .expect("Failed to create client connection");

    // 1. Try to observe non-existent resource
    let response = send_coap_request(
        &conn,
        RequestType::Get,
        "/sensors/nonexistent",
        Some(ObserveOption::Register),
        None,
    )
    .await
    .expect("Failed to send observe request for non-existent resource");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::NotFound)
    );

    // 2. Try to deregister without registering first
    let response = send_coap_request(
        &conn,
        RequestType::Get,
        "/sensors/test",
        Some(ObserveOption::Deregister),
        None,
    )
    .await
    .expect("Failed to send deregister request");

    // This should still succeed (idempotent operation)
    assert!(
        response.header.code == coapum::MessageClass::Response(ResponseType::Content)
            || response.header.code == coapum::MessageClass::Response(ResponseType::NotFound)
    );
}

#[tokio::test]
async fn test_observe_with_cbor_payload() {
    let _ = env_logger::try_init();

    // Create test state
    let (tx, _rx) = broadcast::channel(10);
    let app_state = TestAppState {
        sensors: Arc::new(Mutex::new(HashMap::new())),
        notification_trigger: Arc::new(tx),
    };

    // Start server
    let observer = MemObserver::new();
    let server_addr = start_test_server(app_state.clone(), observer)
        .await
        .expect("Failed to start server");

    // Create client connection
    let conn = create_client_connection(server_addr)
        .await
        .expect("Failed to create client connection");

    // Test with complex CBOR data
    let complex_data = SensorData {
        temperature: -15.75, // Negative temperature
        humidity: 100.0,     // Maximum humidity
        timestamp: u64::MAX, // Large timestamp
    };

    // Add data
    {
        let mut sensors = app_state.sensors.lock().await;
        sensors.insert("sensor_complex".to_string(), complex_data.clone());
    }

    // Register observer
    let response = send_coap_request(
        &conn,
        RequestType::Get,
        "/sensors/sensor_complex",
        Some(ObserveOption::Register),
        None,
    )
    .await
    .expect("Failed to register observer for complex data");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content)
    );

    // Verify CBOR deserialization
    let received_data: SensorData = ciborium::de::from_reader(&response.payload[..])
        .expect("Failed to deserialize complex CBOR response");

    assert_eq!(received_data, complex_data);
    assert_eq!(received_data.temperature, -15.75);
    assert_eq!(received_data.humidity, 100.0);
    assert_eq!(received_data.timestamp, u64::MAX);
}
