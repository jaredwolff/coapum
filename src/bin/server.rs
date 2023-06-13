use std::time::Duration;

use futures_util::StreamExt;
use openssl::ssl::{SslContext, SslMethod};
use tokio::net::UdpSocket;
use tokio_dtls_stream_sink::Server as DtlsServer;

const IDENTITY: &str = "goobie!";
const PSK: &[u8] = "63ef2024b1de6417f856fab7005d38f6df70b6c5e97c220060e2ea122c4fdd054555827ab229457c366b2dd4817ff38b".as_bytes();

lazy_static::lazy_static! {
    static ref RESOURCE_INDEX: openssl::ex_data::Index<openssl::ssl::Ssl, String> =
        openssl::ssl::Ssl::new_ex_index().unwrap();
}

#[tokio::main]
async fn main() {
    println!("Server!");

    env_logger::init();

    // Setup socket
    let addr = "127.0.0.1:5683";
    let server = UdpSocket::bind(&addr).await.unwrap();

    // Setup SSL context for PSK
    let mut ctx = SslContext::builder(SslMethod::dtls()).unwrap();
    ctx.set_psk_server_callback(move |ssl, identity, secret_mut| {
        let mut to_copy = 0;
        if let Some(Ok(identity)) = identity.map(|s| core::str::from_utf8(s)) {
            log::info!("PSK auth for {:?}", identity);

            // TODO: psk lookup

            // Check if we have the appropriate identity
            if identity.eq(IDENTITY) {
                // TODO: calculate hash of secret instead of raw one

                // Set the psk in the copy
                to_copy = std::cmp::min(PSK.len(), secret_mut.len());
                secret_mut[..to_copy].copy_from_slice(&PSK[..to_copy]);

                // Set the identity
                // TODO: what does this do exactly?
                ssl.set_ex_data(*RESOURCE_INDEX, identity.to_string());
            }
        }

        log::info!("Copying {} bytes", to_copy);

        Ok(to_copy)
    });
    let ctx = ctx.build();

    log::info!("context built!");

    // Setup server and accept connections
    let mut server = DtlsServer::new(server);
    loop {
        // if let Err(e) = tokio::time::timeout(opts.connect_timeout, connect).await? {
        //     debug!("DTLS connect error: {:?}", e);
        //     return Err(Error::new(ErrorKind::Other, "DTLS connect failed"));
        // };

        match server.accept(Some(&ctx)).await {
            Ok(mut session) => {
                // TODO: split the session

                // Task for handling reading
                tokio::spawn(async move {
                    // let name: String = match session.ssl() {
                    //     Some(ssl) => {
                    //         if let Some(Ok(s)) = ssl.psk_identity().map(|s| core::str::from_utf8(s))
                    //         {
                    //             s.to_string()
                    //         } else {
                    //             "unknown".to_string()
                    //         }
                    //     }
                    //     None => session.peer().to_string(),
                    // };
                    let name = session.peer().to_string();

                    loop {
                        let mut rx = [0; 2048];

                        // TODO: otherwise read/write
                        let read = session.read(&mut rx[..]);
                        let res = match tokio::time::timeout(Duration::from_secs(10), read).await {
                            Ok(r) => r,
                            Err(_e) => {
                                log::warn!("Session timeout for: {:?}", name);
                                break;
                            }
                        };

                        match res {
                            Ok(len) => {
                                let _ = match std::str::from_utf8(&rx) {
                                    Ok(v) => log::info!("len: {} payload: {}", len, v),
                                    Err(e) => log::error!("Invalid UTF-8 sequence: {}", e),
                                };
                            }
                            Err(e) => {
                                log::error!("Error while receiving data: {:?}", e);
                                break;
                            }
                        }
                    }

                    log::warn!("Cleaning up {:?}", name);
                });

                // TODO: task for handling writes
            }
            Err(e) => {
                log::warn!("Error when accepting session: {:?}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
