//! Integration tests for the ServerHandle / SessionHandle shutdown API.
//!
//! **Must run with `--test-threads=1`** to avoid port conflicts from the
//! bind-drop-rebind pattern used to discover free ports.

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use coapum::{
    MemoryCredentialStore, ServerHandle, bind_and_spawn, client::DtlsClient,
    config::Config as ServerConfig, credential::resolver::MapResolver,
    observer::memory::MemObserver, router::RouterBuilder, serve,
};

const PSK: &[u8] = b"test_psk_key_1234567890abcdef";
const SERVER_ADDR: &str = "127.0.0.1:0";

#[derive(Clone, Debug)]
struct EmptyState;

impl AsRef<EmptyState> for EmptyState {
    fn as_ref(&self) -> &EmptyState {
        self
    }
}

fn pick_port() -> SocketAddr {
    let listener = std::net::UdpSocket::bind(SERVER_ADDR).expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    drop(listener);
    addr
}

fn make_router() -> coapum::router::CoapRouter<MemObserver, EmptyState> {
    RouterBuilder::new(EmptyState, MemObserver::new()).build()
}

fn make_credential_store(identities: &[&str]) -> MemoryCredentialStore {
    let mut clients = HashMap::new();
    for id in identities {
        clients.insert(id.to_string(), PSK.to_vec());
    }
    MemoryCredentialStore::from_clients(&clients)
}

fn make_config() -> ServerConfig {
    ServerConfig {
        psk_identity_hint: Some(b"test_server".to_vec()),
        timeout: 30,
        ..Default::default()
    }
}

async fn spawn_server(identities: &[&str]) -> ServerHandle {
    let addr = pick_port();
    let store = make_credential_store(identities);
    bind_and_spawn(addr.to_string(), make_config(), make_router(), store)
        .await
        .expect("bind_and_spawn")
}

async fn connect_client(
    server_addr: SocketAddr,
    identity: &str,
) -> Result<DtlsClient, Box<dyn std::error::Error>> {
    let mut keys = HashMap::new();
    keys.insert(identity.to_string(), PSK.to_vec());
    let resolver = Arc::new(MapResolver::new(keys));
    let config = dimpl::Config::builder()
        .with_psk_client(
            identity.as_bytes().to_vec(),
            resolver as Arc<dyn dimpl::PskResolver>,
        )
        .build()
        .expect("valid DTLS config");
    DtlsClient::connect(&server_addr.to_string(), Arc::new(config)).await
}

/// Wait until the server's session count reaches `n`, polling at 50ms.
async fn wait_for_sessions(handle: &ServerHandle, n: usize, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if handle.active_session_count() >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "timed out waiting for {} sessions; saw {}",
        n,
        handle.active_session_count()
    );
}

#[tokio::test]
async fn cancel_token_stops_accept_keeps_sessions() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let handle = spawn_server(&["client_a"]).await;
    let server_addr = handle.local_addr();

    let _client = connect_client(server_addr, "client_a")
        .await
        .expect("connect");
    wait_for_sessions(&handle, 1, Duration::from_secs(5)).await;

    // Cancel — accept loop should stop, existing session should remain.
    handle.shutdown_token().cancel();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(
        handle.active_session_count(),
        1,
        "established session should survive accept-loop cancel"
    );

    // Drop client; close_all_graceful issues close_notify and drained() resolves.
    handle.close_all_graceful(Duration::ZERO).await;
    tokio::time::timeout(Duration::from_secs(5), handle.drained())
        .await
        .expect("drained should complete after close_all_graceful");
}

#[tokio::test]
async fn close_all_graceful_dispatches_within_jitter() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let handle = spawn_server(&["c1", "c2", "c3"]).await;
    let server_addr = handle.local_addr();

    let _c1 = connect_client(server_addr, "c1").await.expect("c1");
    let _c2 = connect_client(server_addr, "c2").await.expect("c2");
    let _c3 = connect_client(server_addr, "c3").await.expect("c3");
    wait_for_sessions(&handle, 3, Duration::from_secs(5)).await;

    let jitter = Duration::from_millis(200);
    let started = tokio::time::Instant::now();
    handle.shutdown_token().cancel();
    handle.close_all_graceful(jitter).await;
    tokio::time::timeout(Duration::from_secs(5), handle.drained())
        .await
        .expect("drained");
    let elapsed = started.elapsed();

    // Allow generous slack for cleanup work.
    assert!(
        elapsed < jitter + Duration::from_secs(5),
        "close_all_graceful + drained took {elapsed:?}, expected < {:?}",
        jitter + Duration::from_secs(5)
    );
}

#[tokio::test]
async fn close_graceful_against_dead_peer_returns_quickly() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let handle = spawn_server(&["dead_peer"]).await;
    let server_addr = handle.local_addr();

    let client = connect_client(server_addr, "dead_peer")
        .await
        .expect("connect");
    wait_for_sessions(&handle, 1, Duration::from_secs(5)).await;
    drop(client); // peer goes away silently — no close_notify

    // close_graceful should still return — Notify wakes once cleanup runs.
    let sessions = handle.sessions().await;
    assert_eq!(sessions.len(), 1);
    let session = sessions.into_iter().next().unwrap();

    handle.shutdown_token().cancel();
    tokio::time::timeout(Duration::from_secs(10), session.close_graceful())
        .await
        .expect("close_graceful should not hang on a silent peer");
}

#[tokio::test]
async fn drained_with_timeout_does_not_panic() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let handle = spawn_server(&[]).await;
    handle.shutdown_token().cancel();

    // Wrapping drained() in timeout must not panic when nothing is connected.
    tokio::time::timeout(Duration::from_secs(2), handle.drained())
        .await
        .expect("drained should complete on idle server");
    assert_eq!(handle.active_session_count(), 0);
}

#[tokio::test]
async fn back_compat_serve_with_credential_store_still_works() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let addr = pick_port();
    let store = make_credential_store(&["legacy_client"]);

    let (tx, rx) = tokio::sync::watch::channel(());
    let mut config = make_config();
    config.shutdown = Some(rx);

    let server_addr_str = addr.to_string();
    let join = tokio::spawn(async move {
        serve::serve_with_credential_store(server_addr_str, config, make_router(), store).await
    });

    // Give the server a moment to bind, then trigger legacy watch shutdown.
    tokio::time::sleep(Duration::from_millis(200)).await;
    drop(tx);

    let result = tokio::time::timeout(Duration::from_secs(5), join)
        .await
        .expect("legacy serve should exit on watch shutdown")
        .expect("server task panicked");
    assert!(
        result.is_ok(),
        "serve_with_credential_store returned {result:?}"
    );
}

#[tokio::test]
async fn config_shutdown_relays_into_token() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let addr = pick_port();
    let store = make_credential_store(&[]);

    let (tx, rx) = tokio::sync::watch::channel(());
    let mut config = make_config();
    config.shutdown = Some(rx);

    let handle = bind_and_spawn(addr.to_string(), config, make_router(), store)
        .await
        .expect("bind_and_spawn");

    // Watch shutdown should flip the cancel token.
    drop(tx);
    tokio::time::timeout(Duration::from_secs(2), handle.shutdown_token().cancelled())
        .await
        .expect("legacy watch should relay into the cancel token");

    handle.drained().await;
}

#[tokio::test]
async fn session_handle_id_and_psk_identity_are_stable() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();

    let handle = spawn_server(&["stable_id_42"]).await;
    let server_addr = handle.local_addr();

    let _client = connect_client(server_addr, "stable_id_42")
        .await
        .expect("connect");
    wait_for_sessions(&handle, 1, Duration::from_secs(5)).await;

    let sessions = handle.sessions().await;
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert_eq!(s.id().as_str(), "stable_id_42");
    assert_eq!(s.id().to_string(), "stable_id_42");
    assert_eq!(s.psk_identity(), b"stable_id_42");
    let _ = s.peer_addr(); // just ensure it's accessible

    handle.shutdown_token().cancel();
    handle.close_all_graceful(Duration::ZERO).await;
}
