use std::net::SocketAddr;

use crate::router::{CoapumRequest, Request};
use coap_lite::Packet;
use serde_json::Value;

use super::FromCoapumRequest;

pub struct RawPayload {
    pub value: Value,
    pub raw: Packet,
    pub identity: Vec<u8>,
}

impl FromCoapumRequest for RawPayload {
    type Error = std::io::Error;

    fn from_coap_request(request: &CoapumRequest<SocketAddr>) -> Result<Self, Self::Error> {
        let value = Value::Null;
        let raw = request.message.clone();
        let identity = request.identity.clone();
        Ok(RawPayload {
            value,
            raw,
            identity,
        })
    }
}

impl Request for RawPayload {
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
