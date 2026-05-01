use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use coap_lite::{CoapResponse, MessageClass, ResponseType};
use tower::{Layer, Service};

/// Tower layer that enforces a per-call timeout on the inner service.
///
/// When the inner future exceeds the deadline, a [`ResponseType::GatewayTimeout`]
/// (CoAP 5.04) response is returned as `Ok(_)`. This upholds the
/// `Error = Infallible` discipline — the timeout is encoded in the response, not
/// in the error channel.
pub struct TimeoutLayer {
    deadline: Duration,
}

impl TimeoutLayer {
    pub fn new(deadline: Duration) -> Self {
        Self { deadline }
    }
}

impl<S> Layer<S> for TimeoutLayer {
    type Service = Timeout<S>;

    fn layer(&self, inner: S) -> Self::Service {
        Timeout {
            inner,
            deadline: self.deadline,
        }
    }
}

/// Service produced by [`TimeoutLayer`].
#[derive(Clone)]
pub struct Timeout<S> {
    inner: S,
    deadline: Duration,
}

impl<S, Req> Service<Req> for Timeout<S>
where
    S: Service<Req, Response = CoapResponse, Error = Infallible>,
    S::Future: Send + 'static,
    Req: Send + 'static,
{
    type Response = CoapResponse;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let deadline = self.deadline;
        let fut = self.inner.call(req);
        Box::pin(async move {
            match tokio::time::timeout(deadline, fut).await {
                Ok(resp) => resp,
                Err(_elapsed) => {
                    let packet = coap_lite::Packet::new();
                    let mut resp = CoapResponse::new(&packet)
                        .expect("CoapResponse::new with empty packet is infallible");
                    resp.message.header.code = MessageClass::Response(ResponseType::GatewayTimeout);
                    Ok(resp)
                }
            }
        })
    }
}
