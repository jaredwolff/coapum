use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tower::Service;
use webrtc_dtls::{config::Config, listener};
use webrtc_util::conn::Listener;

use coap_lite::{CoapRequest, Packet};

use crate::router::CoapRouter;

const BUF_SIZE: usize = 8192;

pub async fn serve(
    addr: String,
    config: Config,
    router: CoapRouter,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = Arc::new(listener::listen(addr.clone(), config).await.unwrap());
    let router = Arc::new(Mutex::new(router));

    while let Ok((conn, socket_addr)) = listener.accept().await {
        let r = router.clone();

        tokio::spawn(async move {
            let mut b = vec![0u8; BUF_SIZE];

            loop {
                let recv = tokio::time::timeout(Duration::from_secs(10), conn.recv(&mut b)).await;

                // Check if timeout
                let recv = match recv {
                    Ok(r) => r,
                    Err(e) => {
                        log::error!("Timeout!: {}", e);
                        break;
                    }
                };

                match recv {
                    Ok(n) => {
                        let packet = Packet::from_bytes(&b[0..n]).unwrap();
                        let request = CoapRequest::from_packet(packet, socket_addr);

                        // Push it into the router
                        let fut = {
                            let mut r = r.lock().unwrap();
                            r.call(request)
                        };

                        // Get the response
                        let resp = fut.await.unwrap();
                        let bytes = resp.message.to_bytes().unwrap();

                        // Write it back
                        let _ = conn.send(&bytes);
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

    Ok(())
}
