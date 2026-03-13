use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use coapum::{
    CoapRequest, ContentFormat, MemoryCredentialStore, Packet, Raw, RequestType,
    client::DtlsClient, observer::memory::MemObserver, router::RouterBuilder, serve,
};

const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6".as_bytes();

const CONCURRENCY: usize = 100; // The number of simultaneous clients
const REQUESTS: usize = 1000; // The number of requests each client will send

async fn echo() -> Raw {
    Raw {
        payload: b"{\"resp\":\"OK\"}".to_vec(),
        content_format: None,
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Register all client identities with the server's credential store
    let mut clients = HashMap::new();
    for i in 0..CONCURRENCY {
        clients.insert(format!("goobie-{}", i), PSK.to_vec());
    }
    let credential_store = MemoryCredentialStore::from_clients(&clients);

    let obs = MemObserver::new();
    let router = RouterBuilder::new((), obs).get("test", echo).build();

    // Bind to a free port
    let listener = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind failed");
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let saddr = addr.to_string();
    let cfg = coapum::config::Config {
        psk_identity_hint: Some(b"coapum concurrency".to_vec()),
        ..Default::default()
    };

    // Start the server
    let server_addr = saddr.clone();
    tokio::spawn(async move {
        if let Err(e) =
            serve::serve_with_credential_store(server_addr, cfg, router, credential_store).await
        {
            tracing::error!("Server error: {}", e);
        }
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!(
        "Starting {} clients with {} requests each against {}",
        CONCURRENCY,
        REQUESTS,
        saddr
    );

    let mut handles = Vec::new();

    for identity_count in 0..CONCURRENCY {
        let saddr = saddr.clone();
        let handle = tokio::spawn(async move {
            let identity = format!("goobie-{}", identity_count);

            // Build dimpl config for this client
            let mut keys = HashMap::new();
            keys.insert(identity.clone(), PSK.to_vec());

            let resolver = Arc::new(coapum::credential::resolver::MapResolver::new(keys));

            let config = dimpl::Config::builder()
                .with_psk_client(
                    identity.as_bytes().to_vec(),
                    resolver as Arc<dyn dimpl::PskResolver>,
                )
                .build()
                .expect("valid DTLS config");

            let mut client = DtlsClient::connect(&saddr, Arc::new(config))
                .await
                .expect("DTLS handshake failed");

            let payload_json = "{\"foo\": {\"bar\": 1, \"baz\": [1, 2, 3]}}";

            for _ in 0..REQUESTS {
                tracing::info!("Writing: {}", payload_json);

                let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
                request.set_method(RequestType::Get);
                request.set_path("test");
                request.message.payload = payload_json.as_bytes().to_vec();
                request
                    .message
                    .set_content_format(ContentFormat::ApplicationJSON);

                match client.send(&request.message.to_bytes().unwrap()).await {
                    Ok(()) => {
                        tracing::info!("Sent request");
                    }
                    Err(e) => {
                        tracing::error!("Error writing: {}", e);
                        break;
                    }
                };

                match client.recv(Duration::from_secs(5)).await {
                    Ok(data) => {
                        let packet = Packet::from_bytes(&data).unwrap();
                        tracing::info!(
                            "Response: {:?}",
                            String::from_utf8(packet.payload).unwrap()
                        );
                        tracing::info!("Status: {:?}", packet.header.code);
                    }
                    Err(e) => {
                        tracing::error!("Error reading: {}", e);
                        break;
                    }
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all clients to finish
    for task in handles {
        task.await.unwrap();
    }
}
