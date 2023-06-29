use std::{collections::HashMap, fmt::Debug, net::SocketAddr, sync::Arc, time::Duration};

use tokio::sync::{
    mpsc::{channel, Sender},
    Mutex,
};
use tower::Service;
use webrtc_dtls::listener;
use webrtc_util::conn::Listener;

use coap_lite::{CoapRequest, ObserveOption, Packet, RequestType, ResponseType};

use crate::{
    config::Config,
    observer::{Observer, ObserverValue},
    router::{CoapRouter, CoapumRequest},
};

const BUF_SIZE: usize = 8192;

pub async fn serve<O, S>(
    addr: String,
    config: Config,
    router: CoapRouter<O, S>,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: Debug + Clone + Send + Sync + 'static, // The shared state needs to be Send and Sync to be shared across threads
    O: Observer + Send + Sync + 'static,
{
    let listener = Arc::new(
        listener::listen(addr.clone(), config.dtls_cfg)
            .await
            .unwrap(),
    );
    let connections: Arc<Mutex<HashMap<Vec<u8>, Sender<()>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    loop {
        if let Ok((conn, state, socket_addr)) = listener.accept().await {
            log::info!("Got a connection from: {}", socket_addr);

            let mut router = router.clone();
            let mut identity = Vec::new();
            let socket_addr = socket_addr.clone();
            let timeout = config.timeout;

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
                let _ = tx.send(()).await; // Signal the old connection to terminate
            }

            tokio::spawn(async move {
                let (tx, mut rx) = channel::<()>(1);

                // Observers
                let (obs_tx, mut obs_rx) = channel::<ObserverValue>(10);
                let obs_tx = Arc::new(obs_tx);

                // Insert the channel
                cons.lock().await.insert(identity.clone(), tx);

                // Buffer
                let mut b = vec![0u8; BUF_SIZE];

                loop {
                    tokio::select! {
                        // Handling observer..
                        notify = obs_rx.recv() => {
                            if let Some(value) = notify {

                                log::info!("Got notification: {:?}", value);

                                // Convert to request
                                let req = value.to_request(socket_addr);

                                // Formulate request from Value (i.e. create JSON payload)
                                match router.call(req).await{
                                    Ok(resp)=> {

                                        // Check to make sure we don't send error messages since this is server internal
                                        if *resp.get_status() == ResponseType::BadRequest {
                                            log::error!("Error: {:?}", resp.message);
                                            continue;
                                        }

                                        // Then send..
                                        log::info!("Sending data to: {}", socket_addr);
                                        match conn.send(&resp.message.to_bytes().unwrap()).await{
                                            Ok(n) => log::debug!("Wrote {} notification bytes", n),
                                            Err(e) => {
                                                log::error!("Error: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => log::error!("Error: {}", e)
                                };
                            }
                        }
                        recv = tokio::time::timeout(Duration::from_secs(timeout), conn.recv(&mut b)) => {

                            // Check for timeout
                            let recv = match recv {
                                Ok(r) => r,
                                Err(e) => {
                                    log::error!("Timeout! Err: {}", e);

                                    // Since we timed out remove:
                                    let _ = cons.lock().await.remove(&identity);
                                    break;
                                }
                            };

                            if let Ok(n) = recv {
                                let packet = Packet::from_bytes(&b[..n]).unwrap();
                                let request = CoapRequest::from_packet(packet, socket_addr);

                                // Convert to wrapper
                                let mut request: CoapumRequest<SocketAddr> = request.into();
                                request.identity = identity.clone();

                                // Get path
                                let path = request.get_path().clone();
                                let observe_flag = request.get_observe_flag().clone();
                                let method = request.get_method().clone();

                                // Handle observations
                                match (observe_flag, method) {
                                    (Some(ObserveOption::Register), RequestType::Get) => {
                                        // register
                                        router.register_observer(path, obs_tx.clone()).await;
                                    }
                                    (Some(ObserveOption::Deregister), RequestType::Delete) => {
                                        // unregister
                                        router.unregister_observer(path).await;
                                    }
                                    _ => {}
                                };

                                // Call the service
                                match router.call(request).await
                                {
                                    Ok(resp) => {
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
                                    Err(e)=> {
                                        log::error!("Error: {}", e);
                                    }
                                }
                            }
                        }
                        _ = rx.recv() => {
                            log::info!("Terminating connection with: {}", socket_addr);
                            break;
                        }
                    }
                }

                log::info!("Terminated: {}", &socket_addr);
            });
        }
    }
}
