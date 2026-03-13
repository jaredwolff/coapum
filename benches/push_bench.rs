use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use coapum::{
    CoapRequest, MemoryCredentialStore, NotificationTrigger, Packet, Raw, RequestType,
    client::DtlsClient,
    config::Config as ServerConfig,
    credential::resolver::MapResolver,
    extract::{Path, State},
    observer::memory::MemObserver,
    router::RouterBuilder,
    serve,
};

use coap_lite::ObserveOption;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use serde_json::json;
use tokio::sync::Mutex;

const PSK: &[u8] = b"bench_push_notification_key";
const IDENTITY: &str = "bench_client";

#[derive(Clone, Debug)]
struct BenchState {
    value: Arc<Mutex<f64>>,
}

impl AsRef<BenchState> for BenchState {
    fn as_ref(&self) -> &BenchState {
        self
    }
}

async fn get_value(Path(_id): Path<String>, State(state): State<BenchState>) -> Raw {
    let v = state.value.lock().await;
    Raw {
        payload: format!("{}", *v).into_bytes(),
        content_format: None,
    }
}

async fn notify_value(Path(id): Path<String>, State(state): State<BenchState>) -> Raw {
    get_value(Path(id), State(state)).await
}

// --- Single-client latency benchmark ---

struct BenchFixture {
    client: DtlsClient,
    trigger: NotificationTrigger<MemObserver>,
}

async fn connect_client(addr: &str, identity: &str) -> DtlsClient {
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

    DtlsClient::connect(addr, Arc::new(config))
        .await
        .expect("DTLS handshake failed")
}

async fn register_observer(client: &mut DtlsClient, path: &str) {
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.set_method(RequestType::Get);
    request.set_path(path);
    request.set_observe_flag(ObserveOption::Register);
    client
        .send(&request.message.to_bytes().unwrap())
        .await
        .unwrap();

    let data = client.recv(Duration::from_secs(5)).await.unwrap();
    let packet = Packet::from_bytes(&data).unwrap();
    assert!(
        packet.get_observe_value().is_some(),
        "Initial observe response should have observe option"
    );
}

async fn setup_single() -> BenchFixture {
    let state = BenchState {
        value: Arc::new(Mutex::new(0.0)),
    };
    let observer = MemObserver::new();

    let router_builder = RouterBuilder::new(state, observer);
    let trigger = router_builder.notification_trigger();
    let router = router_builder
        .get("/sensor/:id", get_value)
        .observe("/sensor/:id", get_value, notify_value)
        .build();

    let listener = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let mut clients = HashMap::new();
    clients.insert(IDENTITY.to_string(), PSK.to_vec());
    let credential_store = MemoryCredentialStore::from_clients(&clients);

    let server_config = ServerConfig {
        psk_identity_hint: Some(b"bench_server".to_vec()),
        timeout: 30,
        ..Default::default()
    };

    let server_addr = addr.to_string();
    tokio::spawn(async move {
        let _ = serve::serve_with_credential_store(
            server_addr,
            server_config,
            router,
            credential_store,
        )
        .await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut client = connect_client(&addr.to_string(), IDENTITY).await;
    register_observer(&mut client, "/sensor/bench1").await;

    BenchFixture { client, trigger }
}

fn push_notification_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut fixture = rt.block_on(setup_single());

    let mut counter = 0u64;

    c.bench_function("push_notification_e2e", |b| {
        b.iter(|| {
            counter += 1;
            let payload = json!({"value": counter as f64});

            rt.block_on(async {
                fixture
                    .trigger
                    .trigger_notification(IDENTITY, "/sensor/bench1", &payload)
                    .await
                    .unwrap();

                let data = fixture.client.recv(Duration::from_secs(5)).await.unwrap();

                let _packet = Packet::from_bytes(&data).unwrap();
            });
        })
    });
}

// --- Multi-client throughput benchmark ---

struct ThroughputFixture {
    clients: Vec<(String, DtlsClient)>, // (identity, client)
    trigger: NotificationTrigger<MemObserver>,
}

async fn setup_throughput(num_clients: usize) -> ThroughputFixture {
    let state = BenchState {
        value: Arc::new(Mutex::new(0.0)),
    };
    let observer = MemObserver::new();

    let router_builder = RouterBuilder::new(state, observer);
    let trigger = router_builder.notification_trigger();
    let router = router_builder
        .get("/sensor/:id", get_value)
        .observe("/sensor/:id", get_value, notify_value)
        .build();

    let listener = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    // Register all client identities
    let mut cred_clients = HashMap::new();
    for i in 0..num_clients {
        cred_clients.insert(format!("client-{}", i), PSK.to_vec());
    }
    let credential_store = MemoryCredentialStore::from_clients(&cred_clients);

    let server_config = ServerConfig {
        psk_identity_hint: Some(b"bench_server".to_vec()),
        timeout: 30,
        max_connections: num_clients + 100,
        ..Default::default()
    };

    let server_addr = addr.to_string();
    tokio::spawn(async move {
        let _ = serve::serve_with_credential_store(
            server_addr,
            server_config,
            router,
            credential_store,
        )
        .await;
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect clients in parallel batches to avoid handshake timeouts
    let addr_str = addr.to_string();
    let mut clients = Vec::with_capacity(num_clients);
    let batch_size = 50;
    for batch_start in (0..num_clients).step_by(batch_size) {
        let batch_end = (batch_start + batch_size).min(num_clients);
        let mut handles = Vec::new();
        for i in batch_start..batch_end {
            let addr = addr_str.clone();
            let identity = format!("client-{}", i);
            handles.push(tokio::spawn(async move {
                let client = connect_client(&addr, &identity).await;
                (identity, client)
            }));
        }
        for handle in handles {
            let (identity, mut client) = handle.await.unwrap();
            let path = format!("/sensor/{}", identity.strip_prefix("client-").unwrap());
            register_observer(&mut client, &path).await;
            clients.push((identity, client));
        }
    }

    ThroughputFixture { clients, trigger }
}

fn push_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("push_throughput");

    group.sample_size(10);
    for &num_clients in &[1, 10, 50, 100, 250, 500, 1000, 2000, 5000] {
        let mut fixture = rt.block_on(setup_throughput(num_clients));
        let mut counter = 0u64;

        group.throughput(criterion::Throughput::Elements(num_clients as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_clients),
            &num_clients,
            |b, &_n| {
                b.iter(|| {
                    counter += 1;
                    let payload = json!({"value": counter as f64});

                    rt.block_on(async {
                        // Fire all notifications
                        for (identity, _) in &fixture.clients {
                            let path =
                                format!("/sensor/{}", identity.strip_prefix("client-").unwrap());
                            fixture
                                .trigger
                                .trigger_notification(identity, &path, &payload)
                                .await
                                .unwrap();
                        }

                        // Receive all notifications
                        for (_, client) in &mut fixture.clients {
                            let data = client.recv(Duration::from_secs(5)).await.unwrap();
                            let _packet = Packet::from_bytes(&data).unwrap();
                        }
                    });
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, push_notification_latency, push_throughput);
criterion_main!(benches);
