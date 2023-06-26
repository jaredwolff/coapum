use std::net::SocketAddr;

use crate::router::{CoapumRequest, Request};
use serde_json::Value;

use super::FromCoapumRequest;

pub struct RawPayload {
    pub value: Value,
    pub raw: CoapumRequest<SocketAddr>,
}

impl FromCoapumRequest for RawPayload {
    type Error = std::io::Error;

    fn from_coap_request(request: &CoapumRequest<SocketAddr>) -> Result<Self, Self::Error> {
        let value = Value::Null;
        let raw = request.clone();
        Ok(RawPayload { value, raw })
    }
}

impl Request for RawPayload {
    fn get_value(&self) -> &Value {
        &self.value
    }

    fn get_raw(&self) -> &CoapumRequest<SocketAddr> {
        &self.raw
    }
}
