//! Layered server example.
//!
//! Demonstrates applying `tower::Layer`s to a `CoapRouter` using
//! `RouterBuilder::layer`. Three built-in middleware layers are stacked:
//!
//! - `TraceLayer` — emits a tracing span per call (both paths)
//! - `TimeoutLayer` — returns 5.04 GatewayTimeout after 5 seconds (both paths)
//! - `MapResponseLayer` — appends a payload note to request responses (request path only)
//!
//! `MapResponseLayer` is applied via `layer_request_only` because its closure is
//! typed to `CoapumRequest` — a single concrete closure cannot simultaneously
//! satisfy both the request and notification `Req` types.
//!
//! Run with:
//! ```not_rust
//! RUST_LOG=info cargo run --example layered_server
//! ```

use std::{collections::HashMap, net::SocketAddr, time::Duration};

use coapum::{
    CoapResponse, CoapumRequest, MemoryCredentialStore,
    extract::StatusCode,
    middleware::{MapResponseLayer, TimeoutLayer, TraceLayer},
    observer::memory::MemObserver,
    router::RouterBuilder,
    serve,
};

const PSK: &[u8] = b"63ef2024b1de6417f856fab7005d38f6";

async fn hello() -> StatusCode {
    tracing::info!("hello handler called");
    StatusCode::Content
}

async fn slow() -> StatusCode {
    tracing::info!("slow handler: sleeping 10 s");
    tokio::time::sleep(Duration::from_secs(10)).await;
    StatusCode::Content
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("layered_server starting");

    let mut clients = HashMap::new();
    clients.insert("device1".to_string(), PSK.to_vec());
    let credential_store = MemoryCredentialStore::from_clients(&clients);

    let router = RouterBuilder::new((), MemObserver::new())
        .get("hello", hello)
        .get("slow", slow)
        // TraceLayer — outermost on both paths, runs first
        .layer(TraceLayer::new())
        // TimeoutLayer — 5-second deadline on both paths
        .layer(TimeoutLayer::new(Duration::from_secs(5)))
        // MapResponseLayer — tags request responses with a payload note (request path only)
        .layer_request_only(MapResponseLayer::new(
            |_req: &CoapumRequest<SocketAddr>, resp: &mut CoapResponse| {
                if resp.message.payload.is_empty() {
                    resp.message.payload = b"layered".to_vec();
                }
            },
        ));

    let addr = "127.0.0.1:5684";
    let cfg = coapum::config::Config {
        psk_identity_hint: Some(b"coapum layered example".to_vec()),
        ..Default::default()
    };

    tracing::info!("Listening on {}", addr);
    let _ =
        serve::serve_with_credential_store(addr.to_string(), cfg, router, credential_store).await;
}
