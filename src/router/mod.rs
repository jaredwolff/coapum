use coap_lite::{
    CoapRequest, CoapResponse, ContentFormat, ObserveOption, Packet, RequestType, ResponseType,
};
use route_recognizer::Router;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::vec;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tower::Service;

use crate::extractor::cbor::CborPayload;
use crate::extractor::handle_payload_extraction;
use crate::extractor::json::JsonPayload;
use crate::extractor::raw::RawPayload;
use crate::observer::{Observer, ObserverRequest, ObserverValue};

use self::wrapper::{RequestTypeWrapper, RouteHandler};

pub mod wrapper;

pub type RouterError = Box<(dyn std::error::Error + Send + Sync + 'static)>;

pub trait Request: Send {
    fn get_value(&self) -> &Value;
    fn get_raw(&self) -> &CoapumRequest<SocketAddr>;
}

pub type Handler<S> = Arc<
    dyn Fn(
            Box<dyn Request>,
            Arc<Mutex<S>>,
        ) -> Pin<Box<dyn Future<Output = Result<CoapResponse, RouterError>> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub struct CoapRouter<O = (), S = ()>
where
    S: Clone + Debug,
    O: Observer,
{
    inner: Router<HashMap<RequestTypeWrapper, RouteHandler<S>>>,
    state: Arc<Mutex<S>>, // Shared state
    db: O,
}

impl<O, S> CoapRouter<O, S>
where
    S: Send + Clone + Debug,
    O: Observer + Send + Sync + Clone + 'static,
{
    pub fn new(state: S, db: O) -> Self {
        Self {
            inner: Router::new(),
            state: Arc::new(Mutex::new(state)),
            db,
        }
    }

    pub async fn register_observer(&mut self, path: String, sender: Arc<Sender<ObserverValue>>) {
        self.db.register(path, sender).await;
    }

    pub async fn unregister_observer(&mut self, path: String) {
        self.db.unregister(path).await;
    }

    pub async fn backend_write(&mut self, path: String, payload: Value) {
        self.db.write(path, payload).await;
    }

    pub async fn backend_read(&mut self, path: String) -> Option<Value> {
        self.db.read(path).await
    }

    pub fn add(&mut self, route: &str, handler: RouteHandler<S>) {
        // Check if route already exists
        match self.inner.recognize(route) {
            Ok(r) => {
                let mut r = (**r.handler()).clone();
                r.insert(handler.method.into(), handler);
                self.inner.add(route, r);
            }
            Err(_) => {
                let mut r = HashMap::new();
                r.insert(handler.method.into(), handler);
                self.inner.add(route, r);
            }
        };
    }

    pub fn lookup_observer_handler(&self, path: &str) -> Option<Handler<S>> {
        match self.inner.recognize(path) {
            Ok(matched) => {
                let handler = matched.handler();

                // If it's an observe, get by default
                let reqtype: RequestTypeWrapper = RequestType::Get.into();

                log::debug!("Matched route: {:?}", matched);
                match handler.get(&reqtype) {
                    Some(h) => {
                        log::debug!("Matched handler: {:?}", h);
                        Some(h.handler.clone())
                    }
                    None => {
                        log::debug!("No handler found");
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!("Unable to recognize. Err: {}", e);
                None
            }
        }
    }

    pub fn lookup(&self, r: &CoapumRequest<SocketAddr>) -> Option<Handler<S>> {
        match self.inner.recognize(r.get_path()) {
            Ok(matched) => {
                let handler = matched.handler();

                let reqtype: RequestTypeWrapper = r.get_method().into();

                log::debug!("Matched route: {:?}", matched);
                match handler.get(&reqtype) {
                    Some(h) => {
                        log::debug!("Matched handler: {:?}", h);
                        Some(h.handler.clone())
                    }
                    None => {
                        log::debug!("No handler found");
                        None
                    }
                }
            }
            Err(e) => {
                log::warn!("Unable to recognize. Err: {}", e);
                None
            }
        }
    }
}

#[derive(Debug, Clone)]
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
    pub fn get_path(&self) -> &String {
        &self.path
    }

    pub fn get_method(&self) -> &RequestType {
        &self.code
    }

    pub fn get_observe_flag(&self) -> &Option<ObserveOption> {
        &self.observe_flag
    }
}

pub fn create_observer_error_response(
    rtype: ResponseType,
) -> Pin<Box<dyn Future<Output = Result<CoapResponse, RouterError>> + Send>> {
    let pkt = Packet::default();
    let mut response = CoapResponse::new(&pkt).unwrap();
    response.set_status(rtype);

    Box::pin(async move { Ok(response) })
}

pub fn create_error_response(
    req: &CoapumRequest<SocketAddr>,
    rtype: ResponseType,
) -> Pin<Box<dyn Future<Output = Result<CoapResponse, RouterError>> + Send>> {
    let pkt = Packet::default();
    let mut response = CoapResponse::new(&pkt).unwrap();
    response.message.header.message_id = req.message.header.message_id;
    response.message.set_token(req.message.get_token().to_vec());
    response.set_status(rtype);

    Box::pin(async move { Ok(response) })
}

impl<O, S> Service<CoapumRequest<SocketAddr>> for CoapRouter<O, S>
where
    S: Debug + Send + Clone + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
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
                let path = request.get_path();
                log::debug!("Handler found for route: {:?}", &path);

                if let Some(format) = &request.message.get_content_format() {
                    log::info!("Content format: {:?}", format);

                    match format {
                        ContentFormat::ApplicationJSON => {
                            handle_payload_extraction::<JsonPayload, S>(&request, handler, state)
                        }
                        ContentFormat::ApplicationCBOR => {
                            handle_payload_extraction::<CborPayload, S>(&request, handler, state)
                        }
                        // All other unsupported formats for extraction
                        _ => handle_payload_extraction::<RawPayload, S>(&request, handler, state),
                    }
                } else {
                    log::debug!("Content format not declared");
                    handle_payload_extraction::<RawPayload, S>(&request, handler, state)
                }
            }
            None => {
                log::error!(
                    "No handler found for method: {:#?} to: {:?}",
                    request.get_method(),
                    request.get_path()
                );

                // If no route handler is found, return a not found error
                create_error_response(&request, ResponseType::BadRequest)
            }
        }
    }
}

impl<O, S> Service<ObserverRequest<SocketAddr>> for CoapRouter<O, S>
where
    S: Debug + Send + Clone + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
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

    fn call(&mut self, request: ObserverRequest<SocketAddr>) -> Self::Future {
        let state = self.state.clone(); // Clone the state so it can be moved into the async block

        match self.lookup_observer_handler(&request.path) {
            Some(handler) => {
                log::debug!("Handler found for route: {:?}", &request.path);

                let packet = Packet::default();
                let raw = CoapRequest::from_packet(packet, request.source);

                let payload = JsonPayload {
                    value: request.value,
                    raw: raw.into(),
                };

                handler(Box::new(payload), state)
            }
            None => {
                log::error!("No observer handler found for: {}", request.path);

                // If no route handler is found, return a not found error
                create_observer_error_response(ResponseType::BadRequest)
            }
        }
    }
}
