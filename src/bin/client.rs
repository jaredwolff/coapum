use std::time::Duration;

use openssl::ssl::{SslContext, SslMethod};
use tokio::net::UdpSocket;
use tokio_dtls_stream_sink::Client as DtlsClient;

const IDENTITY: &[u8] = "goobie!".as_bytes();
const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6df70b6c5e97c220060e2ea122c4fdd054555827ab229457c366b2dd4817ff38b".as_bytes();

#[tokio::main]
async fn main() {
    println!("Client!");

    env_logger::init();

    // Setup socket
    let addr = "127.0.0.1:0";
    let saddr = "127.0.0.1:5683";
    let client = UdpSocket::bind(addr).await.unwrap();

    // Setup SSL context for PSK
    let mut ctx = SslContext::builder(SslMethod::dtls()).unwrap();
    ctx.set_psk_client_callback(move |_ssl, _int, identity_mut, secret_mut| {
        let secret_size = std::cmp::min(PSK.len(), secret_mut.len());
        let identity_size = std::cmp::min(IDENTITY.len(), identity_mut.len());

        // TODO: calculate hash of secret instead of raw one

        // Copy over secret and identity
        secret_mut[..secret_size].copy_from_slice(&PSK[..secret_size]);
        identity_mut[..identity_size].copy_from_slice(&IDENTITY[..identity_size]);

        log::info!(
            "Setting identity and secret. Identity: {} Secret: {}",
            identity_size,
            secret_size
        );

        Ok(secret_size)
    });
    let ctx = ctx.build();

    log::info!("Connecting..");

    let client = DtlsClient::new(client);
    let mut session = client.connect(saddr, Some(ctx)).await.unwrap();

    loop {
        log::info!("Writing norf");

        session.write(b"norf3").await.unwrap();

        tokio::time::sleep(Duration::from_secs(1)).await;

        // TODO: check if session is still good
    }
}
