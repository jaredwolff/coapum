use std::net::SocketAddr;

use crate::{
    helper,
    router::{CoapumRequest, Request},
};
use coap_lite::Packet;
use serde_json::Value;

use super::FromCoapumRequest;

pub struct CborPayload {
    pub value: Value,
    pub raw: Packet,
    pub identity: Vec<u8>,
}

impl FromCoapumRequest for CborPayload {
    type Error = std::io::Error;

    fn from_coap_request(request: &CoapumRequest<SocketAddr>) -> Result<Self, Self::Error> {
        let value = helper::convert_cbor_to_json(&request.message.payload)?;
        let raw = request.message.clone();
        let identity = request.identity.clone();
        Ok(CborPayload {
            value,
            raw,
            identity,
        })
    }
}

impl Request for CborPayload {
    fn get_value(&self) -> &Value {
        &self.value
    }

    fn get_raw(&self) -> &Packet {
        &self.raw
    }

    fn get_identity(&self) -> &Vec<u8> {
        &self.identity
    }
}
