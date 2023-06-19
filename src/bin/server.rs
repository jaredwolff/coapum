use std::{self, net::SocketAddr, sync::Arc};

use webrtc_dtls::{
    cipher_suite::CipherSuiteId,
    config::{Config, ExtendedMasterSecretType},
    Error,
};

use coapum::{
    router::{wrapper::get, CoapRouter, CoapumRequest, RouterError},
    serve, {CoapResponse, Packet, ResponseType},
};

const IDENTITY: &str = "goobie!";
const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6df70b6c5e97c220060e2ea122c4fdd054555827ab229457c366b2dd4817ff38b".as_bytes();

async fn test<S>(r: CoapumRequest<SocketAddr>, _state: S) -> Result<CoapResponse, RouterError> {
    log::info!(
        "Got request: {} from: {}",
        String::from_utf8(r.message.payload).unwrap(),
        String::from_utf8(r.identity).unwrap()
    );

    let pkt = Packet::default();
    let mut response = CoapResponse::new(&pkt).unwrap();
    let json = "{\"resp\":\"OK\"}";
    response.message.payload = json.as_bytes().to_vec();
    response.set_status(ResponseType::Valid);

    log::info!("Writing: {}", json);
    Ok(response)
}

#[tokio::main]
async fn main() {
    env_logger::init();

    log::info!("Server!");

    let mut router = CoapRouter::new();
    router.add("test", get(test));

    // Setup socket
    let addr = "127.0.0.1:5683";
    let cfg = Config {
        psk: Some(Arc::new(|hint: &[u8]| -> Result<Vec<u8>, Error> {
            // TODO: actually look this up somewhere

            log::info!(
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
