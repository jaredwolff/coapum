use std::{self, net::SocketAddr, sync::Arc};

use coap_lite::{CoapRequest, CoapResponse, Packet};

use webrtc_dtls::{
    cipher_suite::CipherSuiteId,
    config::{Config, ExtendedMasterSecretType},
    Error,
};

use coapum::{
    router::{wrapper::get, CoapRouter, RouterError},
    serve,
};

const IDENTITY: &str = "goobie!";
const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6df70b6c5e97c220060e2ea122c4fdd054555827ab229457c366b2dd4817ff38b".as_bytes();

async fn get_foo(req: CoapRequest<SocketAddr>) -> Result<CoapResponse, RouterError> {
    log::info!("Request path: {}", req.get_path());
    log::info!(
        "Received: {}",
        String::from_utf8(req.message.payload).unwrap()
    );

    let pkt = Packet::default();
    let mut response = CoapResponse::new(&pkt).unwrap();
    response.message.payload = b"bar".to_vec();

    log::info!("Writing: bar");

    Ok(response)
}

#[tokio::main]
async fn main() {
    println!("Server!");

    env_logger::init();

    let mut router = CoapRouter::new();
    router.add_route("foo", get(get_foo));

    // Setup socket
    let addr = "127.0.0.1:5683";
    let cfg = Config {
        psk: Some(Arc::new(|hint: &[u8]| -> Result<Vec<u8>, Error> {
            println!(
                "Client's hint: {}",
                String::from_utf8(hint.to_vec()).unwrap()
            );

            if hint.eq(IDENTITY.as_bytes()) {
                Ok(PSK.to_vec())
            } else {
                Err(Error::ErrClientCertificateNotVerified)
            }
        })),
        psk_identity_hint: Some("coapum server".as_bytes().to_vec()),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Ccm_8],
        extended_master_secret: ExtendedMasterSecretType::Require,
        ..Default::default()
    };

    let _ = serve::serve(addr.to_string(), cfg, router).await;
}
