use std::net::SocketAddr;

use crate::{
    helper,
    router::{CoapumRequest, Request},
};
use serde_json::Value;

use super::FromCoapumRequest;

pub struct CborPayload {
    pub value: Value,
    pub raw: CoapumRequest<SocketAddr>,
}

impl FromCoapumRequest for CborPayload {
    type Error = std::io::Error;

    fn from_coap_request(request: &CoapumRequest<SocketAddr>) -> Result<Self, Self::Error> {
        let value = helper::convert_cbor_to_json(&request.message.payload)?;
        let raw = request.clone();
        Ok(CborPayload {
            value,
            raw,
        })
    }
}

impl Request for CborPayload {
    fn get_value(&self) -> &Value {
        &self.value
    }

    fn get_raw(&self) -> &CoapumRequest<SocketAddr> {
        &self.raw
    }
}
