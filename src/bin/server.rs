use std::{self, sync::Arc};

use webrtc_dtls::{
    cipher_suite::CipherSuiteId,
    config::{Config, ExtendedMasterSecretType},
    Error,
};

use coapum::{
    observer::sled::SledObserver,
    router::wrapper::{CoapResponseResult, IntoCoapResponse},
};

use coapum::{
    router::{wrapper::get, CoapRouter, Request},
    serve, ResponseType,
};

const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6".as_bytes();

async fn test<S>(payload: Box<dyn Request>, _state: S) -> CoapResponseResult {
    log::info!("Got json payload: {}", payload.get_value());
    let json = "{\"resp\":\"OK\"}";
    log::info!("Writing: {}", json);
    let json = json.as_bytes().to_vec();

    (ResponseType::Valid, json).into_response()
}

#[tokio::main]
async fn main() {
    env_logger::init();

    log::info!("Server!");

    let obs = SledObserver::new("coapum.db");

    let mut router = CoapRouter::new((), obs);
    router.add("test", get(test));

    // Setup socket
    let addr = "127.0.0.1:5684";
    let dtls_cfg = Config {
        psk: Some(Arc::new(|hint: &[u8]| -> Result<Vec<u8>, Error> {
            log::info!(
                "Client's hint: {}",
                String::from_utf8(hint.to_vec()).unwrap()
            );

            Ok(PSK.to_vec())
        })),
        psk_identity_hint: Some("coapum server".as_bytes().to_vec()),
        cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
        extended_master_secret: ExtendedMasterSecretType::Require,
        ..Default::default()
    };

    // Server config
    let cfg = coapum::config::Config {
        dtls_cfg,
        ..Default::default()
    };

    let _ = serve::serve(addr.to_string(), cfg, router).await;
}
