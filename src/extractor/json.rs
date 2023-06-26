use std::net::SocketAddr;

use crate::router::{CoapumRequest, Request};
use serde_json::Value;

use super::FromCoapumRequest;

#[derive(Debug)]
pub struct JsonPayload {
    pub value: Value,
    pub raw: CoapumRequest<SocketAddr>
}

impl FromCoapumRequest for JsonPayload {
    type Error = std::io::Error;

    fn from_coap_request(request: &CoapumRequest<SocketAddr>) -> Result<Self, Self::Error> {
        let value = serde_json::from_slice(&request.message.payload)?;
        let raw = request.clone();

        Ok(JsonPayload {
            value,
            raw,
        })
    }
}

impl Request for JsonPayload {
    fn get_value(&self) -> &Value {
        &self.value
    }

    fn get_raw(&self) -> &CoapumRequest<SocketAddr> {
        &self.raw
    }
}
