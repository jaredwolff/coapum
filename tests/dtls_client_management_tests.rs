//! Integration tests for `serve_with_client_management` DTLS API.
//!
//! Verifies that the server can be started with initial client credentials,
//! that a DTLS client can connect and communicate, and that `ClientManager`
//! can dynamically add, list, update, and remove clients while the server
//! is running.
//!
//! **Must run with `--test-threads=1`** to avoid port conflicts.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
    time::Duration,
};

use coap_lite::{CoapRequest, MessageClass, Packet, RequestType, ResponseType};
use coapum::{
    client::DtlsClient, config::Config, credential::resolver::MapResolver, extract::Bytes,
    observer::memory::MemObserver, router::RouterBuilder, serve,
};

const INITIAL_PSK: &[u8] = b"initial_psk_key_123";
const INITIAL_IDENTITY: &str = "initial_client";
const TIMEOUT_SECS: u64 = 10;

static MSG_ID_COUNTER: AtomicU16 = AtomicU16::new(1);

fn next_msg_id() -> u16 {
    MSG_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

async fn echo_handler(body: Bytes) -> Bytes {
    body
}

/// Build a DTLS client config for the given identity and PSK.
fn build_dtls_config(identity: &str, psk: &[u8]) -> Arc<dimpl::Config> {
    let mut keys = HashMap::new();
    keys.insert(identity.to_string(), psk.to_vec());
    let resolver = Arc::new(MapResolver::new(keys));

    Arc::new(
        dimpl::Config::builder()
            .with_psk_client(
                identity.as_bytes().to_vec(),
                resolver as Arc<dyn dimpl::PskResolver>,
            )
            .build()
            .expect("valid DTLS config"),
    )
}

/// Send a simple GET request and return the response status code.
async fn send_get_request(
    client: &mut DtlsClient,
    path: &str,
) -> Result<ResponseType, Box<dyn std::error::Error>> {
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.message.header.message_id = next_msg_id();
    request.set_method(RequestType::Get);
    request.set_path(path);

    let bytes = request.message.to_bytes()?;
    client.send(&bytes).await?;

    let data = tokio::time::timeout(
        Duration::from_secs(TIMEOUT_SECS),
        client.recv(Duration::from_secs(TIMEOUT_SECS)),
    )
    .await
    .map_err(|_| "recv timeout")?
    .map_err(|e| format!("recv error: {e}"))?;

    let packet = Packet::from_bytes(&data)?;
    match packet.header.code {
        MessageClass::Response(resp) => Ok(resp),
        _ => Err("unexpected response type".into()),
    }
}

/// Start a server with `serve_with_client_management` and return the
/// bound address plus the `ClientManager`.
async fn start_managed_server()
-> Result<(SocketAddr, coapum::ClientManager), Box<dyn std::error::Error>> {
    let listener = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    drop(listener);

    let mut initial_clients = HashMap::new();
    initial_clients.insert(INITIAL_IDENTITY.to_string(), INITIAL_PSK.to_vec());

    let router = RouterBuilder::new((), MemObserver::new())
        .get("/echo", echo_handler)
        .build();

    let config = Config::default().with_client_management(initial_clients);

    let (client_manager, server_future) =
        serve::serve_with_client_management(addr.to_string(), config, router).await?;

    tokio::spawn(async move {
        if let Err(e) = server_future.await {
            tracing::error!("Server error: {}", e);
        }
    });

    // Give the server time to start listening
    tokio::time::sleep(Duration::from_millis(200)).await;

    Ok((addr, client_manager))
}

/// Initial client can connect and make requests.
#[tokio::test]
async fn test_initial_client_can_connect() {
    let (addr, _client_manager) = start_managed_server().await.expect("server start");

    let dtls_config = build_dtls_config(INITIAL_IDENTITY, INITIAL_PSK);
    let mut client = DtlsClient::connect(&addr.to_string(), dtls_config)
        .await
        .expect("DTLS connect");

    let response = send_get_request(&mut client, "/echo")
        .await
        .expect("request succeeded");

    assert_eq!(response, ResponseType::Content);
}

/// ClientManager can add a new client, and that client can connect.
#[tokio::test]
async fn test_add_client_dynamic() {
    let (addr, client_manager) = start_managed_server().await.expect("server start");

    // Add a new client dynamically
    let new_identity = "dynamic_client_001";
    let new_psk = b"dynamic_psk_key_456";

    client_manager
        .add_client(new_identity, new_psk)
        .await
        .expect("add_client succeeded");

    // Give the credential store time to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect with the new credentials
    let dtls_config = build_dtls_config(new_identity, new_psk);
    let mut client = DtlsClient::connect(&addr.to_string(), dtls_config)
        .await
        .expect("DTLS connect with dynamic credentials");

    let response = send_get_request(&mut client, "/echo")
        .await
        .expect("request succeeded");

    assert_eq!(response, ResponseType::Content);
}

/// ClientManager can list all registered clients.
#[tokio::test]
async fn test_list_clients() {
    let (_addr, client_manager) = start_managed_server().await.expect("server start");

    // Initial client should be present
    let clients = client_manager
        .list_clients()
        .await
        .expect("list_clients succeeded");
    assert!(clients.contains(&INITIAL_IDENTITY.to_string()));

    // Add another client
    client_manager
        .add_client("client_b", b"psk_b")
        .await
        .expect("add_client succeeded");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let clients = client_manager
        .list_clients()
        .await
        .expect("list_clients succeeded");
    assert_eq!(clients.len(), 2);
    assert!(clients.contains(&INITIAL_IDENTITY.to_string()));
    assert!(clients.contains(&"client_b".to_string()));
}

/// ClientManager can update a client's key, and the old key no longer works.
#[tokio::test]
async fn test_update_key_invalidates_old() {
    let (addr, client_manager) = start_managed_server().await.expect("server start");

    // Update the initial client's key
    let new_psk = b"rotated_psk_key_789";
    client_manager
        .update_key(INITIAL_IDENTITY, new_psk)
        .await
        .expect("update_key succeeded");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Old key should fail (handshake won't complete)
    let old_config = build_dtls_config(INITIAL_IDENTITY, INITIAL_PSK);
    let result = DtlsClient::connect(&addr.to_string(), old_config).await;
    assert!(result.is_err(), "old key should be rejected");

    // New key should succeed
    let new_config = build_dtls_config(INITIAL_IDENTITY, new_psk);
    let mut client = DtlsClient::connect(&addr.to_string(), new_config)
        .await
        .expect("DTLS connect with rotated key");

    let response = send_get_request(&mut client, "/echo")
        .await
        .expect("request succeeded");

    assert_eq!(response, ResponseType::Content);
}

/// ClientManager can remove a client, and that client can no longer connect.
#[tokio::test]
async fn test_remove_client() {
    let (addr, client_manager) = start_managed_server().await.expect("server start");

    // Remove the initial client
    client_manager
        .remove_client(INITIAL_IDENTITY)
        .await
        .expect("remove_client succeeded");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Should no longer be able to connect
    let dtls_config = build_dtls_config(INITIAL_IDENTITY, INITIAL_PSK);
    let result = DtlsClient::connect(&addr.to_string(), dtls_config).await;
    assert!(result.is_err(), "removed client should be rejected");
}
