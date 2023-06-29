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

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use coap_lite::Packet;

    use super::*;
    use crate::CoapRequest;

    #[test]
    fn test_from_coap_request() {
        let mut pkt = Packet::new();
        pkt.payload = "Test".as_bytes().to_vec();

        let request = CoapRequest::from_packet(
            pkt,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );
        let raw_payload = RawPayload::from_coap_request(&request.into()).unwrap();

        assert_eq!(raw_payload.value, Value::Null);
    }

    #[test]
    fn test_get_value() {
        let request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );

        let raw_payload = RawPayload {
            value: Value::Null,
            raw: request.into(),
        };
        assert_eq!(*raw_payload.get_value(), Value::Null);
    }

    #[test]
    fn test_get_raw() {
        let mut pkt = Packet::new();
        pkt.payload = "Test".as_bytes().to_vec();

        let request = CoapRequest::from_packet(
            pkt,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );
        let raw_payload = RawPayload::from_coap_request(&request.into()).unwrap();

        assert_eq!(
            raw_payload.get_raw().message.payload,
            "Test".as_bytes().to_vec()
        );
    }
}
