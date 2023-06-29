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
        Ok(CborPayload { value, raw })
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

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use coap_lite::Packet;

    use super::*;
    use crate::router::CoapumRequest;
    use crate::CoapRequest;
    use ciborium::cbor;

    #[test]
    fn test_from_coap_request() {
        let mut pkt = Packet::new();

        let value = ciborium::cbor!({
            "code" => 415,
            "message" => null,
            "continue" => false,
            "extra" => { "numbers" => [8.2341e+4, 0.251425] },
        })
        .unwrap();

        // Create a buffer to hold the serialized CBOR
        let mut buffer = Vec::new();

        // Serialize the CBOR value into the buffer
        match ciborium::ser::into_writer(&value, &mut buffer) {
            Ok(_) => {}
            Err(_e) => assert!(false),
        };

        // Set value
        pkt.payload = buffer;

        let request = CoapRequest::from_packet(
            pkt,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );
        let raw_payload = CborPayload::from_coap_request(&request.into()).unwrap();

        assert_eq!(raw_payload.value, serde_json::to_value(value).unwrap());
    }

    #[test]
    fn test_get_value() {
        let request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );

        let value = ciborium::cbor!({
            "code" => 415,
            "message" => null,
            "continue" => false,
            "extra" => { "numbers" => [8.2341e+4, 0.251425] },
        })
        .unwrap();
        let value = serde_json::to_value(value).unwrap();

        let raw_payload = CborPayload {
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

        let raw_payload = CborPayload {
            value: Value::Null,
            raw: request,
        };

        assert_eq!(
            raw_payload.get_raw().message.payload,
            "Test".as_bytes().to_vec()
        );
    }
}
