use std::net::SocketAddr;

use crate::router::{CoapumRequest, Request};
use serde_json::Value;

use super::FromCoapumRequest;

#[derive(Debug)]
pub struct JsonPayload {
    pub value: Value,
    pub raw: CoapumRequest<SocketAddr>,
}

impl FromCoapumRequest for JsonPayload {
    type Error = std::io::Error;

    fn from_coap_request(request: &CoapumRequest<SocketAddr>) -> Result<Self, Self::Error> {
        let value = serde_json::from_slice(&request.message.payload)?;
        let raw = request.clone();

        Ok(JsonPayload { value, raw })
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

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use coap_lite::Packet;

    use super::*;
    use crate::router::CoapumRequest;
    use crate::CoapRequest;
    use serde_json::json;

    #[test]
    fn test_from_coap_request() {
        let mut pkt = Packet::new();

        let value = json!({
            "code": 415,
            "message": null,
            "continue": false,
            "extra": { "numbers" : [8.2341e+4, 0.251425] },
        });

        // Serialize the value into the buffer
        let buffer = serde_json::to_vec(&value).unwrap();

        // Set value
        pkt.payload = buffer;

        let request = CoapRequest::from_packet(
            pkt,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );
        let raw_payload = JsonPayload::from_coap_request(&request.into()).unwrap();

        assert_eq!(raw_payload.value, value);
    }

    #[test]
    fn test_get_value() {
        let request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );

        let value = json!({
            "code": 415,
            "message": null,
            "continue": false,
            "extra": { "numbers" : [8.2341e+4, 0.251425] },
        });

        let raw_payload = JsonPayload {
            value: value.clone(),
            raw: request.into(),
        };
        assert_eq!(raw_payload.get_value(), &value);
    }

    #[test]
    fn test_get_raw() {
        let mut pkt = Packet::new();
        pkt.payload = "Test".as_bytes().to_vec();

        let request = CoapRequest::from_packet(
            pkt,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );

        let request: CoapumRequest<SocketAddr> = request.into();

        let raw_payload = JsonPayload {
            value: Value::Null,
            raw: request,
        };

        assert_eq!(
            raw_payload.get_raw().message.payload,
            "Test".as_bytes().to_vec()
        );
    }
}
