# Coapum

A modern, ergonomic CoAP (Constrained Application Protocol) library for Rust with support for DTLS, observers, and asynchronous handlers.

[![Crates.io](https://img.shields.io/crates/v/coapum.svg)](https://crates.io/crates/coapum)
[![Documentation](https://docs.rs/coapum/badge.svg)](https://docs.rs/coapum)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

## Features

- 🚀 **Async/await support** - Built on Tokio for high-performance async networking
- 🛡️ **DTLS security** - Full DTLS 1.2 support with PSK authentication
- 🎯 **Ergonomic routing** - Express-like routing with automatic parameter extraction
- 👁️ **Observer pattern** - CoAP observe support with persistent storage backends
- 📦 **Multiple payload formats** - JSON, CBOR, and raw byte support
- 🔧 **Type-safe extractors** - Automatic request parsing with compile-time guarantees
- 🗄️ **Pluggable storage** - Memory and Sled database backends for observers
- 🧪 **Comprehensive testing** - High test coverage with benchmarks

## Quick Start

Add Coapum to your `Cargo.toml`:

```toml
[dependencies]
coapum = "0.2.0"

# For standalone SenML usage
coapum-senml = "0.1.0"
```

### Basic Server

```rust
use coapum::{
    router::RouterBuilder,
    observer::memory::MemObserver,
    serve,
    extract::{Json, Path, StatusCode},
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct DeviceState {
    temperature: f32,
    humidity: f32,
}

// Handler with automatic JSON deserialization and path parameter extraction
async fn update_device(
    Path(device_id): Path<String>,
    Json(state): Json<DeviceState>,
) -> Result<StatusCode, StatusCode> {
    println!("Updating device {}: temp={}°C", device_id, state.temperature);
    Ok(StatusCode::Changed)
}

// Observer handler for device state notifications
async fn get_device_state(Path(device_id): Path<String>) -> Json<DeviceState> {
    Json(DeviceState {
        temperature: 23.5,
        humidity: 45.2,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create router with ergonomic builder API
    let router = RouterBuilder::new((), MemObserver::new())
        .post("/device/:id", update_device)
        .get("/device/:id", get_device_state)
        .observe("/device/:id", get_device_state, get_device_state)
        .build();

    // Start server
    serve::serve("127.0.0.1:5683".to_string(), Default::default(), router).await?;
    Ok(())
}
```

### Secure DTLS Server

```rust
use coapum::{
    MemoryCredentialStore, Raw,
    config::Config,
    observer::memory::MemObserver,
    router::RouterBuilder,
    serve,
};
use std::collections::HashMap;

async fn status() -> Raw {
    Raw { payload: b"OK".to_vec(), content_format: None }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Register device PSK credentials
    let mut clients = HashMap::new();
    clients.insert("device123".to_string(), b"secret_key".to_vec());
    let credential_store = MemoryCredentialStore::from_clients(&clients);

    // Create router
    let router = RouterBuilder::new((), MemObserver::new())
        .get("/status", status)
        .build();

    // Server config with PSK identity hint
    let server_config = Config {
        psk_identity_hint: Some(b"coapum-server".to_vec()),
        ..Default::default()
    };

    serve::serve_with_credential_store(
        "127.0.0.1:5684".to_string(),
        server_config,
        router,
        credential_store,
    ).await?;
    Ok(())
}
```

### Client Example

```rust
use coapum::{
    CoapRequest, Packet, RequestType,
    client::DtlsClient,
    credential::resolver::MapResolver,
};
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure DTLS client with PSK credentials
    let mut keys = HashMap::new();
    keys.insert("device123".to_string(), b"secret_key".to_vec());
    let resolver = Arc::new(MapResolver::new(keys));

    let config = dimpl::Config::builder()
        .with_psk_client(b"device123".to_vec(), resolver as Arc<dyn dimpl::PskResolver>)
        .build()?;

    let mut client = DtlsClient::connect("127.0.0.1:5684", Arc::new(config)).await?;

    // Send CoAP request
    let mut request: CoapRequest<SocketAddr> = CoapRequest::new();
    request.set_method(RequestType::Get);
    request.set_path("status");
    client.send(&request.message.to_bytes()?).await?;

    // Receive response
    let data = client.recv(Duration::from_secs(5)).await?;
    let response = Packet::from_bytes(&data)?;
    println!("Response: {}", String::from_utf8_lossy(&response.payload));
    Ok(())
}
```

## Core Concepts

### Routing

Coapum provides an ergonomic routing system inspired by web frameworks:

```rust
let router = RouterBuilder::new(state, observer)
    .get("/users/:id", get_user)           // GET with path parameter
    .post("/users", create_user)           // POST with JSON body
    .put("/users/:id", update_user)        // PUT with path + body
    .delete("/users/:id", delete_user)     // DELETE
    .observe("/sensors/:id", get_sensor, notify_sensor)  // Observer pattern
    .build();
```

### Extractors

Coapum automatically extracts data from requests using type-safe extractors:

- `Path<T>` - Extract path parameters
- `Json<T>` - Parse JSON payload
- `Cbor<T>` - Parse CBOR payload
- `SenML` - Parse SenML (Sensor Measurement Lists) payload
- `Bytes` - Raw byte payload
- `Raw` - Raw payload data
- `State<T>` - Access shared application state
- `Identity` - Client identity from DTLS
- `ObserveFlag` - CoAP observe option
- `Source` - Request source information

```rust
async fn handler(
    Path(user_id): Path<u32>,           // Extract :id as u32
    Json(user_data): Json<UserData>,    // Parse JSON body
    State(db): State<Database>,         // Access shared state
) -> Result<Json<User>, StatusCode> {
    // Handler logic here
}

// SenML handler example
async fn sensor_handler(
    Path(device_id): Path<String>,      // Extract device ID
    SenML(measurements): SenML,         // Parse SenML payload
) -> Result<StatusCode, StatusCode> {
    println!("Device {}: {} measurements", device_id, measurements.len());
    Ok(StatusCode::Changed)
}
```

### Observer Pattern

CoAP's observe mechanism is fully supported with persistent storage:

```rust
// Register observer endpoint
.observe("/temperature", get_temp, notify_temp)

// Get handler - returns current value
async fn get_temp() -> Json<Temperature> {
    Json(Temperature { value: 23.5 })
}

// Notify handler - called when sending updates to observers
async fn notify_temp() -> Json<Temperature> {
    Json(read_current_temperature())
}
```

### SenML Support

Coapum includes built-in support for Sensor Measurement Lists (SenML) RFC 8428:

```rust
use coapum::extract::SenML;
use coapum_senml::SenMLBuilder;

// Handler accepting SenML sensor data
async fn sensor_data(SenML(measurements): SenML) -> SenML {
    println!("Received {} measurements", measurements.len());
    
    // Create response using SenML builder
    let response = SenMLBuilder::new()
        .base_name("urn:controller/")
        .add_string_value("status", "received")
        .add_value("count", measurements.len() as f64)
        .build();
    
    SenML(response)
}
```

SenML supports multiple formats:
- **JSON** - Standard SenML JSON format
- **CBOR** - Compact binary format for IoT devices
- **XML** - Legacy XML format (with `xml` feature)

### Storage Backends

Choose from multiple observer storage backends:

```rust
// In-memory (for testing/development)
let observer = MemObserver::new();

// Persistent storage with Sled
let observer = SledObserver::new("observers.db").unwrap();
```

## Configuration

### Server Configuration

```rust
use coapum::config::Config;

let config = Config {
    dtls_cfg: dtls_config,
    max_message_size: 1024,
    ack_timeout: Duration::from_secs(2),
    max_retransmit: 4,
    ..Default::default()
};
```

### DTLS Configuration

```rust
use coapum::dtls::config::{Config, ExtendedMasterSecretType};

let dtls_config = Config {
    psk: Some(Arc::new(psk_callback)),
    psk_identity_hint: Some("server".as_bytes().to_vec()),
    cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
    extended_master_secret: ExtendedMasterSecretType::Require,
    ..Default::default()
};
```

## Feature Flags

```toml
[dependencies]
coapum = { version = "0.2.0", features = ["sled-observer"] }
coapum-senml = { version = "0.1.0", features = ["json", "cbor", "xml"] }
```

### Coapum Features
- `sled-observer` - Enable Sled database backend for observers (optional)

### SenML Features  
- `json` - JSON serialization support (default)
- `cbor` - CBOR serialization support (default)
- `xml` - XML serialization support
- `validation` - Input validation support

## Examples

The `examples/` directory contains complete examples:

- `cbor_server.rs` - CBOR payload handling with device state management
- `cbor_client.rs` - CBOR client implementation
- `raw_server.rs` - Raw payload handling
- `raw_client.rs` - Raw client implementation
- `senml_example.rs` - Advanced SenML payload handling with time-series data
- `senml_simple.rs` - Simple SenML payload handling demonstration
- `concurrency.rs` - Concurrent request handling
- `dynamic_client_management.rs` - Dynamic client management example
- `external_state_updates.rs` - External state update handling

Run an example:

```bash
# Start CBOR server
cargo run --example cbor_server

# In another terminal, run client
cargo run --example cbor_client
```

## Testing

Run the test suite:

```bash
# Run all tests
cargo test

# Run with logging
RUST_LOG=debug cargo test

# Run specific test module
cargo test router
```

### Benchmarks

```bash
# Run router benchmarks
cargo bench
```

### Code Coverage

Install `grcov` and generate coverage reports:

```bash
cargo install grcov

# Generate coverage data
CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' \
LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo test

# Generate HTML report
grcov . --binary-path ./target/debug/ -s . -t html \
--branch --ignore-not-existing --ignore "target/*" \
-o target/coverage/

# Generate LCOV report
grcov . --binary-path ./target/debug/ -s . -t lcov \
--branch --ignore-not-existing --ignore "target/*" \
-o target/coverage/tests.lcov
```

## Architecture

Coapum is built with the following principles:

- **Async-first**: Built on Tokio for high-performance async I/O
- **Type safety**: Extensive use of Rust's type system to prevent runtime errors
- **Ergonomics**: API design inspired by modern web frameworks
- **Modularity**: Pluggable components for storage, security, and serialization
- **Performance**: Zero-copy parsing and efficient routing algorithms

### Key Components

- **Router**: Route matching and handler dispatch
- **Extractors**: Type-safe request data extraction
- **Handlers**: Function-based request handling
- **Observers**: CoAP observe pattern implementation
- **DTLS**: Secure transport layer
- **Config**: Server and security configuration

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Setup

```bash
# Clone the repository
git clone https://github.com/username/coapum.git
cd coapum

# Run tests
cargo test

# Run clippy for linting
cargo clippy

# Format code
cargo fmt
```

## License

This project is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Acknowledgments

- Built on the excellent [coap-lite](https://crates.io/crates/coap-lite) library
- DTLS implementation powered by [dimpl](https://github.com/jaredwolff/dimpl) (sans-IO DTLS 1.2)
- Routing powered by [route-recognizer](https://crates.io/crates/route-recognizer)
- Storage backend using [sled](https://crates.io/crates/sled)

---

For more information, see the [API documentation](https://docs.rs/coapum).
