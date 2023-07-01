use std::{net::SocketAddr, sync::Arc};

use tokio::net::UdpSocket;
use webrtc_dtls::{
    cipher_suite::CipherSuiteId,
    config::{Config, ExtendedMasterSecretType},
    conn::DTLSConn,
    Error,
};
use webrtc_util::Conn;

use coapum::{CoapRequest, ContentFormat, Packet, RequestType};

// const IDENTITY: &[u8] = "goobie!".as_bytes();
const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6".as_bytes();

const CONCURRENCY: usize = 100; // The number of simultaneous clients
const REQUESTS: usize = 1000; // The number of requests each client will send

#[tokio::main]
async fn main() {
    env_logger::init();

    log::info!("Client!");

    // Setup socket
    let addr = "127.0.0.1:0";
    let saddr: &str = "127.0.0.1:5684";
    let mut handles = Vec::new();

    for (identity_count, _) in (0..CONCURRENCY).enumerate() {
        let handle = tokio::spawn(async move {
            let conn = Arc::new(UdpSocket::bind(addr).await.unwrap());
            conn.connect(saddr).await.unwrap();

            let identity = format!("goobie-{}", identity_count);

            // Setup SSL context for PSK
            let config = Config {
                psk: Some(Arc::new(|hint: &[u8]| -> Result<Vec<u8>, Error> {
                    log::info!(
                        "Server's hint: {}",
                        String::from_utf8(hint.to_vec()).unwrap()
                    );
                    Ok(PSK.to_vec())
                })),
                psk_identity_hint: Some(identity.as_bytes().to_vec()),
                cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
                extended_master_secret: ExtendedMasterSecretType::Require,
                ..Default::default()
            };

            let dtls_conn: Arc<dyn Conn + Send + Sync> = Arc::new(
                DTLSConn::new(conn, config.clone(), true, None)
                    .await
                    .unwrap(),
            );

            let mut b = vec![0u8; 1024];
            let payload_json = "{\"foo\": {\"bar\": 1, \"baz\": [1, 2, 3]}}";

            for _ in 0..REQUESTS {
                log::info!("Writing: {}", payload_json);

                let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
                request.set_method(RequestType::Get);
                request.set_path("test");
                request.message.payload = payload_json.as_bytes().to_vec();
                request
                    .message
                    .set_content_format(ContentFormat::ApplicationJSON);
                match dtls_conn.send(&request.message.to_bytes().unwrap()).await {
                    Ok(n) => {
                        log::info!("Wrote {} bytes", n);
                    }
                    Err(e) => {
                        log::error!("Error writing: {}", e);
                        break;
                    }
                };

                if let Ok(n) = dtls_conn.recv(&mut b).await {
                    log::debug!("Read {} bytes", n);

                    let packet = Packet::from_bytes(&b[0..n]).unwrap();

                    log::info!("Response: {:?}", String::from_utf8(packet.payload).unwrap());
                    log::info!("Status: {:?}", packet.header.code);
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
