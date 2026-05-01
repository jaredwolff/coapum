use std::{fmt::Debug, future::Future, net::SocketAddr, sync::Arc};

use coap_lite::CoapResponse;
use tokio::sync::mpsc::Sender;
use tower::Service;

use crate::{
    observer::{Observer, ObserverRequest, ObserverValue},
    router::{BlockTransferEvent, CoapRouter, CoapumRequest, DeviceEvent},
};

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// Sealed trait bundling dispatch (via `tower::Service`) and lifecycle
/// operations (observer registration, device events) needed by the serve loop.
///
/// Implemented by [`CoapRouter<O, S>`] and by the layered wrappers produced by
/// [`RouterBuilder::layer`](crate::router::RouterBuilder::layer).
///
/// This trait is sealed: only types inside this crate can implement it.
/// Consumers interact with it only as a bound on [`bind_and_spawn`](crate::bind_and_spawn)
/// and related serve functions.
pub trait RouterHandle: sealed::Sealed + Clone + Send + 'static {
    // --- dispatch ---

    fn call_request(
        &mut self,
        req: CoapumRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_;

    fn call_notification(
        &mut self,
        req: ObserverRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_;

    // --- observer lifecycle ---

    fn register_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a;

    fn unregister_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a;

    fn unregister_device_if_owned<'a>(
        &'a mut self,
        device_id: &'a str,
        owner: &'a Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a;

    fn observer_count<'a>(&'a self, device_id: &'a str) -> impl Future<Output = usize> + Send + 'a;

    // --- route metadata ---

    fn has_observe_route(&self, path: &str) -> bool;

    fn is_confirmable_notify(&self, path: &str) -> bool;

    // --- event emission ---

    fn emit_device_event(&self, event: DeviceEvent);

    fn emit_block_transfer_event(&self, event: BlockTransferEvent);
}

impl<O, S> sealed::Sealed for CoapRouter<O, S>
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
}

impl<O, S> RouterHandle for CoapRouter<O, S>
where
    S: Debug + Clone + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    fn call_request(
        &mut self,
        req: CoapumRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_ {
        let fut = Service::<CoapumRequest<SocketAddr>>::call(self, req);
        async move { fut.await.unwrap() }
    }

    fn call_notification(
        &mut self,
        req: ObserverRequest<SocketAddr>,
    ) -> impl Future<Output = CoapResponse> + Send + '_ {
        let fut = Service::<ObserverRequest<SocketAddr>>::call(self, req);
        async move { fut.await.unwrap() }
    }

    fn register_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        let fut = CoapRouter::register_observer(self, device_id, path, sender);
        async move { fut.await.map_err(|e| format!("{e:?}")) }
    }

    fn unregister_observer<'a>(
        &'a mut self,
        device_id: &'a str,
        path: &'a str,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        let fut = CoapRouter::unregister_observer(self, device_id, path);
        async move { fut.await.map_err(|e| format!("{e:?}")) }
    }

    fn unregister_device_if_owned<'a>(
        &'a mut self,
        device_id: &'a str,
        owner: &'a Arc<Sender<ObserverValue>>,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        let fut = CoapRouter::unregister_device_if_owned(self, device_id, owner);
        async move { fut.await.map_err(|e| format!("{e:?}")) }
    }

    fn observer_count<'a>(&'a self, device_id: &'a str) -> impl Future<Output = usize> + Send + 'a {
        CoapRouter::observer_count(self, device_id)
    }

    fn has_observe_route(&self, path: &str) -> bool {
        CoapRouter::has_observe_route(self, path)
    }

    fn is_confirmable_notify(&self, path: &str) -> bool {
        CoapRouter::is_confirmable_notify(self, path)
    }

    fn emit_device_event(&self, event: DeviceEvent) {
        CoapRouter::emit_device_event(self, event);
    }

    fn emit_block_transfer_event(&self, event: BlockTransferEvent) {
        CoapRouter::emit_block_transfer_event(self, event);
    }
}
