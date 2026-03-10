//! Pluggable credential/PSK storage backends.
//!
//! This module provides the [`CredentialStore`] trait for implementing custom
//! credential storage backends (e.g., PostgreSQL, Redis). See
//! [`memory::MemoryCredentialStore`] for a reference implementation.
//!
//! # Sync PSK Lookup
//!
//! The DTLS handshake requires synchronous PSK lookup via [`CredentialStore::lookup_psk`].
//! Implementations using async backends should maintain an internal sync cache.
//! See the `lookup_psk` documentation for safe patterns.

pub mod memory;
pub mod resolver;

use std::fmt::Debug;
use std::future::Future;

use crate::router::ClientMetadata;

/// Minimum info returned by a PSK lookup.
///
/// # Key Material Security
///
/// PSK key bytes are stored as plain `Vec<u8>` and are **not** zeroized on drop.
/// Key material is cloned during the DTLS handshake flow (`lookup_psk` → `PskResolver::resolve`),
/// and the downstream `dimpl` crate also does not zeroize its copy. Adding `zeroize` to coapum
/// alone would not provide meaningful protection. Production deployments requiring key zeroization
/// should track upstream support in `dimpl`.
#[derive(Debug, Clone)]
pub struct PskEntry {
    /// The pre-shared key bytes.
    ///
    /// **Note:** Not zeroized on drop. See [`PskEntry`] docs for details.
    pub key: Vec<u8>,
    /// Whether this client is enabled for connections.
    pub enabled: bool,
}

/// Full client info returned by [`CredentialStore::get_client`].
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// The client's identity string.
    pub identity: String,
    /// Whether this client is enabled for connections.
    pub enabled: bool,
    /// Client metadata (name, tags, custom fields, etc.).
    pub metadata: ClientMetadata,
}

/// Trait for pluggable credential/PSK storage backends.
///
/// Implement this trait to provide a custom credential storage backend
/// for DTLS PSK authentication. The trait requires both a synchronous
/// [`lookup_psk`](CredentialStore::lookup_psk) method (for the DTLS handshake callback)
/// and async management methods.
///
/// # Example
///
/// ```rust,no_run
/// use coapum::credential::{CredentialStore, PskEntry};
/// use coapum::router::ClientMetadata;
///
/// #[derive(Clone, Debug)]
/// struct MyStore { /* ... */ }
///
/// impl CredentialStore for MyStore {
///     type Error = std::io::Error;
///
///     fn lookup_psk(&self, identity: &str) -> Result<Option<PskEntry>, Self::Error> {
///         // Return cached PSK for the given identity
///         Ok(None)
///     }
///
///     async fn add_client(&self, identity: &str, key: Vec<u8>,
///         metadata: Option<ClientMetadata>) -> Result<(), Self::Error> { Ok(()) }
///     async fn remove_client(&self, identity: &str) -> Result<bool, Self::Error> { Ok(false) }
///     async fn update_key(&self, identity: &str, key: Vec<u8>) -> Result<bool, Self::Error> { Ok(false) }
///     async fn update_metadata(&self, identity: &str,
///         metadata: ClientMetadata) -> Result<bool, Self::Error> { Ok(false) }
///     async fn set_enabled(&self, identity: &str, enabled: bool) -> Result<bool, Self::Error> { Ok(false) }
///     async fn list_clients(&self) -> Result<Vec<String>, Self::Error> { Ok(vec![]) }
/// }
/// ```
pub trait CredentialStore: Clone + Debug + Send + Sync + 'static {
    /// The error type returned by credential operations.
    type Error: Debug + Send + Sync;

    /// Synchronous PSK lookup — called from the DTLS handshake callback.
    ///
    /// This method is invoked from a **synchronous** context during the DTLS
    /// handshake. Implementations **must not** block the async runtime.
    ///
    /// # What NOT To Do
    ///
    /// ```rust,ignore
    /// // WRONG — will deadlock on current_thread runtime, may deadlock on multi-thread:
    /// fn lookup_psk(&self, identity: &str) -> Result<Option<PskEntry>, Self::Error> {
    ///     tokio::runtime::Handle::current().block_on(self.db.query(identity))
    /// }
    ///
    /// // WRONG — .await is not available in a sync context:
    /// fn lookup_psk(&self, identity: &str) -> Result<Option<PskEntry>, Self::Error> {
    ///     self.async_store.get(identity).await  // compile error
    /// }
    /// ```
    ///
    /// # Recommended Patterns
    ///
    /// - **`std::sync::RwLock`** — used by [`memory::MemoryCredentialStore`].
    ///   Simple and correct for in-memory stores.
    /// - **`DashMap`** — lock-free concurrent reads; best for database-backed
    ///   stores that maintain an in-memory cache refreshed by a background task.
    /// - **`parking_lot::RwLock`** — synchronous lock that does not interact
    ///   with tokio's cooperative scheduling.
    /// - **`tokio::sync::RwLock::blocking_read()`** — works on **multi-threaded
    ///   runtimes only**. Will deadlock on `current_thread` runtimes
    ///   (e.g., `#[tokio::test]` defaults to `current_thread`).
    ///
    /// See [`memory::MemoryCredentialStore`] for a reference implementation.
    fn lookup_psk(&self, identity: &str) -> Result<Option<PskEntry>, Self::Error>;

    /// Add a client with a PSK key and optional metadata.
    fn add_client(
        &self,
        identity: &str,
        key: Vec<u8>,
        metadata: Option<ClientMetadata>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Remove a client. Returns `true` if the client existed.
    fn remove_client(
        &self,
        identity: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    /// Update a client's PSK key. Returns `true` if the client existed.
    fn update_key(
        &self,
        identity: &str,
        key: Vec<u8>,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    /// Update client metadata. Returns `true` if the client existed.
    fn update_metadata(
        &self,
        identity: &str,
        metadata: ClientMetadata,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    /// Enable or disable a client. Returns `true` if the client existed.
    fn set_enabled(
        &self,
        identity: &str,
        enabled: bool,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    /// List all registered client identities.
    fn list_clients(&self) -> impl Future<Output = Result<Vec<String>, Self::Error>> + Send;

    /// Get full client info by identity.
    ///
    /// Returns `Ok(None)` if the client doesn't exist. The default implementation
    /// always returns `Ok(None)` — override this to expose stored metadata.
    fn get_client(
        &self,
        _identity: &str,
    ) -> impl Future<Output = Result<Option<ClientInfo>, Self::Error>> + Send {
        std::future::ready(Ok(None))
    }
}
