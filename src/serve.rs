use std::sync::{Arc, Mutex};

use tower::Service;
use webrtc_dtls::{config::Config, listener};
use webrtc_util::conn::Listener;

use coap_lite::{CoapRequest, Packet};

use crate::router::CoapRouter;

const BUF_SIZE: usize = 8192;

pub async fn serve<S>(
    addr: String,
    config: Config,
    router: CoapRouter<S>,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Send + Sync + 'static, // The shared state needs to be Send and Sync to be shared across threads
{
    let listener = Arc::new(listener::listen(addr.clone(), config).await.unwrap());
    let router = Arc::new(Mutex::new(router));

    loop {
        if let Ok((conn, socket_addr)) = listener.accept().await {
            let r = router.clone();

            log::info!("Got a connection from: {}", socket_addr);

            // TODO: get the client ID
            // let client_id = conn.get_client_id().await.unwrap();

            tokio::spawn(async move {
                let mut b = vec![0u8; BUF_SIZE];

                loop {
                    match conn.recv(&mut b).await {
                        Ok(n) => {
                            let packet = Packet::from_bytes(&b[0..n]).unwrap();
                            let request = CoapRequest::from_packet(packet, socket_addr);

                            log::debug!("Got request: {:?}", request);

                            // Push it into the router
                            let fut = {
                                let mut r = r.lock().unwrap();
                                r.call(request)
                            };

                            // Get the response
                            let resp = fut.await.unwrap();
                            let bytes = resp.message.to_bytes().unwrap();

                            log::debug!("Got response: {:?}", resp.message);

                            // Write it back
                            match conn.send(&bytes).await {
                                Ok(n) => log::debug!("Wrote {} bytes", n),
                                Err(e) => {
                                    log::error!("Error: {}", e);
                                    break;
                                }
                            };
                        }
                        Err(e) => {
                            log::error!("Error: {}", e);
                            break;
                        }
                    }
                }

                conn.close().await.unwrap();
            });
        }
    }
}
