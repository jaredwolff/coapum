use std::net::SocketAddr;

use crate::router::CoapumRequest;

pub mod cbor;
pub mod json;
pub mod raw;

pub trait FromCoapumRequest {
    type Error;

    fn from_coap_request(request: &CoapumRequest<SocketAddr>) -> Result<Self, Self::Error>
    where
        Self: Sized;
}
