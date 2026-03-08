# Custom Storage Backends

coapum provides two pluggable traits for implementing custom storage backends:

- **`CredentialStore`** — PSK credential storage and client management
- **`Observer`** — Device state storage and push notifications

Both traits are designed for external implementation, allowing you to back them with PostgreSQL, Redis, or any other storage system.

## CredentialStore

The `CredentialStore` trait handles DTLS PSK authentication and client lifecycle management.

### Key constraint: synchronous PSK lookup

The DTLS handshake requires a **synchronous** PSK callback. The `lookup_psk()` method is deliberately not async. If your backend is async (e.g., PostgreSQL), you should maintain an internal sync cache and update it when clients are added/removed.

### Implementing a custom store

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use coapum::credential::{CredentialStore, PskEntry};
use coapum::ClientMetadata;

#[derive(Clone, Debug)]
struct PgCredentialStore {
    pool: PgPool,
    // Sync cache for the DTLS handshake callback
    cache: Arc<RwLock<HashMap<String, PskEntry>>>,
}

impl PgCredentialStore {
    async fn new(pool: PgPool) -> Result<Self, sqlx::Error> {
        // Pre-populate cache from database
        let rows = sqlx::query!("SELECT identity, key, enabled FROM clients")
            .fetch_all(&pool)
            .await?;

        let mut cache = HashMap::new();
        for row in rows {
            cache.insert(row.identity, PskEntry {
                key: row.key,
                enabled: row.enabled,
            });
        }

        Ok(Self {
            pool,
            cache: Arc::new(RwLock::new(cache)),
        })
    }
}

impl CredentialStore for PgCredentialStore {
    type Error = sqlx::Error;

    fn lookup_psk(&self, identity: &str) -> Result<Option<PskEntry>, Self::Error> {
        // Synchronous — reads from the in-memory cache
        Ok(self.cache.blocking_read().get(identity).cloned())
    }

    async fn add_client(
        &self,
        identity: &str,
        key: Vec<u8>,
        metadata: Option<ClientMetadata>,
    ) -> Result<(), Self::Error> {
        // Persist to database
        sqlx::query!("INSERT INTO clients (identity, key, enabled) VALUES ($1, $2, $3)",
            identity, &key, true)
            .execute(&self.pool)
            .await?;

        // Update sync cache
        self.cache.write().await.insert(identity.to_string(), PskEntry {
            key,
            enabled: true,
        });
        Ok(())
    }

    async fn remove_client(&self, identity: &str) -> Result<bool, Self::Error> {
        let result = sqlx::query!("DELETE FROM clients WHERE identity = $1", identity)
            .execute(&self.pool)
            .await?;
        self.cache.write().await.remove(identity);
        Ok(result.rows_affected() > 0)
    }

    async fn update_key(&self, identity: &str, key: Vec<u8>) -> Result<bool, Self::Error> {
        let result = sqlx::query!("UPDATE clients SET key = $1 WHERE identity = $2", &key, identity)
            .execute(&self.pool)
            .await?;
        if let Some(entry) = self.cache.write().await.get_mut(identity) {
            entry.key = key;
        }
        Ok(result.rows_affected() > 0)
    }

    async fn update_metadata(
        &self,
        identity: &str,
        metadata: ClientMetadata,
    ) -> Result<bool, Self::Error> {
        // Store metadata fields in your schema as needed
        Ok(true)
    }

    async fn set_enabled(&self, identity: &str, enabled: bool) -> Result<bool, Self::Error> {
        let result = sqlx::query!("UPDATE clients SET enabled = $1 WHERE identity = $2", enabled, identity)
            .execute(&self.pool)
            .await?;
        if let Some(entry) = self.cache.write().await.get_mut(identity) {
            entry.enabled = enabled;
        }
        Ok(result.rows_affected() > 0)
    }

    async fn list_clients(&self) -> Result<Vec<String>, Self::Error> {
        let rows = sqlx::query_scalar!("SELECT identity FROM clients")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }
}
```

### Wiring it up

Use `serve_with_credential_store` to plug in your custom store:

```rust
use coapum::{RouterBuilder, config::Config};
use coapum::serve::serve_with_credential_store;
use coapum::observer::memory::MemObserver;

let pool = PgPool::connect("postgres://...").await?;
let credentials = PgCredentialStore::new(pool).await?;

let state = AppState { /* ... */ };
let observer = MemObserver::new();
let router = RouterBuilder::new(state, observer)
    .get("/temperature", handle_temperature)
    .build();

let config = Config::default();

// The credential store handles PSK lookup directly —
// no ClientManager needed
serve_with_credential_store(
    "0.0.0.0:5684".to_string(),
    config,
    router,
    credentials,
).await?;
```

### Built-in: MemoryCredentialStore

For development and testing, use `MemoryCredentialStore`:

```rust
use coapum::MemoryCredentialStore;

// Empty store
let store = MemoryCredentialStore::new();

// Pre-populated from a HashMap<String, Vec<u8>>
let store = MemoryCredentialStore::from_clients(&initial_clients);
```

This is also what `serve_with_client_management` uses internally.

## Observer

The `Observer` trait handles device state storage and push notification delivery (CoAP Observe / RFC 7641).

### Implementing a custom observer

```rust
use std::sync::Arc;
use std::collections::HashMap;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::{RwLock, mpsc::Sender};
use coapum::observer::{Observer, ObserverValue};

#[derive(Clone, Debug)]
struct PgObserver {
    pool: PgPool,
    // Track registered observer channels per device/path
    channels: Arc<RwLock<HashMap<String, HashMap<String, Arc<Sender<ObserverValue>>>>>>,
}

#[async_trait]
impl Observer for PgObserver {
    type Error = sqlx::Error;

    async fn register(
        &mut self,
        device_id: &str,
        path: &str,
        sender: Arc<Sender<ObserverValue>>,
    ) -> Result<(), Self::Error> {
        self.channels.write().await
            .entry(device_id.to_string())
            .or_default()
            .insert(path.to_string(), sender);
        Ok(())
    }

    async fn unregister(&mut self, device_id: &str, path: &str) -> Result<(), Self::Error> {
        let mut channels = self.channels.write().await;
        if let Some(device) = channels.get_mut(device_id) {
            device.remove(path);
            if device.is_empty() {
                channels.remove(device_id);
            }
        }
        Ok(())
    }

    async fn unregister_all(&mut self) -> Result<(), Self::Error> {
        self.channels.write().await.clear();
        Ok(())
    }

    async fn write(
        &mut self,
        device_id: &str,
        path: &str,
        payload: &Value,
    ) -> Result<(), Self::Error> {
        // Persist to database
        sqlx::query!(
            "INSERT INTO device_state (device_id, path, value)
             VALUES ($1, $2, $3)
             ON CONFLICT (device_id, path) DO UPDATE SET value = $3",
            device_id, path, payload
        ).execute(&self.pool).await?;

        // Notify registered observers
        let channels = self.channels.read().await;
        if let Some(device_channels) = channels.get(device_id) {
            if let Some(sender) = device_channels.get(path) {
                let _ = sender.send(ObserverValue {
                    path: path.to_string(),
                    value: payload.clone(),
                }).await;
            }
        }

        Ok(())
    }

    async fn read(
        &mut self,
        device_id: &str,
        path: &str,
    ) -> Result<Option<Value>, Self::Error> {
        let row = sqlx::query_scalar!(
            "SELECT value FROM device_state WHERE device_id = $1 AND path = $2",
            device_id, path
        ).fetch_optional(&self.pool).await?;
        Ok(row)
    }

    async fn clear(&mut self, device_id: &str) -> Result<(), Self::Error> {
        sqlx::query!("DELETE FROM device_state WHERE device_id = $1", device_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

### Wiring it up

Pass your observer to `RouterBuilder`:

```rust
let observer = PgObserver::new(pool).await?;
let router = RouterBuilder::new(state, observer)
    .get("/temperature", handle_temperature)
    .build();
```

### Built-in observers

| Backend | Type | Feature flag |
|---------|------|-------------|
| In-memory | `MemObserver` | (always available) |
| Sled | `SledObserver` | `sled-observer` |
| Redb | `RedbObserver` | `redb-observer` |
| No-op | `()` | (always available) |

## Using both together

For a fully custom storage layer (e.g., everything in PostgreSQL):

```rust
let pool = PgPool::connect("postgres://...").await?;
let credentials = PgCredentialStore::new(pool.clone()).await?;
let observer = PgObserver::new(pool).await?;

let router = RouterBuilder::new(state, observer)
    .get("/temperature", handle_temperature)
    .post("/control", handle_control)
    .build();

serve_with_credential_store(
    "0.0.0.0:5684".to_string(),
    Config::default(),
    router,
    credentials,
).await?;
```

This gives you full control over both device state persistence and credential management through your database, with no in-memory stores involved (aside from the PSK sync cache, which is required by the DTLS handshake).
