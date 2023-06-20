use std::{collections::HashMap, fmt::Debug, net::SocketAddr, sync::Arc, time::Duration};

use tokio::sync::{
    mpsc::{channel, Sender},
    Mutex,
};
use tower::Service;
use webrtc_dtls::{config::Config, listener};
use webrtc_util::{conn::Listener, Conn};

use coap_lite::{CoapRequest, Packet};

use crate::router::{CoapRouter, CoapumRequest};

const BUF_SIZE: usize = 8192;

async fn receive<S>(
    conn: Arc<dyn Conn + Send + Sync>,
    socket_addr: SocketAddr,
    r: &mut CoapRouter<S>,
    identity: Vec<u8>,
) where
    S: Debug + Clone + Send + Sync + 'static,
{
    let mut b = vec![0u8; BUF_SIZE];

    // Set timeout to 1 hour
    // TODO: Make this configurable
    let recv = tokio::time::timeout(Duration::from_secs(60 * 60), conn.recv(&mut b)).await;

    // Timeout handling
    let recv = match recv {
        Ok(r) => r,
        Err(e) => {
            log::error!("Timeout! Err: {}", e);
            return;
        }
    };

    match recv {
        Ok(n) => {
            let packet = Packet::from_bytes(&b[0..n]).unwrap();
            let request = CoapRequest::from_packet(packet, socket_addr);

            // Convert to wrapper
            let mut request: CoapumRequest<SocketAddr> = request.into();
            request.identity = identity.clone();

            log::debug!("Got {} bytes: {:?}", n, request);
            log::debug!(
                "Payload: {}",
                String::from_utf8(request.message.payload.to_vec()).unwrap(),
            );

            // Call the service
            let resp = match r.call(request).await {
                Ok(r) => r,
                Err(e) => {
                    log::error!("Fatal Error: {}", e);
                    return;
                }
            };

            // Get the response
            let bytes = resp.message.to_bytes().unwrap();
            log::debug!("Got response: {:?}", resp.message);

            // Write it back
            match conn.send(&bytes).await {
                Ok(n) => log::debug!("Wrote {} bytes", n),
                Err(e) => {
                    log::error!("Error: {}", e);
                }
            };
        }
        Err(e) => {
            log::error!("Error: {}", e);
        }
    }
}

pub async fn serve<S>(
    addr: String,
    config: Config,
    router: CoapRouter<S>,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Debug + Clone + Send + Sync + 'static, // The shared state needs to be Send and Sync to be shared across threads
{
    let listener = Arc::new(listener::listen(addr.clone(), config).await.unwrap());
    let connections: Arc<Mutex<HashMap<Vec<u8>, Sender<()>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    loop {
        if let Ok((conn, state, socket_addr)) = listener.accept().await {
            log::info!("Got a connection from: {}", socket_addr);

            let mut router = router.clone();
            let mut identity = Vec::new();

            // Get PSK Identity and use it as the Client's ID
            if let Some(s) = state {
                if let Some(s) = s.psk_identity() {
                    identity = s.clone();
                    log::info!("PSK Identity: {}", String::from_utf8(s).unwrap());
                }
            }

            let cons = connections.clone();

            // Check for old connection and terminate it
            if let Some(tx) = cons.lock().await.get(&identity) {
                log::info!("Terminating old connection with: {}", socket_addr);
                tx.send(()).await.unwrap(); // Signal the old connection to terminate
            }

            tokio::spawn(async move {
                let (tx, mut rx) = channel::<()>(1);

                // Insert the channel
                cons.lock().await.insert(identity.clone(), tx);

                loop {
                    tokio::select! {
                        _ = async {
                            receive(conn.clone(), socket_addr, &mut router, identity.clone()).await
                        } => {}
                        _ = rx.recv() => {
                            log::info!("Terminating connection with: {}", socket_addr);
                            break;
                        }
                    }
                }
            });
        }
    }
}
