use coap_lite::{CoapResponse, Packet, RequestType, ResponseType};

use core::fmt::{self, Debug};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::{fmt::Formatter, hash::Hasher};


use super::CoapumRequest;
use crate::handler::ErasedHandler;

/// A wrapper struct for `RequestType` that implements `Hash`, `PartialEq`, and `Eq` traits.
#[derive(Clone, Copy, Debug)]
pub struct RequestTypeWrapper(RequestType);

impl std::hash::Hash for RequestTypeWrapper {
    /// Hashes the `RequestTypeWrapper` instance.
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.0 {
            RequestType::Get => 0u8.hash(state),
            RequestType::Post => 0u8.hash(state),
            RequestType::Put => 0u8.hash(state),
            RequestType::Delete => 0u8.hash(state),
            RequestType::Fetch => 0u8.hash(state),
            RequestType::Patch => 0u8.hash(state),
            RequestType::IPatch => 0u8.hash(state),
            RequestType::UnKnown => 0u8.hash(state),
        }
    }
}

impl PartialEq for RequestTypeWrapper {
    /// Compares two `RequestTypeWrapper` instances for equality.
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for RequestTypeWrapper {}

impl From<RequestType> for RequestTypeWrapper {
    /// Converts a `RequestType` instance to a `RequestTypeWrapper` instance.
    fn from(r: RequestType) -> Self {
        RequestTypeWrapper(r)
    }
}

impl From<&RequestType> for RequestTypeWrapper {
    /// Converts a reference to a `RequestType` instance to a `RequestTypeWrapper` instance.
    fn from(r: &RequestType) -> Self {
        RequestTypeWrapper(*r)
    }
}

/// A struct that represents a route handler.
pub struct RouteHandler<S>
where
    S: Send + Sync + 'static,
{
    /// The handler function for the route.
    pub handler: Box<dyn ErasedHandler<S>>,
    /// The handler function for the observe request.
    pub observe_handler: Option<Box<dyn ErasedHandler<S>>>,
    /// The request type for the route.
    pub method: RequestType,
}

impl<S> Debug for RouteHandler<S>
where
    S: Send + Sync + 'static,
{
    /// Formats the `RouteHandler` instance for debugging purposes.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "RouteHandler {{ method: {:?} }}", self.method)
    }
}

impl<S> Clone for RouteHandler<S>
where
    S: Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone_erased(),
            observe_handler: self.observe_handler.as_ref().map(|h| h.clone_erased()),
            method: self.method,
        }
    }
}

pub type CoapResponseResult = Result<CoapResponse, Infallible>;

pub trait IntoCoapResponse {
    fn into_response(self) -> CoapResponseResult;
}

impl IntoCoapResponse for ResponseType {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.set_status(self);
        Ok(response)
    }
}

impl<R> IntoCoapResponse for (ResponseType, R)
where
    R: IntoCoapResponse,
{
    fn into_response(self) -> CoapResponseResult {
        let mut response = self.1.into_response().unwrap();
        response.set_status(self.0);
        Ok(response)
    }
}


impl IntoCoapResponse for &CoapumRequest<SocketAddr> {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.message.header.message_id = self.message.header.message_id;
        response
            .message
            .set_token(self.message.get_token().to_vec());
        Ok(response)
    }
}

impl IntoCoapResponse for CoapumRequest<SocketAddr> {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.message.header.message_id = self.message.header.message_id;
        response
            .message
            .set_token(self.message.get_token().to_vec());
        Ok(response)
    }
}

impl IntoCoapResponse for ciborium::Value {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();

        let mut buffer = Vec::new();
        let _ = ciborium::ser::into_writer(&self, &mut buffer);

        response.message.payload = buffer;
        Ok(response)
    }
}

impl IntoCoapResponse for serde_json::Value {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.message.payload = serde_json::to_vec(&self).unwrap();
        Ok(response)
    }
}

impl IntoCoapResponse for Vec<u8> {
    fn into_response(self) -> CoapResponseResult {
        let pkt = Packet::new();
        let mut response = CoapResponse::new(&pkt).unwrap();
        response.message.payload = self;
        Ok(response)
    }
}
