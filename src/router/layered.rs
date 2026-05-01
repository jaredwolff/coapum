use std::{fmt::Debug, future::Future, net::SocketAddr, sync::Arc};

use coap_lite::CoapResponse;
use tokio::sync::mpsc::Sender;
use tower::{Layer, Service};

use crate::{
    observer::{Observer, ObserverRequest, ObserverValue},
    serve::router_handle::{self, RouterHandle},
    service::CoapService,
};

use super::{BlockTransferEvent, CoapRouter, CoapumRequest, DeviceEvent};

// ─── LayeredCoapRouter ────────────────────────────────────────────────────────

/// A [`CoapRouter`] with a [`tower::Layer`] applied to both the request-dispatch
/// and observer-notification paths.
///
/// Constructed by [`RouterBuilder::layer`](super::RouterBuilder::layer). Lifecycle
/// operations (observer registration, device events) delegate to the inner
/// [`CoapRouter`] so they are unaffected by the layer.
///
/// To apply a layer to only one path class, call
/// [`layer_request_only`](LayeredCoapRouter::layer_request_only) or
/// [`layer_notification_only`](LayeredCoapRouter::layer_notification_only) on the
/// returned value.
///
/// Route registration after `.layer(...)` is a compile-time error — `LayeredCoapRouter`
/// has no route-registration methods:
///
/// ```compile_fail
/// # use coapum::{router::RouterBuilder, observer::memory::MemObserver, extract::StatusCode};
/// # use coapum::middleware::TraceLayer;
/// let _r = RouterBuilder::new((), MemObserver::new())
///     .layer(TraceLayer::new())
///     .get("fail", || async { StatusCode::Content }); // error: no method `get`
/// ```
pub struct LayeredCoapRouter<R, O, S>
where
    R: CoapService,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    /// Wrapped service — handles both request and notification dispatch.
    service: R,
    /// Retained inner router — provides lifecycle operations.
    inner: CoapRouter<O, S>,
}

impl<R, O, S> LayeredCoapRouter<R, O, S>
where
    R: CoapService,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    pub(super) fn new(service: R, inner: CoapRouter<O, S>) -> Self {
        Self { service, inner }
    }

    /// Apply another layer on top, wrapping the current service.
    ///
    /// Layers compose in inside-out order: `.layer(A).layer(B)` means B wraps A.
    pub fn layer<L>(self, layer: L) -> LayeredCoapRouter<L::Service, O, S>
    where
        L: Layer<R>,
        L::Service: CoapService,
    {
        LayeredCoapRouter {
            service: layer.layer(self.service),
            inner: self.inner,
        }
    }

    /// Apply a layer to the **request path only**.
    ///
    /// Notifications continue through the existing service stack unchanged.
    /// Use this for layers whose closure is typed to a specific request type,
    /// such as [`MapResponseLayer`](crate::middleware::MapResponseLayer) with an
    /// `Fn(&CoapumRequest<_>, &mut CoapResponse)` closure.
    pub fn layer_request_only<L>(
        self,
        layer: L,
    ) -> LayeredCoapRouter<RequestLayeredService<L::Service, R>, O, S>
    where
        L: Layer<R>,
        L::Service: Service<
                CoapumRequest<SocketAddr>,
                Response = CoapResponse,
                Error = std::convert::Infallible,
            > + Clone
            + Send
            + 'static,
        <L::Service as Service<CoapumRequest<SocketAddr>>>::Future: Send + 'static,
    {
        let full_notify = self.service.clone();
        let new_request = layer.layer(self.service);
        LayeredCoapRouter {
            service: RequestLayeredService {
                new_request,
                full_notify,
            },
            inner: self.inner,
        }
    }

    /// Apply a layer to the **notification path only**.
    ///
    /// Requests continue through the existing service stack unchanged.
    pub fn layer_notification_only<L>(
        self,
        layer: L,
    ) -> LayeredCoapRouter<NotificationLayeredService<L::Service, R>, O, S>
    where
        L: Layer<R>,
        L::Service: Service<
                ObserverRequest<SocketAddr>,
                Response = CoapResponse,
                Error = std::convert::Infallible,
            > + Clone
            + Send
            + 'static,
        <L::Service as Service<ObserverRequest<SocketAddr>>>::Future: Send + 'static,
    {
        let full_request = self.service.clone();
        let new_notify = layer.layer(self.service);
        LayeredCoapRouter {
            service: NotificationLayeredService {
                new_notify,
                full_request,
            },
            inner: self.inner,
        }
    }
}

impl<R, O, S> Clone for LayeredCoapRouter<R, O, S>
where
    R: CoapService,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    fn clone(&self) -> Self {
        Self {
            service: self.service.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<R, O, S> Service<CoapumRequest<SocketAddr>> for LayeredCoapRouter<R, O, S>
where
    R: CoapService,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = <R as Service<CoapumRequest<SocketAddr>>>::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Service::<CoapumRequest<SocketAddr>>::poll_ready(&mut self.service, cx)
    }

    fn call(&mut self, req: CoapumRequest<SocketAddr>) -> Self::Future {
        Service::<CoapumRequest<SocketAddr>>::call(&mut self.service, req)
    }
}

impl<R, O, S> Service<ObserverRequest<SocketAddr>> for LayeredCoapRouter<R, O, S>
where
    R: CoapService,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = <R as Service<ObserverRequest<SocketAddr>>>::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Service::<ObserverRequest<SocketAddr>>::poll_ready(&mut self.service, cx)
    }

    fn call(&mut self, req: ObserverRequest<SocketAddr>) -> Self::Future {
        Service::<ObserverRequest<SocketAddr>>::call(&mut self.service, req)
    }
}

impl<R, O, S> router_handle::sealed::Sealed for LayeredCoapRouter<R, O, S>
where
    R: CoapService,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
}

impl<R, O, S> RouterHandle for LayeredCoapRouter<R, O, S>
where
    R: CoapService,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    fn call_request(
        &mut self,
        req: CoapumRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_ {
        let fut = Service::<CoapumRequest<SocketAddr>>::call(&mut self.service, req);
        async move { fut.await.unwrap() }
    }

    fn call_notification(
        &mut self,
        req: ObserverRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_ {
        let fut = Service::<ObserverRequest<SocketAddr>>::call(&mut self.service, req);
        async move { fut.await.unwrap() }
    }

    fn register_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        RouterHandle::register_observer(&mut self.inner, device_id, path, sender)
    }

    fn unregister_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        RouterHandle::unregister_observer(&mut self.inner, device_id, path)
    }

    fn unregister_device_if_owned<'a>(
        &'a mut self,
        device_id: &'a str,
        owner: &'a Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        RouterHandle::unregister_device_if_owned(&mut self.inner, device_id, owner)
    }

    fn observer_count<'a>(&'a self, device_id: &'a str) -> impl Future<Output = usize> + Send + 'a {
        self.inner.observer_count(device_id)
    }

    fn has_observe_route(&self, path: &str) -> bool {
        self.inner.has_observe_route(path)
    }

    fn is_confirmable_notify(&self, path: &str) -> bool {
        self.inner.is_confirmable_notify(path)
    }

    fn emit_device_event(&self, event: DeviceEvent) {
        self.inner.emit_device_event(event);
    }

    fn emit_block_transfer_event(&self, event: BlockTransferEvent) {
        self.inner.emit_block_transfer_event(event);
    }
}

// ─── RequestLayeredService ────────────────────────────────────────────────────
// Dispatches requests through `New` and notifications through `Full`.
// Produced by `LayeredCoapRouter::layer_request_only`.

pub struct RequestLayeredService<New, Full> {
    new_request: New,
    full_notify: Full,
}

impl<New, Full> Clone for RequestLayeredService<New, Full>
where
    New: Clone,
    Full: Clone,
{
    fn clone(&self) -> Self {
        Self {
            new_request: self.new_request.clone(),
            full_notify: self.full_notify.clone(),
        }
    }
}

impl<New, Full> Service<CoapumRequest<SocketAddr>> for RequestLayeredService<New, Full>
where
    New: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    New::Future: Send + 'static,
    Full: Clone + Send + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = New::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.new_request.poll_ready(cx)
    }

    fn call(&mut self, req: CoapumRequest<SocketAddr>) -> Self::Future {
        self.new_request.call(req)
    }
}

impl<New, Full> Service<ObserverRequest<SocketAddr>> for RequestLayeredService<New, Full>
where
    New: Clone + Send + 'static,
    Full: Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    Full::Future: Send + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = Full::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.full_notify.poll_ready(cx)
    }

    fn call(&mut self, req: ObserverRequest<SocketAddr>) -> Self::Future {
        self.full_notify.call(req)
    }
}

// ─── NotificationLayeredService ───────────────────────────────────────────────
// Dispatches notifications through `New` and requests through `Full`.
// Produced by `LayeredCoapRouter::layer_notification_only`.

pub struct NotificationLayeredService<New, Full> {
    new_notify: New,
    full_request: Full,
}

impl<New, Full> Clone for NotificationLayeredService<New, Full>
where
    New: Clone,
    Full: Clone,
{
    fn clone(&self) -> Self {
        Self {
            new_notify: self.new_notify.clone(),
            full_request: self.full_request.clone(),
        }
    }
}

impl<New, Full> Service<CoapumRequest<SocketAddr>> for NotificationLayeredService<New, Full>
where
    New: Clone + Send + 'static,
    Full: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    Full::Future: Send + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = Full::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.full_request.poll_ready(cx)
    }

    fn call(&mut self, req: CoapumRequest<SocketAddr>) -> Self::Future {
        self.full_request.call(req)
    }
}

impl<New, Full> Service<ObserverRequest<SocketAddr>> for NotificationLayeredService<New, Full>
where
    New: Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    New::Future: Send + 'static,
    Full: Clone + Send + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = New::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.new_notify.poll_ready(cx)
    }

    fn call(&mut self, req: ObserverRequest<SocketAddr>) -> Self::Future {
        self.new_notify.call(req)
    }
}

// ─── LayeredCoapRouterRequestOnly ─────────────────────────────────────────────

/// A [`CoapRouter`] with a [`tower::Layer`] applied only to the request-dispatch
/// path. Observer notifications are dispatched directly through the inner router.
///
/// Constructed by [`RouterBuilder::layer_request_only`](super::RouterBuilder::layer_request_only).
pub struct LayeredCoapRouterRequestOnly<R, O, S>
where
    R: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    request_service: R,
    inner: CoapRouter<O, S>,
}

impl<R, O, S> LayeredCoapRouterRequestOnly<R, O, S>
where
    R: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    pub(super) fn new(request_service: R, inner: CoapRouter<O, S>) -> Self {
        Self {
            request_service,
            inner,
        }
    }
}

impl<R, O, S> Clone for LayeredCoapRouterRequestOnly<R, O, S>
where
    R: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    fn clone(&self) -> Self {
        Self {
            request_service: self.request_service.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<R, O, S> Service<CoapumRequest<SocketAddr>> for LayeredCoapRouterRequestOnly<R, O, S>
where
    R: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = R::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.request_service.poll_ready(cx)
    }

    fn call(&mut self, req: CoapumRequest<SocketAddr>) -> Self::Future {
        self.request_service.call(req)
    }
}

impl<R, O, S> Service<ObserverRequest<SocketAddr>> for LayeredCoapRouterRequestOnly<R, O, S>
where
    R: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = <CoapRouter<O, S> as Service<ObserverRequest<SocketAddr>>>::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Service::<ObserverRequest<SocketAddr>>::poll_ready(&mut self.inner, cx)
    }

    fn call(&mut self, req: ObserverRequest<SocketAddr>) -> Self::Future {
        Service::<ObserverRequest<SocketAddr>>::call(&mut self.inner, req)
    }
}

impl<R, O, S> router_handle::sealed::Sealed for LayeredCoapRouterRequestOnly<R, O, S>
where
    R: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    <R as Service<CoapumRequest<SocketAddr>>>::Future: Send + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
}

impl<R, O, S> RouterHandle for LayeredCoapRouterRequestOnly<R, O, S>
where
    R: Service<
            CoapumRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    <R as Service<CoapumRequest<SocketAddr>>>::Future: Send + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    fn call_request(
        &mut self,
        req: CoapumRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_ {
        let fut = self.request_service.call(req);
        async move { fut.await.unwrap() }
    }

    fn call_notification(
        &mut self,
        req: ObserverRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_ {
        self.inner.call_notification(req)
    }

    fn register_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        RouterHandle::register_observer(&mut self.inner, device_id, path, sender)
    }

    fn unregister_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        RouterHandle::unregister_observer(&mut self.inner, device_id, path)
    }

    fn unregister_device_if_owned<'a>(
        &'a mut self,
        device_id: &'a str,
        owner: &'a Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        RouterHandle::unregister_device_if_owned(&mut self.inner, device_id, owner)
    }

    fn observer_count<'a>(&'a self, device_id: &'a str) -> impl Future<Output = usize> + Send + 'a {
        self.inner.observer_count(device_id)
    }

    fn has_observe_route(&self, path: &str) -> bool {
        self.inner.has_observe_route(path)
    }

    fn is_confirmable_notify(&self, path: &str) -> bool {
        self.inner.is_confirmable_notify(path)
    }

    fn emit_device_event(&self, event: DeviceEvent) {
        self.inner.emit_device_event(event);
    }

    fn emit_block_transfer_event(&self, event: BlockTransferEvent) {
        self.inner.emit_block_transfer_event(event);
    }
}

// ─── LayeredCoapRouterNotificationOnly ────────────────────────────────────────

/// A [`CoapRouter`] with a [`tower::Layer`] applied only to the observer-notification
/// path. Requests are dispatched directly through the inner router.
///
/// Constructed by [`RouterBuilder::layer_notification_only`](super::RouterBuilder::layer_notification_only).
pub struct LayeredCoapRouterNotificationOnly<R, O, S>
where
    R: Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    notification_service: R,
    inner: CoapRouter<O, S>,
}

impl<R, O, S> LayeredCoapRouterNotificationOnly<R, O, S>
where
    R: Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    pub(super) fn new(notification_service: R, inner: CoapRouter<O, S>) -> Self {
        Self {
            notification_service,
            inner,
        }
    }
}

impl<R, O, S> Clone for LayeredCoapRouterNotificationOnly<R, O, S>
where
    R: Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    fn clone(&self) -> Self {
        Self {
            notification_service: self.notification_service.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl<R, O, S> Service<CoapumRequest<SocketAddr>> for LayeredCoapRouterNotificationOnly<R, O, S>
where
    R: Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = <CoapRouter<O, S> as Service<CoapumRequest<SocketAddr>>>::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        Service::<CoapumRequest<SocketAddr>>::poll_ready(&mut self.inner, cx)
    }

    fn call(&mut self, req: CoapumRequest<SocketAddr>) -> Self::Future {
        Service::<CoapumRequest<SocketAddr>>::call(&mut self.inner, req)
    }
}

impl<R, O, S> Service<ObserverRequest<SocketAddr>> for LayeredCoapRouterNotificationOnly<R, O, S>
where
    R: Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    type Response = CoapResponse;
    type Error = std::convert::Infallible;
    type Future = R::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.notification_service.poll_ready(cx)
    }

    fn call(&mut self, req: ObserverRequest<SocketAddr>) -> Self::Future {
        self.notification_service.call(req)
    }
}

impl<R, O, S> router_handle::sealed::Sealed for LayeredCoapRouterNotificationOnly<R, O, S>
where
    R: Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    <R as Service<ObserverRequest<SocketAddr>>>::Future: Send + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
}

impl<R, O, S> RouterHandle for LayeredCoapRouterNotificationOnly<R, O, S>
where
    R: Service<
            ObserverRequest<SocketAddr>,
            Response = CoapResponse,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    <R as Service<ObserverRequest<SocketAddr>>>::Future: Send + 'static,
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    fn call_request(
        &mut self,
        req: CoapumRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_ {
        self.inner.call_request(req)
    }

    fn call_notification(
        &mut self,
        req: ObserverRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_ {
        let fut = self.notification_service.call(req);
        async move { fut.await.unwrap() }
    }

    fn register_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        RouterHandle::register_observer(&mut self.inner, device_id, path, sender)
    }

    fn unregister_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        RouterHandle::unregister_observer(&mut self.inner, device_id, path)
    }

    fn unregister_device_if_owned<'a>(
        &'a mut self,
        device_id: &'a str,
        owner: &'a Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        RouterHandle::unregister_device_if_owned(&mut self.inner, device_id, owner)
    }

    fn observer_count<'a>(&'a self, device_id: &'a str) -> impl Future<Output = usize> + Send + 'a {
        self.inner.observer_count(device_id)
    }

    fn has_observe_route(&self, path: &str) -> bool {
        self.inner.has_observe_route(path)
    }

    fn is_confirmable_notify(&self, path: &str) -> bool {
        self.inner.is_confirmable_notify(path)
    }

    fn emit_device_event(&self, event: DeviceEvent) {
        self.inner.emit_device_event(event);
    }

    fn emit_block_transfer_event(&self, event: BlockTransferEvent) {
        self.inner.emit_block_transfer_event(event);
    }
}
