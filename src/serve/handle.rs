//! Handles for driving a running server: cancellation, session enumeration,
//! and graceful drain.
//!
//! `ServerHandle` is returned by [`bind_and_spawn`](super::bind_and_spawn) and
//! is the primary surface lion-server (and other downstream callers) use to
//! coordinate shutdown.
//!
//! [`SessionHandle`](super::SessionHandle) and the per-session graceful close
//! are added on top of this scaffolding in subsequent commits.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use tokio::{
    sync::{Mutex, Notify},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::Error;

use super::ConnectionInfo;

/// Stable identifier for a DTLS session, cloned from the PSK identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub(super) String);

impl SessionId {
    /// Borrow the underlying identity string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume into the owned identity string.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Driver for a running CoAP/DTLS server.
///
/// Returned by [`bind_and_spawn`](super::bind_and_spawn). Cancelling
/// [`shutdown_token`](Self::shutdown_token) stops the accept loop without
/// disturbing established sessions; existing connection tasks continue to
/// serve traffic until they idle out, are closed individually via the
/// per-session API (commit 6), or drop with the handle.
///
/// Dropping a `ServerHandle` cancels the accept loop and aborts the
/// background task, so callers that want to await graceful drain must call
/// [`join`](Self::join) before drop.
#[allow(dead_code)] // Some fields are consumed by SessionHandle/drained in commits 6-7.
pub struct ServerHandle {
    pub(super) shutdown: CancellationToken,
    pub(super) accept_done: Arc<Notify>,
    pub(super) cleanup_notify: Arc<Notify>,
    pub(super) connections: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
    pub(super) active_count: Arc<AtomicUsize>,
    pub(super) bound_addr: SocketAddr,
    pub(super) join: StdMutex<Option<JoinHandle<Result<(), Error>>>>,
}

impl ServerHandle {
    /// Token that cancels the accept loop. Existing sessions are not
    /// disturbed — the cancel only stops new handshakes from being accepted.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// The bound UDP socket address. Useful for tests that bind to port 0.
    pub fn local_addr(&self) -> SocketAddr {
        self.bound_addr
    }

    /// Approximate count of in-flight connection tasks.
    ///
    /// Includes mid-handshake and established sessions; the value is a
    /// snapshot of an atomic counter and may change between calls.
    pub fn active_session_count(&self) -> usize {
        self.active_count.load(Ordering::Relaxed)
    }

    /// Await the background accept-loop task. Consumes the inner
    /// [`JoinHandle`]; subsequent calls return `Ok(())`.
    pub async fn join(&self) -> Result<(), Error> {
        let handle = self.join.lock().unwrap().take();
        match handle {
            Some(h) => h.await?,
            None => Ok(()),
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown.cancel();
        if let Some(h) = self.join.lock().unwrap().take() {
            h.abort();
        }
    }
}
