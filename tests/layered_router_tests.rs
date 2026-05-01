//! Integration tests for the layered router system.
//!
//! Covers:
//! - `RouterBuilder::layer` / `layer_request_only` / `layer_notification_only`
//! - Layer chaining order
//! - `MapResponseLayer`, `TraceLayer`, `TimeoutLayer`
//! - Layer interaction with large payloads

use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use ciborium::value::Value;
use coapum::{
    CoapResponse, CoapumRequest, ContentFormat, MessageClass, ResponseType,
    extract::StatusCode,
    middleware::{MapResponseLayer, TimeoutLayer, TraceLayer},
    observer::{ObserverRequest, memory::MemObserver},
    router::RouterBuilder,
};
use tower::Service;

// ── helpers ──────────────────────────────────────────────────────────────────

fn test_addr() -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
}

fn observer_req(path: &str) -> ObserverRequest<SocketAddr> {
    ObserverRequest {
        value: Value::Text("x".into()),
        path: path.to_string(),
        source: test_addr(),
    }
}

// A layer that counts how many times it intercepts a call, for both request
// and notification paths.
#[derive(Clone)]
struct CountLayer(Arc<AtomicUsize>);

#[derive(Clone)]
struct CountService<S> {
    inner: S,
    counter: Arc<AtomicUsize>,
}

impl<S> tower::Layer<S> for CountLayer {
    type Service = CountService<S>;
    fn layer(&self, inner: S) -> CountService<S> {
        CountService {
            inner,
            counter: Arc::clone(&self.0),
        }
    }
}

impl<S, Req> Service<Req> for CountService<S>
where
    S: Service<Req, Response = CoapResponse, Error = std::convert::Infallible> + Send + 'static,
    S::Future: Send + 'static,
    Req: Send + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<CoapResponse, std::convert::Infallible>>
                + Send
                + 'static,
        >,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        self.counter.fetch_add(1, Ordering::SeqCst);
        Box::pin(self.inner.call(req))
    }
}

// ── Task 9: combinator behavior ───────────────────────────────────────────────

#[tokio::test]
async fn layer_applies_to_both_paths() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut router = RouterBuilder::new((), MemObserver::new())
        .observe(
            "/ping",
            || async { StatusCode::Content },
            || async { StatusCode::Content },
        )
        .layer(CountLayer(Arc::clone(&counter)));

    // Request path
    let req = coapum::test_utils::create_test_request("/ping");
    let resp = Service::<_>::call(&mut router, req).await.unwrap();
    assert_eq!(*resp.get_status(), ResponseType::Content);
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "layer should intercept request"
    );

    // Notification path
    let obs = observer_req("/ping");
    let _resp = Service::<ObserverRequest<_>>::call(&mut router, obs)
        .await
        .unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        2,
        "layer should intercept notification"
    );
}

#[tokio::test]
async fn layer_request_only_skips_notifications() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut router = RouterBuilder::new((), MemObserver::new())
        .observe(
            "/ping",
            || async { StatusCode::Content },
            || async { StatusCode::Content },
        )
        .layer_request_only(CountLayer(Arc::clone(&counter)));

    // Request path — layer fires
    let req = coapum::test_utils::create_test_request("/ping");
    Service::<_>::call(&mut router, req).await.unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    // Notification path — layer does NOT fire
    let obs = observer_req("/ping");
    Service::<ObserverRequest<_>>::call(&mut router, obs)
        .await
        .unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "notification should bypass request-only layer"
    );
}

#[tokio::test]
async fn layer_notification_only_skips_requests() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut router = RouterBuilder::new((), MemObserver::new())
        .observe(
            "/ping",
            || async { StatusCode::Content },
            || async { StatusCode::Content },
        )
        .layer_notification_only(CountLayer(Arc::clone(&counter)));

    // Notification path — layer fires
    let obs = observer_req("/ping");
    Service::<ObserverRequest<_>>::call(&mut router, obs)
        .await
        .unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    // Request path — layer does NOT fire
    let req = coapum::test_utils::create_test_request("/ping");
    Service::<_>::call(&mut router, req).await.unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "request should bypass notification-only layer"
    );
}

// Task 9: chaining order — B.layer(A) means A runs before B (inside-out)
#[tokio::test]
async fn chained_layers_fire_in_order() {
    // We track call order via a shared Vec protected by a Mutex.
    let order: Arc<std::sync::Mutex<Vec<u8>>> = Arc::new(std::sync::Mutex::new(vec![]));

    #[derive(Clone)]
    struct OrderLayer(Arc<std::sync::Mutex<Vec<u8>>>, u8);
    #[derive(Clone)]
    struct OrderService<S> {
        inner: S,
        order: Arc<std::sync::Mutex<Vec<u8>>>,
        id: u8,
    }
    impl<S> tower::Layer<S> for OrderLayer {
        type Service = OrderService<S>;
        fn layer(&self, inner: S) -> OrderService<S> {
            OrderService {
                inner,
                order: Arc::clone(&self.0),
                id: self.1,
            }
        }
    }
    impl<S, Req> Service<Req> for OrderService<S>
    where
        S: Service<Req, Response = CoapResponse, Error = std::convert::Infallible> + Send + 'static,
        S::Future: Send + 'static,
        Req: Send + 'static,
    {
        type Response = CoapResponse;
        type Error = std::convert::Infallible;
        type Future = std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<CoapResponse, std::convert::Infallible>>
                    + Send
                    + 'static,
            >,
        >;

        fn poll_ready(
            &mut self,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            self.inner.poll_ready(cx)
        }

        fn call(&mut self, req: Req) -> Self::Future {
            self.order.lock().unwrap().push(self.id);
            Box::pin(self.inner.call(req))
        }
    }

    // .layer(A).layer(B): B is the outermost wrapper, so B runs first.
    let mut router = RouterBuilder::new((), MemObserver::new())
        .get("/ping", || async { StatusCode::Content })
        .layer(OrderLayer(Arc::clone(&order), 1)) // inner: id=1
        .layer(OrderLayer(Arc::clone(&order), 2)); // outer: id=2

    let req = coapum::test_utils::create_test_request("/ping");
    Service::<_>::call(&mut router, req).await.unwrap();

    // Outer (id=2) runs first, then inner (id=1)
    assert_eq!(*order.lock().unwrap(), vec![2, 1]);
}

// ── Task 6 + Task 4: MapResponseLayer ────────────────────────────────────────

#[tokio::test]
async fn map_response_modifies_request_response() {
    let mut router = RouterBuilder::new((), MemObserver::new())
        .get("/data", || async { StatusCode::Content })
        .layer_request_only(MapResponseLayer::new(
            |_req: &CoapumRequest<SocketAddr>, resp: &mut CoapResponse| {
                resp.message
                    .set_content_format(ContentFormat::ApplicationJSON);
            },
        ));

    let req = coapum::test_utils::create_test_request("/data");
    let resp = Service::<_>::call(&mut router, req).await.unwrap();
    assert_eq!(*resp.get_status(), ResponseType::Content);
    assert_eq!(
        resp.message.get_content_format(),
        Some(ContentFormat::ApplicationJSON),
        "MapResponseLayer should have set content format on request path"
    );
}

#[tokio::test]
async fn map_response_modifies_notification_response() {
    let mut router = RouterBuilder::new((), MemObserver::new())
        .observe(
            "/data",
            || async { StatusCode::Content },
            || async { StatusCode::Content },
        )
        .layer_notification_only(MapResponseLayer::new(
            |_req: &ObserverRequest<SocketAddr>, resp: &mut CoapResponse| {
                resp.message
                    .set_content_format(ContentFormat::ApplicationJSON);
            },
        ));

    let obs = observer_req("/data");
    let resp = Service::<ObserverRequest<_>>::call(&mut router, obs)
        .await
        .unwrap();
    assert_eq!(
        resp.message.get_content_format(),
        Some(ContentFormat::ApplicationJSON),
        "MapResponseLayer should apply to notification path via layer_notification_only"
    );
}

#[tokio::test]
async fn map_response_large_payload_passes_through() {
    // Verifies that the layer is invoked once per assembled response even for large payloads.
    let call_count = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&call_count);
    let payload = vec![0xABu8; 2048];

    let mut router = RouterBuilder::new((), MemObserver::new())
        .post("/upload", move |coapum::Bytes(body): coapum::Bytes| {
            let len = body.len();
            async move {
                if len == 2048 {
                    StatusCode::Changed
                } else {
                    StatusCode::BadRequest
                }
            }
        })
        .layer_request_only(MapResponseLayer::new(
            move |_req: &CoapumRequest<SocketAddr>, _resp: &mut CoapResponse| {
                count.fetch_add(1, Ordering::SeqCst);
            },
        ));

    let req = coapum::test_utils::create_test_request_with_payload("/upload", payload);
    let resp = Service::<_>::call(&mut router, req).await.unwrap();
    assert_eq!(*resp.get_status(), ResponseType::Changed);
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "layer fires exactly once"
    );
}

// ── Task 11: TraceLayer ────────────────────────────────────────────────────────

#[tokio::test]
async fn trace_layer_passes_response_through() {
    let mut router = RouterBuilder::new((), MemObserver::new())
        .get("/ping", || async { StatusCode::Content })
        .layer(TraceLayer::new());

    let req = coapum::test_utils::create_test_request("/ping");
    let resp = Service::<_>::call(&mut router, req).await.unwrap();
    assert_eq!(*resp.get_status(), ResponseType::Content);
}

#[tokio::test]
async fn trace_layer_passes_notification_through() {
    let mut router = RouterBuilder::new((), MemObserver::new())
        .observe(
            "/ping",
            || async { StatusCode::Content },
            || async { StatusCode::Content },
        )
        .layer(TraceLayer::new());

    let obs = observer_req("/ping");
    let resp = Service::<ObserverRequest<_>>::call(&mut router, obs)
        .await
        .unwrap();
    assert_eq!(*resp.get_status(), ResponseType::Content);
}

// ── Task 12: TimeoutLayer ──────────────────────────────────────────────────────

#[tokio::test]
async fn timeout_returns_gateway_timeout_on_slow_handler() {
    let mut router = RouterBuilder::new((), MemObserver::new())
        .get("/slow", || async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            StatusCode::Content
        })
        .layer(TimeoutLayer::new(Duration::from_millis(50)));

    let req = coapum::test_utils::create_test_request("/slow");
    let resp = Service::<_>::call(&mut router, req).await.unwrap();
    assert_eq!(
        resp.message.header.code,
        MessageClass::Response(ResponseType::GatewayTimeout),
        "slow handler should produce 5.04 GatewayTimeout"
    );
}

#[tokio::test]
async fn timeout_passes_fast_handler() {
    let mut router = RouterBuilder::new((), MemObserver::new())
        .get("/fast", || async { StatusCode::Content })
        .layer(TimeoutLayer::new(Duration::from_secs(5)));

    let req = coapum::test_utils::create_test_request("/fast");
    let resp = Service::<_>::call(&mut router, req).await.unwrap();
    assert_eq!(*resp.get_status(), ResponseType::Content);
}

#[tokio::test]
async fn timeout_applies_to_notification_path() {
    let mut router = RouterBuilder::new((), MemObserver::new())
        .observe(
            "/slow_obs",
            || async { StatusCode::Content },
            || async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                StatusCode::Content
            },
        )
        .layer(TimeoutLayer::new(Duration::from_millis(50)));

    let obs = observer_req("/slow_obs");
    let resp = Service::<ObserverRequest<_>>::call(&mut router, obs)
        .await
        .unwrap();
    assert_eq!(
        resp.message.header.code,
        MessageClass::Response(ResponseType::GatewayTimeout),
        "slow notification handler should produce 5.04 GatewayTimeout"
    );
}
