use std::{
    self,
    collections::HashMap,
    sync::{Arc, RwLock},
};

use coapum::{
    dtls::{
        cipher_suite::CipherSuiteId,
        config::{Config, ExtendedMasterSecretType},
        Error,
    },
    observer::sled::SledObserver,
    routing::RouterBuilder,
    serve, Raw,
};

type PskStore = Arc<RwLock<HashMap<String, Vec<u8>>>>;

const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6".as_bytes();

async fn test() -> Raw {
    let json = "{\"resp\":\"OK\"}";
    log::info!("Writing: {}", json);
    let json = json.as_bytes().to_vec();

    Raw {
        payload: json,
        content_format: None,
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    log::info!("Server!");

    // Set up store
    let psk_store: PskStore = Arc::new(RwLock::new(HashMap::new()));

    // Add entry
    {
        let mut psk_store_write = psk_store.write().unwrap();
        psk_store_write.insert("goobie!".to_string(), PSK.to_vec());
    }

    let obs = SledObserver::new("coapum.db");

    let router = RouterBuilder::new((), obs).get("test", test).build();

    // Setup socket
    let addr = "127.0.0.1:5684";
    let dtls_cfg = Config {
        psk: Some(Arc::new(move |hint: &[u8]| -> Result<Vec<u8>, Error> {
            let hint = String::from_utf8(hint.to_vec()).unwrap();

            log::info!("Client's hint: {}", hint);

            // Look up the hint in the database
            if let Some(psk) = psk_store.read().unwrap().get(&hint) {
                return Ok(psk.clone());
            } else {
                log::info!("Hint {} not found in store", hint);
                Err(Error::ErrIdentityNoPsk)
            }
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
