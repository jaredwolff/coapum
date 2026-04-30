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
    time::Duration,
};

use rand::RngExt;
use tokio::{
    sync::{Mutex, Notify, mpsc},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::Error;

use super::{ConnectionInfo, DisconnectMode};

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
pub struct ServerHandle {
    pub(super) shutdown: CancellationToken,
    /// Cancelled when the accept loop exits. Using a token (vs Notify)
    /// gives "fire-once, stays signaled" semantics so callers can reach
    /// `drained()` after the loop has already finished without a missed
    /// wakeup.
    pub(super) accept_done: CancellationToken,
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

    /// Snapshot of currently-tracked sessions as [`SessionHandle`]s.
    ///
    /// Each handle owns a clone of the per-session disconnect channel, so
    /// holding one keeps the underlying mpsc::Sender alive but does not
    /// extend the connection task's lifetime.
    pub async fn sessions(&self) -> Vec<SessionHandle> {
        let guard = self.connections.lock().await;
        guard
            .iter()
            .map(|(id, info)| SessionHandle {
                id: SessionId(id.clone()),
                peer_addr: info.source_addr,
                disconnect_tx: info.sender.clone(),
                cleanup_notify: self.cleanup_notify.clone(),
                connections: self.connections.clone(),
            })
            .collect()
    }

    /// Wait until the accept loop has stopped *and* all in-flight
    /// connection tasks have completed cleanup.
    ///
    /// Cancel [`shutdown_token`](Self::shutdown_token) (or call
    /// [`close_all_graceful`](Self::close_all_graceful)) first; otherwise
    /// this call hangs while the server keeps accepting.
    pub async fn drained(&self) {
        self.accept_done.cancelled().await;
        loop {
            if self.active_count.load(Ordering::Relaxed) == 0 {
                return;
            }
            // Double-check pattern: register interest, re-check the count,
            // then await. The count is decremented before
            // cleanup_notify.notify_waiters() in connection_task cleanup,
            // so if we observe 0 here we are guaranteed not to miss the
            // wakeup.
            let notified = self.cleanup_notify.notified();
            if self.active_count.load(Ordering::Relaxed) == 0 {
                return;
            }
            notified.await;
        }
    }

    /// Send a graceful close to every active session, dispatching each
    /// at a uniformly-random delay in `[0, jitter)` to spread the
    /// reconnect storm. Returns when every per-session `close_graceful`
    /// has resolved.
    ///
    /// `jitter == Duration::ZERO` skips the random delay and dispatches
    /// every close immediately.
    pub async fn close_all_graceful(&self, jitter: Duration) {
        let sessions = self.sessions().await;
        if sessions.is_empty() {
            return;
        }
        let jitter_us = jitter.as_micros();
        let mut delays = Vec::with_capacity(sessions.len());
        if jitter_us == 0 {
            delays.resize(sessions.len(), Duration::ZERO);
        } else {
            // Cap jitter at u64::MAX micros (~584K years) to keep the
            // RNG range type-safe.
            let upper = u64::try_from(jitter_us).unwrap_or(u64::MAX);
            let mut rng = rand::rng();
            for _ in 0..sessions.len() {
                delays.push(Duration::from_micros(rng.random_range(0..upper)));
            }
        }
        let futs = sessions
            .into_iter()
            .zip(delays)
            .map(|(s, delay)| async move {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                s.close_graceful().await;
            });
        futures::future::join_all(futs).await;
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

/// Handle to a single live DTLS session.
///
/// Returned by [`ServerHandle::sessions`]. Use [`close_graceful`] to send a
/// DTLS `close_notify` Alert and wait for the connection task to fully
/// unwind (observer unregistration, `DeviceEvent::Disconnected`, etc.).
///
/// The handle is a snapshot: if the underlying session ends before
/// `close_graceful` is called, the call returns immediately on the first
/// connections-map check.
///
/// [`close_graceful`]: SessionHandle::close_graceful
pub struct SessionHandle {
    pub(super) id: SessionId,
    pub(super) peer_addr: SocketAddr,
    pub(super) disconnect_tx: mpsc::Sender<DisconnectMode>,
    pub(super) cleanup_notify: Arc<Notify>,
    pub(super) connections: Arc<Mutex<HashMap<String, ConnectionInfo>>>,
}

impl SessionHandle {
    /// Stable identifier (PSK identity) for this session.
    pub fn id(&self) -> &SessionId {
        &self.id
    }

    /// Snapshot of the peer address taken at session establishment. With
    /// RFC 9146 Connection IDs the live remote may differ.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// Send a graceful close (DTLS `close_notify` Alert) and wait for the
    /// connection task to fully unwind.
    ///
    /// Idempotent: returns immediately if the session is already gone. Uses
    /// the standard `Notify` double-check pattern to avoid lost wakeups
    /// between the disconnect signal and the cleanup notification.
    pub async fn close_graceful(&self) {
        let _ = self.disconnect_tx.send(DisconnectMode::Graceful).await;
        loop {
            if !self.connections.lock().await.contains_key(self.id.as_str()) {
                return;
            }
            let notified = self.cleanup_notify.notified();
            if !self.connections.lock().await.contains_key(self.id.as_str()) {
                return;
            }
            notified.await;
        }
    }
}
