use std::{net::SocketAddr, pin::Pin, sync::Arc};

use coap_lite::{CoapResponse, ResponseType};
use futures::Future;
use tokio::sync::Mutex;

use crate::router::{create_error_response, CoapumRequest, Handler, Request, RouterError};

pub mod cbor;
pub mod json;
pub mod raw;

pub trait FromCoapumRequest {
    type Error;

    fn from_coap_request(request: &CoapumRequest<SocketAddr>) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

pub fn handle_payload_extraction<T, S>(
    request: &CoapumRequest<SocketAddr>,
    handler: Handler<S>,
    state: Arc<Mutex<S>>,
) -> Pin<Box<dyn Future<Output = Result<CoapResponse, RouterError>> + Send>>
where
    T: FromCoapumRequest<Error = std::io::Error> + Request + Send + 'static,
{
    match T::from_coap_request(request) {
        Ok(payload) => Box::pin(handler(Box::new(payload), state)),
        Err(e) => {
            log::warn!("Unable to parse payload: {}", e);
            create_error_response(request, ResponseType::UnsupportedContentFormat)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        extractor::json::JsonPayload,
        router::wrapper::{post, RouteHandler},
    };

    use super::{raw::RawPayload, *};
    use coap_lite::{CoapRequest, CoapResponse, ContentFormat, Packet};
    use std::{
        net::{Ipv4Addr, SocketAddrV4},
        sync::Arc,
    };
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn test_handle_payload_extraction_ok() {
        let handler: RouteHandler<()> = post(|_, _| async {
            let pkt = Packet::default();
            Ok(CoapResponse::new(&pkt).unwrap())
        });

        let request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );

        let request: CoapumRequest<SocketAddr> = request.into();

        let state = Arc::new(Mutex::new(()));

        let result = handle_payload_extraction::<RawPayload, ()>(&request, handler.handler, state)
            .await
            .unwrap();

        assert_eq!(*result.get_status(), ResponseType::Content);
    }

    #[tokio::test]
    async fn test_handle_payload_extraction_err() {
        let handler: RouteHandler<()> = post(|_, _| async {
            let pkt = Packet::default();
            Ok(CoapResponse::new(&pkt).unwrap())
        });

        let mut request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );
        request
            .message
            .set_content_format(ContentFormat::ApplicationJSON);
        request.message.payload = vec![0x01, 0x02, 0x03];
        let request: CoapumRequest<SocketAddr> = request.into();

        let state = Arc::new(Mutex::new(()));
        let result = handle_payload_extraction::<JsonPayload, ()>(&request, handler.handler, state)
            .await
            .unwrap();

        assert_eq!(*result.get_status(), ResponseType::UnsupportedContentFormat);
    }
}
