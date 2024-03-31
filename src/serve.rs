use std::{collections::HashMap, fmt::Debug, net::SocketAddr, sync::Arc, time::Duration};

use tokio::sync::{
    mpsc::{channel, Sender},
    Mutex,
};
use tower::Service;
use webrtc_dtls::{conn::DTLSConn, listener};
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
    let connections: Arc<Mutex<HashMap<String, Sender<()>>>> = Arc::new(Mutex::new(HashMap::new()));

    loop {
        if let Ok((conn, socket_addr)) = listener.accept().await {
            log::info!("Got a connection from: {}", socket_addr);

            let mut router = router.clone();
            let identity: String;
            let timeout = config.timeout;

            let state = if let Some(dtls) = conn.as_any().downcast_ref::<DTLSConn>() {
                dtls.connection_state().await
            } else {
                log::error!("Unable to get state!");
                continue;
            };

            // Get PSK Identity and use it as the Client's ID
            identity = match String::from_utf8(state.identity_hint) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Unable to get identity! Error: {}", e);
                    continue;
                }
            };

            log::info!("PSK Identity: {}", identity);

            let cons = connections.clone();

            // Check for old connection and terminate it
            {
                if let Some(tx) = cons.lock().await.get(&identity) {
                    let _ = tx.send(()).await; // Signal the old connection to terminate
                }
            }

            tokio::spawn(async move {
                let (tx, mut rx) = channel::<()>(1);

                // Observers
                let (obs_tx, mut obs_rx) = channel::<ObserverValue>(10);
                let obs_tx = Arc::new(obs_tx);

                // Insert the channel
                {
                    cons.lock().await.insert(identity.clone(), tx);
                }

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
                                let path = request.get_path();
                                let observe_flag = *request.get_observe_flag();
                                let method = *request.get_method();

                                // Handle observations
                                match (observe_flag, method) {
                                    (Some(ObserveOption::Register), RequestType::Get) => {
                                        // register
                                        router.register_observer(&identity, path, obs_tx.clone()).await.unwrap();
                                    }
                                    (Some(ObserveOption::Deregister), RequestType::Delete) => {
                                        // unregister
                                        router.unregister_observer(&identity, path).await.unwrap();
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

// #[cfg(test)]
// mod tests {
//     use crate::router::wrapper::get;

//     use super::*;
//     use coap_lite::{CoapRequest, CoapResponse, Packet, RequestType, ResponseType};
//     use std::net::SocketAddr;

//     #[tokio::test]
//     async fn test_serve() {
//         // Set up test data
//         let addr = "127.0.0.1:5683".to_string();
//         let config = Config::default();

//         let mut router = CoapRouter::new((), ());
//         router.add(
//             "test",
//             get(|_, _| async { Ok(CoapResponse::new(&Packet::new()).unwrap()) }),
//         );

//         let mut request = CoapRequest::new();
//         request.set_method(RequestType::Get);
//         request.set_path("/test");

//         let identity = vec![0x01, 0x02, 0x03];

//         let mut request: CoapumRequest<SocketAddr> = request.into();
//         request.identity = identity.clone();

//         // Call the serve function
//         let result = serve(addr, config, router.clone()).await;

//         // Check that the serve function returns Ok
//         assert!(result.is_ok());

//         // Call the router with a GET request
//         let response = router.call(request).await.unwrap();

//         // Check that the response has a Valid status
//         assert_eq!(*response.get_status(), ResponseType::Content);

//         // Check that the response message is empty
//         assert!(response.message.payload.is_empty());

//         // Call the router with a DELETE request
//         let mut request = CoapRequest::new();
//         request.set_method(RequestType::Delete);
//         request.set_path("/test");

//         let mut request: CoapumRequest<SocketAddr> = request.into();
//         request.identity = identity.clone();

//         let response = router.call(request).await.unwrap();

//         // Check that the response has a Valid status
//         assert_eq!(*response.get_status(), ResponseType::BadRequest);
//     }
// }
