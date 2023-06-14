use std::{sync::Arc, time::Duration};

use webrtc_dtls::{
    cipher_suite::CipherSuiteId,
    config::{Config, ExtendedMasterSecretType},
    listener, Error,
};
use webrtc_util::conn::Listener;

const IDENTITY: &str = "goobie!";
const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6df70b6c5e97c220060e2ea122c4fdd054555827ab229457c366b2dd4817ff38b".as_bytes();
const BUF_SIZE: usize = 8192;

#[tokio::main]
async fn main() {
    println!("Server!");

    env_logger::init();

    // Setup socket
    let addr = "127.0.0.1:5683";
    let cfg = Config {
        psk: Some(Arc::new(|hint: &[u8]| -> Result<Vec<u8>, Error> {
            println!(
                "Client's hint: {}",
                String::from_utf8(hint.to_vec()).unwrap()
            );
            Ok(PSK.to_vec())
        })),
        psk_identity_hint: Some("webrtc-rs DTLS server".as_bytes().to_vec()),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8],
        extended_master_secret: ExtendedMasterSecretType::Require,
        ..Default::default()
    };

    let listener = Arc::new(listener::listen(addr, cfg).await.unwrap());

    while let Ok((conn, _remote_addr)) = listener.accept().await {
        tokio::spawn(async move {
            let mut b = vec![0u8; BUF_SIZE];

            loop {
                match conn.recv(&mut b).await {
                    Ok(n) => {
                        let msg = String::from_utf8(b[..n].to_vec()).unwrap();
                        log::info!("Got message: {msg}");
                    }
                    Err(e) => {
                        log::error!("Error: {}", e);
                        break;
                    }
                }
            }
        });
    }

    listener.close().await.unwrap();
}
