//! Handler system for ergonomic function-based CoAP request handling
//!
//! This module provides the infrastructure for converting regular Rust functions
//! into CoAP handlers that can be used with the router. It supports automatic
//! extraction of parameters from requests and conversion of return values to responses.

use crate::CoapResponse;
use crate::extract::{FromRequest, IntoResponse};
use crate::router::CoapumRequest;
use async_trait::async_trait;
use std::{convert::Infallible, future::Future, marker::PhantomData, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;

/// Trait for handler functions that can be converted to CoAP handlers
///
/// This trait is implemented for functions with various signatures that use
/// extractors to get data from requests and return types that can be converted
/// to CoAP responses.
#[async_trait]
pub trait Handler<T, S>: Clone + Send + Sized + 'static {
    /// The future returned by this handler
    type Future: Future<Output = Result<CoapResponse, Infallible>> + Send + 'static;

    /// Call this handler with the given request and state
    fn call(self, req: CoapumRequest<SocketAddr>, state: Arc<RwLock<S>>) -> Self::Future;
}

/// Wrapper for converting handler functions to the Handler trait
pub struct HandlerFn<F, S> {
    f: F,
    _marker: PhantomData<S>,
}

impl<F, S> HandlerFn<F, S> {
    /// Create a new handler function wrapper
    pub fn new(f: F) -> Self {
        Self {
            f,
            _marker: PhantomData,
        }
    }
}

impl<F, S> Clone for HandlerFn<F, S>
where
    F: Clone,
{
    fn clone(&self) -> Self {
        Self {
            f: self.f.clone(),
            _marker: PhantomData,
        }
    }
}

/// Implementation for handlers with no extractors
#[async_trait]
impl<F, Fut, Res, S> Handler<(), S> for HandlerFn<F, S>
where
    F: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Res> + Send + 'static,
    Res: IntoResponse + Send + 'static,
    S: Send + Sync + 'static,
{
    type Future = std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>;

    fn call(self, _req: CoapumRequest<SocketAddr>, _state: Arc<RwLock<S>>) -> Self::Future {
        Box::pin(async move {
            let result = (self.f)().await;
            Ok(result.into_response().unwrap_or_else(|e| {
                tracing::error!("Response conversion failed: {}", e);
                crate::extract::StatusCode::InternalServerError
                    .into_response()
                    .unwrap()
            }))
        })
    }
}

/// Implementation for handlers with one extractor
#[async_trait]
impl<F, Fut, Res, T1, S> Handler<(T1,), S> for HandlerFn<F, S>
where
    F: Fn(T1) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Res> + Send + 'static,
    Res: IntoResponse + Send + 'static,
    T1: FromRequest<S> + Send + 'static,
    T1::Rejection: Send + 'static,
    S: Send + Sync + 'static,
{
    type Future = std::pin::Pin<Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>>;

    fn call(self, req: CoapumRequest<SocketAddr>, state: Arc<RwLock<S>>) -> Self::Future {
        Box::pin(async move {
            let state_guard = state.read().await;
            let t1 = match T1::from_request(&req, &*state_guard).await {
                Ok(val) => val,
                Err(rejection) => {
                    return Ok(rejection.into_response().unwrap_or_else(|e| {
                        tracing::error!("Rejection response conversion failed: {}", e);
                        crate::extract::StatusCode::BadRequest
                            .into_response()
                            .unwrap()
                    }));
                }
            };
            drop(state_guard);

            let result = (self.f)(t1).await;
            Ok(result.into_response().unwrap_or_else(|e| {
                tracing::error!("Response conversion failed: {}", e);
                crate::extract::StatusCode::InternalServerError
                    .into_response()
                    .unwrap()
            }))
        })
    }
}

/// Generate [`Handler`] impls for functions with 2–9 extractor parameters.
///
/// Each invocation expands to the full `impl` block, keeping the generated
/// code identical to the hand-written versions while eliminating the
/// copy-paste boilerplate.
macro_rules! impl_handler_for_tuple {
    // Match a comma-separated list of one or more type parameters.
    ($($T:ident),+) => {
        #[async_trait]
        #[allow(non_snake_case)]
        impl<F, Fut, Res, S, $($T),+> Handler<($($T,)+), S> for HandlerFn<F, S>
        where
            F: Fn($($T),+) -> Fut + Clone + Send + Sync + 'static,
            Fut: Future<Output = Res> + Send + 'static,
            Res: IntoResponse + Send + 'static,
            $(
                $T: FromRequest<S> + Send + 'static,
                $T::Rejection: Send + 'static,
            )+
            S: Send + Sync + 'static,
        {
            type Future = std::pin::Pin<
                Box<dyn Future<Output = Result<CoapResponse, Infallible>> + Send>
            >;

            fn call(
                self,
                req: CoapumRequest<SocketAddr>,
                state: Arc<RwLock<S>>,
            ) -> Self::Future {
                Box::pin(async move {
                    let state_guard = state.read().await;

                    $(
                        let $T = match $T::from_request(&req, &*state_guard).await {
                            Ok(val) => val,
                            Err(rejection) => {
                                return Ok(rejection.into_response().unwrap_or_else(|e| {
                                    tracing::error!(
                                        "Rejection response conversion failed: {}",
                                        e
                                    );
                                    crate::extract::StatusCode::BadRequest
                                        .into_response()
                                        .unwrap()
                                }));
                            }
                        };
                    )+

                    drop(state_guard);

                    let result = (self.f)($($T),+).await;
                    Ok(result.into_response().unwrap_or_else(|e| {
                        tracing::error!("Response conversion failed: {}", e);
                        crate::extract::StatusCode::InternalServerError
                            .into_response()
                            .unwrap()
                    }))
                })
            }
        }
    };
}

// 2–9 extractor parameters
impl_handler_for_tuple!(T1, T2);
impl_handler_for_tuple!(T1, T2, T3);
impl_handler_for_tuple!(T1, T2, T3, T4);
impl_handler_for_tuple!(T1, T2, T3, T4, T5);
impl_handler_for_tuple!(T1, T2, T3, T4, T5, T6);
impl_handler_for_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_handler_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);
impl_handler_for_tuple!(T1, T2, T3, T4, T5, T6, T7, T8, T9);

/// Convert a function into a handler
pub fn into_handler<F, T, S>(f: F) -> HandlerFn<F, S>
where
    HandlerFn<F, S>: Handler<T, S>,
{
    HandlerFn::new(f)
}

/// Type-erased handler trait for storing handlers with different extractor types
///
/// This trait allows the router to store handlers with different type parameters
/// in the same collection while preserving the ability to call them with
/// CoapumRequest and state.
#[async_trait]
pub trait ErasedHandler<S>: Send + Sync + 'static {
    /// Call this handler with the given request and state
    async fn call_erased(
        &self,
        req: CoapumRequest<SocketAddr>,
        state: Arc<RwLock<S>>,
    ) -> Result<CoapResponse, Infallible>;

    /// Clone this handler
    fn clone_erased(&self) -> Box<dyn ErasedHandler<S>>;
}

/// Wrapper for storing handlers in type-erased form
/// Generic wrapper that erases any handler implementing [`Handler<T, S>`].
///
/// This replaces the previous `ErasedHandlerWrapper` / `HandlerFnErasedWrapper`
/// split that had a dead-code fallback returning `NotImplemented`.
pub struct ErasedHandlerWrapper<H, T, S> {
    handler: H,
    _phantom: std::marker::PhantomData<(T, S)>,
}

impl<H, T, S> ErasedHandlerWrapper<H, T, S> {
    pub fn new(handler: H) -> Self {
        Self {
            handler,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[async_trait]
impl<H, T, S> ErasedHandler<S> for ErasedHandlerWrapper<H, T, S>
where
    H: Handler<T, S> + Clone + Send + Sync + 'static,
    T: Send + Sync + 'static,
    S: Send + Sync + 'static,
{
    async fn call_erased(
        &self,
        req: CoapumRequest<SocketAddr>,
        state: Arc<RwLock<S>>,
    ) -> Result<CoapResponse, Infallible> {
        self.handler.clone().call(req, state).await
    }

    fn clone_erased(&self) -> Box<dyn ErasedHandler<S>> {
        Box::new(ErasedHandlerWrapper {
            handler: self.handler.clone(),
            _phantom: std::marker::PhantomData,
        })
    }
}

impl<H, T, S> Clone for ErasedHandlerWrapper<H, T, S>
where
    H: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            _phantom: std::marker::PhantomData,
        }
    }
}

/// Convert a [`HandlerFn`] into a type-erased handler for storage in the router.
pub fn into_erased_handler<F, T, S>(handler: HandlerFn<F, S>) -> Box<dyn ErasedHandler<S>>
where
    HandlerFn<F, S>: Handler<T, S>,
    F: Clone + Send + Sync + 'static,
    T: Send + Sync + 'static,
    S: Send + Sync + 'static,
{
    Box::new(ErasedHandlerWrapper::new(handler))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{Identity, StatusCode};
    use crate::{CoapRequest, Packet};
    use std::net::{Ipv4Addr, SocketAddrV4};

    fn create_test_request() -> CoapumRequest<SocketAddr> {
        let request = CoapRequest::from_packet(
            Packet::new(),
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        );
        let mut coap_request: CoapumRequest<SocketAddr> = request.into();
        coap_request.identity = "test_client".to_string();
        coap_request
    }

    #[tokio::test]
    async fn test_no_extractor_handler() {
        async fn simple_handler() -> StatusCode {
            StatusCode::Valid
        }

        let handler = into_handler(simple_handler);
        let req = create_test_request();
        let state = Arc::new(RwLock::new(()));

        let response = handler.call(req, state).await.unwrap();
        assert_eq!(*response.get_status(), coap_lite::ResponseType::Valid);
    }

    #[tokio::test]
    async fn test_single_extractor_handler() {
        async fn identity_handler(Identity(id): Identity) -> StatusCode {
            assert_eq!(id, "test_client");
            StatusCode::Valid
        }

        let handler = into_handler(identity_handler);
        let req = create_test_request();
        let state = Arc::new(RwLock::new(()));

        let response = handler.call(req, state).await.unwrap();
        assert_eq!(*response.get_status(), coap_lite::ResponseType::Valid);
    }

    #[tokio::test]
    async fn test_multiple_extractor_handler() {
        use crate::extract::{ObserveFlag, Source};

        async fn multi_handler(
            Identity(id): Identity,
            Source(addr): Source,
            ObserveFlag(_observe): ObserveFlag,
        ) -> StatusCode {
            assert_eq!(id, "test_client");
            assert_eq!(addr.port(), 0);
            StatusCode::Valid
        }

        let handler = into_handler(multi_handler);
        let req = create_test_request();
        let state = Arc::new(RwLock::new(()));

        let response = handler.call(req, state).await.unwrap();
        assert_eq!(*response.get_status(), coap_lite::ResponseType::Valid);
    }

    #[tokio::test]
    async fn test_erased_handler() {
        async fn simple_handler() -> StatusCode {
            StatusCode::Valid
        }

        let handler = into_handler(simple_handler);
        let erased = into_erased_handler(handler);
        let req = create_test_request();
        let state = Arc::new(RwLock::new(()));

        let response = erased.call_erased(req, state).await.unwrap();
        assert_eq!(*response.get_status(), coap_lite::ResponseType::Valid);
    }
}
