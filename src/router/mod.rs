use coap_lite::{CoapRequest, CoapResponse, ContentFormat, ObserveOption, Packet, RequestType};
use route_recognizer::Router;
use serde_json::Value;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::vec;
use tower::Service;

use crate::extractor::cbor::CborPayload;
use crate::extractor::json::JsonPayload;
use crate::extractor::FromCoapumRequest;

use self::wrapper::RouteHandler;

pub mod wrapper;

pub type RouterError = Box<(dyn std::error::Error + Send + Sync + 'static)>;

pub trait Request: Send {
    fn get_value(&self) -> &Value;
    fn get_raw(&self) -> &Packet;
    fn get_identity(&self) -> &Vec<u8>;
}

pub type Handler<S> = Arc<
    dyn Fn(
            Box<dyn Request>,
            Arc<Mutex<S>>,
        ) -> Pin<Box<dyn Future<Output = Result<CoapResponse, RouterError>> + Send>>
        + Send
        + Sync,
>;

pub struct CoapRouter<S = ()> {
    inner: Router<RouteHandler<S>>,
    state: Arc<Mutex<S>>, // Shared state
}

impl CoapRouter<()> {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Default for CoapRouter<()> {
    fn default() -> Self {
        Self {
            inner: Router::new(),
            state: Arc::new(Mutex::new(())),
        }
    }
}

impl<S> CoapRouter<S>
where
    S: Send,
{
    pub fn new_with_state(state: S) -> Self {
        Self {
            inner: Router::new(),
            state: Arc::new(Mutex::new(state)),
        }
    }

    pub fn add(&mut self, route: &str, handler: RouteHandler<S>) {
        self.inner.add(route, handler);
    }

    pub fn lookup(&self, r: &CoapumRequest<SocketAddr>) -> Option<Handler<S>> {
        match self.inner.recognize(&r.get_path()) {
            Ok(matched) => {
                let handler = matched.handler();

                log::debug!("Matched route: {:?}", matched);

                if handler.method == *r.get_method() {
                    Some(handler.handler.clone())
                } else {
                    None
                }
            }
            Err(e) => {
                log::warn!("Unable to recognize. Err: {}", e);
                None
            }
        }
    }
}

#[derive(Debug)]
pub struct CoapumRequest<Endpoint> {
    pub message: Packet,
    code: RequestType,
    path: String,
    observe_flag: Option<ObserveOption>,
    pub response: Option<CoapResponse>,
    pub source: Option<Endpoint>,
    pub identity: Vec<u8>,
}

impl<Endpoint> From<CoapRequest<Endpoint>> for CoapumRequest<Endpoint> {
    fn from(req: CoapRequest<Endpoint>) -> Self {
        let path = req.get_path();
        let code = *req.get_method();
        let observe_flag = match req.get_observe_flag() {
            Some(o) => match o {
                Ok(o) => Some(o),
                Err(_) => None,
            },
            None => None,
        };

        Self {
            message: req.message,
            response: req.response,
            source: req.source,
            path,
            code,
            observe_flag,
            identity: vec![],
        }
    }
}

impl<Endpoint> CoapumRequest<Endpoint> {
    pub fn get_path(&self) -> String {
        self.path.clone()
    }

    pub fn get_method(&self) -> &RequestType {
        &self.code
    }

    pub fn get_observe_flag(&self) -> &Option<ObserveOption> {
        &self.observe_flag
    }
}

fn create_default_response(
) -> Pin<Box<dyn Future<Output = Result<CoapResponse, RouterError>> + Send>> {
    let pkt = Packet::default();
    let response = CoapResponse::new(&pkt).unwrap();
    Box::pin(async move { Ok(response) })
}

fn handle_payload_extraction<T, S>(
    request: &CoapumRequest<SocketAddr>,
    handler: Handler<S>,
    state: Arc<Mutex<S>>,
) -> Pin<Box<dyn Future<Output = Result<CoapResponse, RouterError>> + Send>>
where
    T: FromCoapumRequest<Error = std::io::Error> + Request + Send + 'static,
{
    match T::from_coap_request(request) {
        Ok(payload) => Box::pin(handler(Box::new(payload), state)),
        Err(e) => {
            log::warn!("Unable to parse payload: {}", e);
            create_default_response()
        }
    }
}

impl<S> Service<CoapumRequest<SocketAddr>> for CoapRouter<S>
where
    S: Send + Sync + 'static,
{
    type Response = CoapResponse;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        // Assume that the router is always ready.
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: CoapumRequest<SocketAddr>) -> Self::Future {
        let state = self.state.clone(); // Clone the state so it can be moved into the async block

        match self.lookup(&request) {
            Some(handler) => {
                log::debug!("Handler found for route: {:?}", request.get_path());

                if let Some(format) = &request.message.get_content_format() {
                    log::info!("Content format: {:?}", format);

                    match format {
                        ContentFormat::ApplicationJSON => {
                            handle_payload_extraction::<JsonPayload, S>(&request, handler, state)
                        }
                        ContentFormat::ApplicationCBOR => {
                            handle_payload_extraction::<CborPayload, S>(&request, handler, state)
                        }
                        _ => {
                            log::error!("Unsupported content format");
                            create_default_response()
                        }
                    }
                } else {
                    log::error!("Unsupported content format");
                    create_default_response()
                }
            }
            None => {
                log::error!(
                    "No handler found for method: {:#?} to: {:?}",
                    request.get_method(),
                    request.get_path()
                );

                // TODO: If no route handler is found, return a not found error
                let pkt = Packet::default();
                let response = CoapResponse::new(&pkt).unwrap();
                Box::pin(async move { Ok(response) })
            }
        }
    }
}
