use std::{net::SocketAddr, pin::Pin, sync::Arc};

use coap_lite::{CoapResponse, ResponseType};
use futures::Future;
use tokio::sync::Mutex;

use crate::router::{create_error_response, CoapumRequest, Handler, Request, RouterError};

pub mod cbor;
pub mod json;
pub mod raw;

pub trait FromCoapumRequest {
    type Error;

    fn from_coap_request(request: &CoapumRequest<SocketAddr>) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

pub fn handle_payload_extraction<T, S>(
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
            create_error_response(request, ResponseType::UnsupportedContentFormat)
        }
    }
}
