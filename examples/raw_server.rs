use std::collections::HashMap;

use coapum::{
    MemoryCredentialStore, Raw, observer::memory::MemObserver, router::RouterBuilder, serve,
};

const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6".as_bytes();

async fn test() -> Raw {
    let json = "{\"resp\":\"OK\"}";
    tracing::info!("Writing: {}", json);
    let json = json.as_bytes().to_vec();

    Raw {
        payload: json,
        content_format: None,
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Server!");

    // Set up PSK credentials
    let mut clients = HashMap::new();
    clients.insert("goobie!".to_string(), PSK.to_vec());
    let credential_store = MemoryCredentialStore::from_clients(&clients);

    let obs = MemObserver::new();
    let router = RouterBuilder::new((), obs).get("test", test).build();

    // Server config
    let addr = "127.0.0.1:5684";
    let cfg = coapum::config::Config {
        psk_identity_hint: Some(b"coapum server".to_vec()),
        ..Default::default()
    };

    let _ =
        serve::serve_with_credential_store(addr.to_string(), cfg, router, credential_store).await;
}
