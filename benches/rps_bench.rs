use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use coapum::{
    CoapRequest, MemoryCredentialStore, Packet, Raw, RequestType, client::DtlsClient,
    config::Config as ServerConfig, credential::resolver::MapResolver,
    observer::memory::MemObserver, router::RouterBuilder, serve,
};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use tokio::sync::Mutex;

const PSK: &[u8] = b"bench_rps_key";

async fn ping() -> Raw {
    Raw {
        payload: b"pong".to_vec(),
        content_format: None,
    }
}

fn build_get_request(path: &str) -> Vec<u8> {
    let mut req: CoapRequest<SocketAddr> = CoapRequest::new();
    req.set_method(RequestType::Get);
    req.set_path(path);
    req.message.to_bytes().unwrap()
}

async fn connect_client(addr: &str, identity: &str) -> DtlsClient {
    let mut keys = HashMap::new();
    keys.insert(identity.to_string(), PSK.to_vec());
    let resolver = Arc::new(MapResolver::new(keys));
    let config = dimpl::Config::builder()
        .with_psk_resolver(resolver as Arc<dyn dimpl::PskResolver>)
        .with_psk_identity(identity.as_bytes().to_vec())
        .build()
        .expect("valid DTLS config");

    DtlsClient::connect(addr, Arc::new(config))
        .await
        .expect("DTLS handshake failed")
}

struct RpsFixture {
    clients: Vec<Arc<Mutex<DtlsClient>>>,
    request_bytes: Vec<u8>,
}

async fn setup_rps(num_clients: usize) -> RpsFixture {
    let observer = MemObserver::new();
    let router = RouterBuilder::new((), observer).get("/ping", ping).build();

    let listener = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let mut cred_clients = HashMap::new();
    for i in 0..num_clients {
        cred_clients.insert(format!("client-{}", i), PSK.to_vec());
    }
    let credential_store = MemoryCredentialStore::from_clients(&cred_clients);

    let server_config = ServerConfig {
        psk_identity_hint: Some(b"bench_rps".to_vec()),
        timeout: 30,
        max_connections: num_clients + 100,
        ..Default::default()
    };

    // Run server on a dedicated runtime/thread so it doesn't compete
    // with client tasks for tokio worker threads.
    let server_addr = addr.to_string();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let _ = serve::serve_with_credential_store(
                server_addr,
                server_config,
                router,
                credential_store,
            )
            .await;
        });
    });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect clients in parallel batches
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
                connect_client(&addr, &identity).await
            }));
        }
        for handle in handles {
            let client = handle.await.unwrap();
            clients.push(Arc::new(Mutex::new(client)));
        }
    }

    // Verify one client can get a response
    let req = build_get_request("/ping");
    {
        let mut c = clients[0].lock().await;
        c.send(&req).await.unwrap();
        let data = c.recv(Duration::from_secs(5)).await.unwrap();
        let packet = Packet::from_bytes(&data).unwrap();
        assert_eq!(packet.payload, b"pong");
    }

    RpsFixture {
        clients,
        request_bytes: req,
    }
}

/// Each client does sequential request/response in its own task.
/// All clients run concurrently. Measures aggregate RPS.
fn coap_rps(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("coap_rps");
    group.sample_size(10);

    let requests_per_client = 50;

    for &num_clients in &[1, 10, 50, 100, 150] {
        let fixture = rt.block_on(setup_rps(num_clients));
        let total_requests = (num_clients * requests_per_client) as u64;

        group.throughput(criterion::Throughput::Elements(total_requests));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_clients),
            &num_clients,
            |b, &_n| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = Vec::with_capacity(fixture.clients.len());
                        for client in &fixture.clients {
                            let client = client.clone();
                            let req = fixture.request_bytes.clone();
                            handles.push(tokio::spawn(async move {
                                let mut c = client.lock().await;
                                for _ in 0..requests_per_client {
                                    c.send(&req).await.unwrap();
                                    let data = c.recv(Duration::from_secs(5)).await.unwrap();
                                    let _packet = Packet::from_bytes(&data).unwrap();
                                }
                            }));
                        }
                        for handle in handles {
                            handle.await.unwrap();
                        }
                    });
                })
            },
        );
    }

    group.finish();
}

/// Measures latency percentiles under concurrent load.
/// Each client sends requests sequentially while all run in parallel.
fn coap_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("coap_latency");
    group.sample_size(10);

    let requests_per_client = 100;

    for &num_clients in &[1, 10, 50, 100] {
        let fixture = rt.block_on(setup_rps(num_clients));

        group.bench_with_input(
            BenchmarkId::from_parameter(num_clients),
            &num_clients,
            |b, &_n| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for _ in 0..iters {
                        let latencies = rt.block_on(async {
                            let mut handles = Vec::with_capacity(fixture.clients.len());
                            for client in &fixture.clients {
                                let client = client.clone();
                                let req = fixture.request_bytes.clone();
                                let n = requests_per_client;
                                handles.push(tokio::spawn(async move {
                                    let mut lats = Vec::with_capacity(n);
                                    let mut c = client.lock().await;
                                    for _ in 0..n {
                                        let start = tokio::time::Instant::now();
                                        c.send(&req).await.unwrap();
                                        let data = c.recv(Duration::from_secs(5)).await.unwrap();
                                        let _packet = Packet::from_bytes(&data).unwrap();
                                        lats.push(start.elapsed());
                                    }
                                    lats
                                }));
                            }

                            let mut all_latencies = Vec::new();
                            for handle in handles {
                                all_latencies.extend(handle.await.unwrap());
                            }
                            all_latencies
                        });

                        let mut sorted = latencies;
                        sorted.sort();
                        let len = sorted.len();
                        let p50 = sorted[len / 2];
                        let p95 = sorted[len * 95 / 100];
                        let p99 = sorted[len * 99 / 100];
                        let avg: Duration = sorted.iter().sum::<Duration>() / sorted.len() as u32;

                        // Print on first iteration only
                        if total == Duration::ZERO {
                            eprintln!(
                                "  [{} clients × {} reqs] avg={:?} p50={:?} p95={:?} p99={:?}",
                                num_clients, requests_per_client, avg, p50, p95, p99
                            );
                        }

                        total += sorted.iter().sum::<Duration>();
                    }

                    total / (num_clients as u32 * requests_per_client as u32)
                })
            },
        );
    }

    group.finish();
}

/// Sustained throughput: N clients send as fast as possible for a fixed duration.
/// Reports total completed requests/sec — the "wrk" style benchmark.
fn coap_sustained(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("coap_sustained");
    group.sample_size(10);

    for &num_clients in &[1, 10, 50, 100, 250, 500, 1000] {
        let fixture = rt.block_on(setup_rps(num_clients));

        group.bench_with_input(
            BenchmarkId::from_parameter(num_clients),
            &num_clients,
            |b, &_n| {
                b.iter_custom(|iters| {
                    let mut total_elapsed = Duration::ZERO;

                    for _ in 0..iters {
                        let (elapsed, completed) = rt.block_on(async {
                            let running = Arc::new(AtomicBool::new(true));
                            let completed = Arc::new(AtomicU64::new(0));
                            let duration = Duration::from_millis(500);

                            let mut handles = Vec::with_capacity(fixture.clients.len());
                            let start = tokio::time::Instant::now();

                            for client in &fixture.clients {
                                let client = client.clone();
                                let req = fixture.request_bytes.clone();
                                let running = running.clone();
                                let completed = completed.clone();
                                handles.push(tokio::spawn(async move {
                                    let mut c = client.lock().await;
                                    while running.load(Ordering::Relaxed) {
                                        if c.send(&req).await.is_err() {
                                            break;
                                        }
                                        if c.recv(Duration::from_millis(500)).await.is_ok() {
                                            completed.fetch_add(1, Ordering::Relaxed);
                                        }
                                    }
                                }));
                            }

                            tokio::time::sleep(duration).await;
                            running.store(false, Ordering::Relaxed);

                            for handle in handles {
                                let _ = handle.await;
                            }

                            let elapsed = start.elapsed();
                            let count = completed.load(Ordering::Relaxed);

                            if total_elapsed == Duration::ZERO {
                                let rps = count as f64 / elapsed.as_secs_f64();
                                eprintln!(
                                    "  [{} clients] {:.0} req/s ({} reqs in {:?})",
                                    num_clients, rps, count, elapsed
                                );
                            }

                            (elapsed, count)
                        });

                        // Report as if each request took elapsed/completed
                        if completed > 0 {
                            total_elapsed += elapsed / completed as u32;
                        }
                    }

                    total_elapsed / iters as u32
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, coap_rps, coap_latency, coap_sustained);
criterion_main!(benches);
