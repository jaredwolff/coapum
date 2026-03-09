//! PSK resolver implementations bridging [`CredentialStore`] to dimpl's [`PskResolver`].

use std::collections::HashMap;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::Mutex;

use dimpl::PskResolver;

use super::CredentialStore;

/// A [`PskResolver`] that wraps a [`CredentialStore`] and captures the last
/// resolved identity for extraction after handshake completion.
///
/// dimpl calls [`PskResolver::resolve`] during the DTLS handshake but does not
/// expose the peer's identity on the connection afterward. This wrapper captures
/// the identity so it can be read via [`take_last_identity`](Self::take_last_identity)
/// when `Output::Connected` is received.
///
/// Each connection task creates its own `CapturingResolver`, so the [`Mutex`]
/// is effectively uncontended — `resolve()` is called synchronously inside
/// `handle_packet()`, then read via [`take_last_identity`](Self::take_last_identity)
/// on `Output::Connected` within the same task.
pub struct CapturingResolver<C> {
    store: C,
    last_identity: Mutex<Option<String>>,
}

impl<C> UnwindSafe for CapturingResolver<C> {}
impl<C> RefUnwindSafe for CapturingResolver<C> {}

impl<C: CredentialStore> CapturingResolver<C> {
    /// Create a new capturing resolver wrapping the given credential store.
    pub fn new(store: C) -> Self {
        Self {
            store,
            last_identity: Mutex::new(None),
        }
    }

    /// Take the last successfully resolved identity.
    ///
    /// Returns `Some(identity)` if a PSK was resolved since the last call,
    /// or `None` if no resolution occurred.
    pub fn take_last_identity(&self) -> Option<String> {
        self.last_identity.lock().unwrap().take()
    }

    /// Get a reference to the underlying credential store.
    pub fn store(&self) -> &C {
        &self.store
    }
}

impl<C: CredentialStore> PskResolver for CapturingResolver<C> {
    fn resolve(&self, identity: &[u8]) -> Option<Vec<u8>> {
        let hint_str = String::from_utf8(identity.to_vec()).ok()?;

        match self.store.lookup_psk(&hint_str) {
            Ok(Some(entry)) if entry.enabled => {
                tracing::info!(identity = %hint_str, "auth.psk_found");
                *self.last_identity.lock().unwrap() = Some(hint_str);
                Some(entry.key)
            }
            Ok(Some(_)) => {
                tracing::warn!(identity = %hint_str, "auth.failed.disabled");
                None
            }
            Ok(None) => {
                tracing::warn!(identity = %hint_str, "auth.failed.not_found");
                None
            }
            Err(e) => {
                tracing::error!(identity = %hint_str, error = ?e, "auth.failed.store_error");
                None
            }
        }
    }
}

/// A simple [`PskResolver`] backed by a static `HashMap`.
///
/// Useful for examples and tests that don't need a full [`CredentialStore`].
pub struct MapResolver {
    keys: HashMap<String, Vec<u8>>,
}

impl UnwindSafe for MapResolver {}
impl RefUnwindSafe for MapResolver {}

impl MapResolver {
    /// Create a new resolver from a map of identity → PSK key.
    pub fn new(keys: HashMap<String, Vec<u8>>) -> Self {
        Self { keys }
    }
}

impl PskResolver for MapResolver {
    fn resolve(&self, identity: &[u8]) -> Option<Vec<u8>> {
        let hint = String::from_utf8(identity.to_vec()).ok()?;
        self.keys.get(&hint).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::memory::MemoryCredentialStore;

    #[test]
    fn capturing_resolver_resolves_and_captures() {
        // Add a client synchronously via the store's internal API
        let mut clients = HashMap::new();
        clients.insert("device1".to_string(), b"secret123".to_vec());
        let store = MemoryCredentialStore::from_clients(&clients);
        let resolver = CapturingResolver::new(store);

        let key = resolver.resolve(b"device1");
        assert!(key.is_some());
        assert_eq!(key.unwrap(), b"secret123");

        let identity = resolver.take_last_identity();
        assert_eq!(identity, Some("device1".to_string()));

        // Second call returns None (already taken)
        assert_eq!(resolver.take_last_identity(), None);
    }

    #[test]
    fn capturing_resolver_returns_none_for_unknown() {
        let store = MemoryCredentialStore::new();
        let resolver = CapturingResolver::new(store);

        let key = resolver.resolve(b"unknown");
        assert!(key.is_none());
        assert_eq!(resolver.take_last_identity(), None);
    }

    #[test]
    fn map_resolver_works() {
        let mut keys = HashMap::new();
        keys.insert("dev1".to_string(), b"key1".to_vec());
        let resolver = MapResolver::new(keys);

        assert_eq!(resolver.resolve(b"dev1"), Some(b"key1".to_vec()));
        assert_eq!(resolver.resolve(b"unknown"), None);
    }
}
