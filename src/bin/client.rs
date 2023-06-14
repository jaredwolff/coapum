use std::{sync::Arc, time::Duration};

use tokio::net::UdpSocket;
use webrtc_dtls::{
    cipher_suite::CipherSuiteId,
    config::{Config, ExtendedMasterSecretType},
    conn::DTLSConn,
    Error,
};
use webrtc_util::Conn;

const IDENTITY: &[u8] = "goobie!".as_bytes();
const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6df70b6c5e97c220060e2ea122c4fdd054555827ab229457c366b2dd4817ff38b".as_bytes();

#[tokio::main]
async fn main() {
    println!("Client!");

    env_logger::init();

    // Setup socket
    let addr = "127.0.0.1:0";
    let saddr = "127.0.0.1:5683";

    let conn = Arc::new(UdpSocket::bind(addr).await.unwrap());
    conn.connect(saddr).await.unwrap();

    // Setup SSL context for PSK
    let config = Config {
        psk: Some(Arc::new(|hint: &[u8]| -> Result<Vec<u8>, Error> {
            println!(
                "Server's hint: {}",
                String::from_utf8(hint.to_vec()).unwrap()
            );
            Ok(PSK.to_vec())
        })),
        psk_identity_hint: Some(IDENTITY.to_vec()),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8],
        extended_master_secret: ExtendedMasterSecretType::Require,
        ..Default::default()
    };
    let dtls_conn: Arc<dyn Conn + Send + Sync> =
        Arc::new(DTLSConn::new(conn, config, true, None).await.unwrap());

    loop {
        log::info!("Writing norf");

        dtls_conn.send(b"norf3").await.unwrap();

        tokio::time::sleep(Duration::from_secs(1)).await;

        // TODO: check if session is still good
    }
}
