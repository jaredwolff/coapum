use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use coapum::{CoapRequest, ContentFormat, Packet, RequestType, client::DtlsClient};

const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6".as_bytes();

const CONCURRENCY: usize = 100; // The number of simultaneous clients
const REQUESTS: usize = 1000; // The number of requests each client will send

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Client!");

    let saddr = "127.0.0.1:5684";
    let mut handles = Vec::new();

    for identity_count in 0..CONCURRENCY {
        let handle = tokio::spawn(async move {
            let identity = format!("goobie-{}", identity_count);

            // Build dimpl config for this client
            let mut keys = HashMap::new();
            keys.insert(identity.clone(), PSK.to_vec());

            let resolver = Arc::new(coapum::credential::resolver::MapResolver::new(keys));

            let config = dimpl::Config::builder()
                .with_psk_resolver(resolver as Arc<dyn dimpl::PskResolver>)
                .with_psk_identity(identity.as_bytes().to_vec())
                .build()
                .expect("valid DTLS config");

            let mut client = DtlsClient::connect(saddr, Arc::new(config))
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
