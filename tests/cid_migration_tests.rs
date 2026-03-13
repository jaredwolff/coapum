//! Integration tests for DTLS Connection ID address migration (RFC 9146).
//!
//! Verifies that a client whose source IP:port changes (NAT rebinding) can
//! continue communicating with the server when CID is negotiated, without
//! requiring a new DTLS handshake.
//!
//! **Must run with `--test-threads=1`** to avoid port conflicts from the
//! bind-drop-rebind pattern used to discover free ports.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
    time::Duration,
};

use tokio::sync::Mutex;

use coapum::{
    CoapRequest, MemoryCredentialStore, Packet, RequestType, ResponseType,
    client::DtlsClient,
    config::Config as ServerConfig,
    credential::resolver::MapResolver,
    extract::{Cbor, Path, State, StatusCode},
    observer::memory::MemObserver,
    router::RouterBuilder,
    serve,
};

use coap_lite::ObserveOption;
use serde::{Deserialize, Serialize};

const PSK: &[u8] = b"test_psk_key_1234567890abcdef";
const IDENTITY: &str = "cid_test_client";
const TIMEOUT_SECS: u64 = 10;
const CID_LENGTH: usize = 4;

static MSG_ID_COUNTER: AtomicU16 = AtomicU16::new(1000);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct SensorData {
    value: f32,
}

#[derive(Clone, Debug)]
struct AppState {
    sensors: Arc<Mutex<HashMap<String, SensorData>>>,
}

impl AsRef<AppState> for AppState {
    fn as_ref(&self) -> &AppState {
        self
    }
}

async fn get_sensor(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Cbor<SensorData>, StatusCode> {
    let sensors = state.sensors.lock().await;
    sensors
        .get(&id)
        .cloned()
        .map(Cbor)
        .ok_or(StatusCode::NotFound)
}

async fn notify_sensor(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Cbor<SensorData>, StatusCode> {
    get_sensor(Path(id), State(state)).await
}

/// Start a test server with CID enabled.
async fn start_cid_server(app_state: AppState) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let listener = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    drop(listener);

    let mut clients = HashMap::new();
    clients.insert(IDENTITY.to_string(), PSK.to_vec());

    let credential_store = MemoryCredentialStore::from_clients(&clients);

    let observer = MemObserver::new();
    let router = RouterBuilder::new(app_state, observer)
        .get("/sensors/:id", get_sensor)
        .observe("/sensors/:id", get_sensor, notify_sensor)
        .build();

    let mut server_config = ServerConfig {
        psk_identity_hint: Some(b"cid_test_server".to_vec()),
        timeout: TIMEOUT_SECS,
        ..Default::default()
    };
    server_config.set_cid_length(CID_LENGTH).unwrap();

    tokio::spawn(async move {
        if let Err(e) = serve::serve_with_credential_store(
            addr.to_string(),
            server_config,
            router,
            credential_store,
        )
        .await
        {
            eprintln!("Server error: {}", e);
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    Ok(addr)
}

/// Create a DTLS client with CID negotiation enabled.
async fn create_cid_client(
    server_addr: SocketAddr,
) -> Result<DtlsClient, Box<dyn std::error::Error>> {
    let mut keys = HashMap::new();
    keys.insert(IDENTITY.to_string(), PSK.to_vec());
    let resolver = Arc::new(MapResolver::new(keys));

    let config = dimpl::Config::builder()
        .with_psk_client(
            IDENTITY.as_bytes().to_vec(),
            resolver as Arc<dyn dimpl::PskResolver>,
        )
        .with_connection_id(vec![0u8; CID_LENGTH])
        .build()
        .expect("valid DTLS config");

    DtlsClient::connect(&server_addr.to_string(), Arc::new(config)).await
}

/// Send a CoAP request and receive response.
async fn send_coap_request(
    client: &mut DtlsClient,
    method: RequestType,
    path: &str,
    observe: Option<ObserveOption>,
) -> Result<Packet, Box<dyn std::error::Error>> {
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.message.header.message_id = MSG_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    request.set_method(method);
    request.set_path(path);

    if let Some(obs) = observe {
        request.set_observe_flag(obs);
    }

    let request_bytes = request.message.to_bytes()?;
    client.send(&request_bytes).await?;

    let data = client.recv(Duration::from_secs(TIMEOUT_SECS)).await?;
    Ok(Packet::from_bytes(&data)?)
}

#[tokio::test]
async fn test_cid_address_migration() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    // Seed data
    let app_state = AppState {
        sensors: Arc::new(Mutex::new(HashMap::from([(
            "temp".to_string(),
            SensorData { value: 22.5 },
        )]))),
    };

    let server_addr = start_cid_server(app_state)
        .await
        .expect("Failed to start server");

    let mut client = create_cid_client(server_addr)
        .await
        .expect("Failed to connect client");

    // 1. Send request before migration — proves baseline works
    let response = send_coap_request(&mut client, RequestType::Get, "/sensors/temp", None)
        .await
        .expect("Failed to send pre-migration request");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content),
        "Pre-migration GET should return 2.05 Content"
    );

    let data: SensorData =
        ciborium::de::from_reader(&response.payload[..]).expect("Failed to deserialize");
    assert_eq!(data.value, 22.5);

    // 2. Rebind — simulate NAT rebinding (new source port)
    let addr_before = client.local_addr().expect("local_addr before rebind");
    client.rebind().await.expect("Failed to rebind");
    let addr_after = client.local_addr().expect("local_addr after rebind");

    assert_ne!(
        addr_before.port(),
        addr_after.port(),
        "Rebind should change the local port (was {}, now {})",
        addr_before.port(),
        addr_after.port()
    );

    // 3. Send request after migration — proves CID routing works
    let response = send_coap_request(&mut client, RequestType::Get, "/sensors/temp", None)
        .await
        .expect("Failed to send post-migration request");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content),
        "Post-migration GET should return 2.05 Content (CID routing)"
    );

    let data: SensorData =
        ciborium::de::from_reader(&response.payload[..]).expect("Failed to deserialize");
    assert_eq!(data.value, 22.5);
}

#[tokio::test]
async fn test_cid_migration_preserves_observer() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let app_state = AppState {
        sensors: Arc::new(Mutex::new(HashMap::from([(
            "temp".to_string(),
            SensorData { value: 20.0 },
        )]))),
    };

    let server_addr = start_cid_server(app_state.clone())
        .await
        .expect("Failed to start server");

    let mut client = create_cid_client(server_addr)
        .await
        .expect("Failed to connect client");

    // 1. Register observer before migration
    let response = send_coap_request(
        &mut client,
        RequestType::Get,
        "/sensors/temp",
        Some(ObserveOption::Register),
    )
    .await
    .expect("Failed to register observer");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content),
    );

    // 2. Rebind
    client.rebind().await.expect("Failed to rebind");

    // 3. Verify we can still query after migration
    let response = send_coap_request(&mut client, RequestType::Get, "/sensors/temp", None)
        .await
        .expect("Failed to send post-migration request");

    assert_eq!(
        response.header.code,
        coapum::MessageClass::Response(ResponseType::Content),
        "Post-migration GET should succeed (observer + CID)"
    );
}
