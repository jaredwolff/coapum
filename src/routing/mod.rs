//! Enhanced routing system for ergonomic CoAP handler registration
//!
//! This module provides an improved routing API that works with the new handler system,
//! allowing for more ergonomic registration of handlers with automatic parameter extraction.

use crate::handler::{into_erased_handler, into_handler, Handler, HandlerFn};
use crate::observer::Observer;
use crate::router::wrapper::RouteHandler;
use crate::router::CoapRouter;
use coap_lite::RequestType;
use std::fmt::Debug;

/// Enhanced router builder for ergonomic handler registration
pub struct RouterBuilder<O, S>
where
    S: Clone + Debug + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    router: CoapRouter<O, S>,
}

impl<O, S> RouterBuilder<O, S>
where
    S: Clone + Debug + Send + Sync + 'static,
    O: Observer + Send + Sync + Clone + 'static,
{
    /// Create a new router builder
    pub fn new(state: S, observer: O) -> Self {
        Self {
            router: CoapRouter::new(state, observer),
        }
    }

    /// Add a GET route with an ergonomic handler
    pub fn get<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        let route_handler = RouteHandler {
            handler: into_erased_handler(into_handler(handler)),
            observe_handler: None,
            method: RequestType::Get,
        };
        self.router.add(path, route_handler);
        self
    }

    /// Add a POST route with an ergonomic handler
    pub fn post<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        let route_handler = RouteHandler {
            handler: into_erased_handler(into_handler(handler)),
            observe_handler: None,
            method: RequestType::Post,
        };
        self.router.add(path, route_handler);
        self
    }

    /// Add a PUT route with an ergonomic handler
    pub fn put<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        let route_handler = RouteHandler {
            handler: into_erased_handler(into_handler(handler)),
            observe_handler: None,
            method: RequestType::Put,
        };
        self.router.add(path, route_handler);
        self
    }

    /// Add a DELETE route with an ergonomic handler
    pub fn delete<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        let route_handler = RouteHandler {
            handler: into_erased_handler(into_handler(handler)),
            observe_handler: None,
            method: RequestType::Delete,
        };
        self.router.add(path, route_handler);
        self
    }

    /// Add a route that handles any HTTP method
    pub fn any<F, T>(mut self, path: &str, handler: F) -> Self
    where
        HandlerFn<F, S>: Handler<T, S>,
        F: Send + Sync + Clone,
        T: Send + Sync + 'static,
    {
        let route_handler = RouteHandler {
            handler: into_erased_handler(into_handler(handler)),
            observe_handler: None,
            method: RequestType::UnKnown,
        };
        self.router.add(path, route_handler);
        self
    }

    /// Add an observable GET route with separate handlers for GET and notifications
    pub fn observe<F1, T1, F2, T2>(
        mut self,
        path: &str,
        get_handler: F1,
        notify_handler: F2,
    ) -> Self
    where
        HandlerFn<F1, S>: Handler<T1, S>,
        HandlerFn<F2, S>: Handler<T2, S>,
        F1: Send + Sync + Clone,
        F2: Send + Sync + Clone,
        T1: Send + Sync + 'static,
        T2: Send + Sync + 'static,
    {
        let route_handler = RouteHandler {
            handler: into_erased_handler(into_handler(get_handler)),
            observe_handler: Some(into_erased_handler(into_handler(notify_handler))),
            method: RequestType::Get,
        };
        self.router.add(path, route_handler);
        self
    }

    /// Build the final router
    pub fn build(self) -> CoapRouter<O, S> {
        self.router
    }

    /// Get a mutable reference to the underlying router for advanced usage
    pub fn router_mut(&mut self) -> &mut CoapRouter<O, S> {
        &mut self.router
    }
}

// Old convert_to_old_handler function removed - now using into_erased_handler directly

/// Convenience functions for creating handlers with specific HTTP methods

/// Create a GET handler
pub fn get<F, T, S>(handler: F) -> RouteHandler<S>
where
    HandlerFn<F, S>: Handler<T, S> + Send + Sync + Clone,
    F: Clone + Send + Sync + 'static,
    T: Send + Sync + 'static,
    S: Clone + Send + Sync + 'static,
{
    RouteHandler {
        handler: into_erased_handler(into_handler(handler)),
        observe_handler: None,
        method: RequestType::Get,
    }
}

/// Create a POST handler
pub fn post<F, T, S>(handler: F) -> RouteHandler<S>
where
    HandlerFn<F, S>: Handler<T, S> + Send + Sync + Clone,
    F: Clone + Send + Sync + 'static,
    T: Send + Sync + 'static,
    S: Clone + Send + Sync + 'static,
{
    RouteHandler {
        handler: into_erased_handler(into_handler(handler)),
        observe_handler: None,
        method: RequestType::Post,
    }
}

/// Create a PUT handler
pub fn put<F, T, S>(handler: F) -> RouteHandler<S>
where
    HandlerFn<F, S>: Handler<T, S> + Send + Sync + Clone,
    F: Clone + Send + Sync + 'static,
    T: Send + Sync + 'static,
    S: Clone + Send + Sync + 'static,
{
    RouteHandler {
        handler: into_erased_handler(into_handler(handler)),
        observe_handler: None,
        method: RequestType::Put,
    }
}

/// Create a DELETE handler
pub fn delete<F, T, S>(handler: F) -> RouteHandler<S>
where
    HandlerFn<F, S>: Handler<T, S> + Send + Sync + Clone,
    F: Clone + Send + Sync + 'static,
    T: Send + Sync + 'static,
    S: Clone + Send + Sync + 'static,
{
    RouteHandler {
        handler: into_erased_handler(into_handler(handler)),
        observe_handler: None,
        method: RequestType::Delete,
    }
}

/// Create a handler for any HTTP method
pub fn any<F, T, S>(handler: F) -> RouteHandler<S>
where
    HandlerFn<F, S>: Handler<T, S> + Send + Sync + Clone,
    F: Clone + Send + Sync + 'static,
    T: Send + Sync + 'static,
    S: Clone + Send + Sync + 'static,
{
    RouteHandler {
        handler: into_erased_handler(into_handler(handler)),
        observe_handler: None,
        method: RequestType::UnKnown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{Identity, StatusCode};

    #[derive(Clone, Debug)]
    struct TestState {
        counter: i32,
    }

    impl AsRef<TestState> for TestState {
        fn as_ref(&self) -> &TestState {
            self
        }
    }

    #[tokio::test]
    async fn test_router_builder() {
        async fn test_handler() -> StatusCode {
            StatusCode::Valid
        }

        let state = TestState { counter: 0 };
        let _router = RouterBuilder::new(state, ())
            .get("/test", test_handler)
            .post("/test", test_handler)
            .build();

        // Basic test that the router can be built
    }

    #[tokio::test]
    async fn test_handler_with_extractor() {
        async fn identity_handler(Identity(id): Identity) -> StatusCode {
            // In a real handler, you'd use the identity
            StatusCode::Valid
        }

        let state = TestState { counter: 0 };
        let _router = RouterBuilder::new(state, ())
            .get("/user", identity_handler)
            .build();

        // Basic test that the router can be built with extractors
    }

    #[tokio::test]
    async fn test_observe_handler() {
        async fn get_handler() -> StatusCode {
            StatusCode::Content
        }

        async fn notify_handler() -> StatusCode {
            StatusCode::Valid
        }

        let state = TestState { counter: 0 };
        let _router = RouterBuilder::new(state, ())
            .observe("/observable", get_handler, notify_handler)
            .build();

        // Basic test that observe handlers can be registered
    }
}
