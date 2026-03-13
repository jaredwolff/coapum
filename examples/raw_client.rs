use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use coapum::{CoapRequest, ContentFormat, Packet, RequestType, client::DtlsClient};

const IDENTITY: &str = "goobie!";
const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6".as_bytes();

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Client!");

    // Build dimpl config for PSK client
    let mut keys = HashMap::new();
    keys.insert(IDENTITY.to_string(), PSK.to_vec());

    let resolver = Arc::new(coapum::credential::resolver::MapResolver::new(keys));

    let config = dimpl::Config::builder()
        .with_psk_client(
            IDENTITY.as_bytes().to_vec(),
            resolver as Arc<dyn dimpl::PskResolver>,
        )
        .build()
        .expect("valid DTLS config");

    let saddr = "127.0.0.1:5684";
    let mut client = DtlsClient::connect(saddr, Arc::new(config))
        .await
        .expect("DTLS handshake failed");

    let payload_json = "{\"foo\": {\"bar\": 1, \"baz\": [1, 2, 3]}}";
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
            return;
        }
    };

    match client.recv(Duration::from_secs(5)).await {
        Ok(data) => {
            let packet = Packet::from_bytes(&data).unwrap();
            tracing::info!("Response: {:?}", String::from_utf8(packet.payload).unwrap());
            tracing::info!("Status: {:?}", packet.header.code);
        }
        Err(e) => {
            tracing::error!("Error reading: {}", e);
        }
    }
}
