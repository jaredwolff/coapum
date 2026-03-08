//! Pluggable credential/PSK storage backends.
//!
//! This module provides the [`CredentialStore`] trait for implementing custom
//! credential storage backends (e.g., PostgreSQL, Redis). See
//! [`memory::MemoryCredentialStore`] for a reference implementation.
//!
//! # Sync PSK Lookup
//!
//! The DTLS handshake requires synchronous PSK lookup via [`CredentialStore::lookup_psk`].
//! Implementations using async backends should maintain an internal sync cache
//! or use `tokio::runtime::Handle::current().block_on()`.

pub mod memory;

use std::fmt::Debug;
use std::future::Future;

use crate::router::ClientMetadata;

/// Minimum info returned by a PSK lookup.
#[derive(Debug, Clone)]
pub struct PskEntry {
    /// The pre-shared key bytes.
    pub key: Vec<u8>,
    /// Whether this client is enabled for connections.
    pub enabled: bool,
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
    /// Implementations using async backends (e.g., PostgreSQL) should maintain
    /// an internal sync cache or use `tokio::runtime::Handle::current().block_on()`.
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
}
