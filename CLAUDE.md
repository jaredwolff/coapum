# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common Development Commands

### Build and Test
```bash
# Build the project
cargo build

# Build in release mode
cargo build --release

# Run all tests
cargo test

# Run tests with logging output
RUST_LOG=debug cargo test

# Run specific test module
cargo test router
cargo test observer

# Run integration tests
cargo test --test observe_integration
cargo test --test observe_push_notifications

# Run a single test function
cargo test test_name -- --exact
```

### Linting and Formatting
```bash
# Run clippy for linting
cargo clippy

# Run clippy with all features
cargo clippy --all-features

# Format code
cargo fmt

# Check formatting without making changes
cargo fmt -- --check
```

### Git Hooks Setup
```bash
# Install pre-commit hooks (runs fmt check, clippy, and tests)
./hooks/install.sh

# Skip hook temporarily if needed
git commit --no-verify
```

### Commits

When writing commits do not include Claude signature.

### Continuous Integration

The project uses GitHub Actions for comprehensive CI/CD:

#### Main CI Pipeline (`.github/workflows/ci.yml`)
- **Multi-version testing**: stable, beta, nightly Rust
- **Feature matrix testing**: default, all features, no features
- **Code formatting**: `cargo fmt --check`
- **Linting**: `cargo clippy` with warnings as errors
- **Security audit**: `cargo audit` for vulnerability scanning
- **Documentation**: Build and deploy docs to GitHub Pages
- **Benchmarks**: Performance regression detection
- **MSRV check**: Minimum Supported Rust Version validation
- **Integration tests**: End-to-end example testing

#### Cross-Platform Testing (`.github/workflows/cross-platform.yml`)
- **OS matrix**: Ubuntu, Windows, macOS
- **Target platforms**: x86_64, aarch64, musl, etc.
- **Cross-compilation**: Using `cross` for various targets

#### Additional Workflows
- **Examples**: Automated testing of example code
- **Performance**: Benchmark tracking and alerts  
- **Release**: Automated releases and changelog generation

### Benchmarks
```bash
# Run router benchmarks
cargo bench
```

### Examples
```bash
# Run the CBOR server example
cargo run --example cbor_server

# Run the CBOR client example
cargo run --example cbor_client

# Run the raw server example
cargo run --example raw_server

# Run the raw client example
cargo run --example raw_client

# Run the concurrency example
cargo run --example concurrency
```

### Code Coverage
```bash
# Generate coverage data
CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo test

# Generate HTML report
grcov . --binary-path ./target/debug/ -s . -t html --branch --ignore-not-existing --ignore "target/*" -o target/coverage/

# Generate LCOV report
grcov . --binary-path ./target/debug/ -s . -t lcov --branch --ignore-not-existing --ignore "target/*" -o target/coverage/tests.lcov
```

## High-Level Architecture

### Core Components

**Router System** (`src/router/`)
- The `CoapRouter` is the central component that manages routes, shared state, and observer database
- Uses `route-recognizer` for efficient path matching with parameter extraction (e.g., `/device/:id`)
- `RouterBuilder` provides an ergonomic API for constructing routers with method-specific handlers
- Routes are stored per HTTP method (GET, POST, PUT, DELETE) and support CoAP observe patterns

**Handler System** (`src/handler/`)
- Handlers are async functions that can accept various extractors and return responses
- `Handler` trait allows functions with different signatures to be converted into a unified interface
- `ErasedHandler` provides type erasure for storing handlers of different types in the router
- Supports up to 9 parameters through specialized implementations

**Extractors** (`src/extract/`)
- Type-safe request data extraction system inspired by web frameworks
- Key extractors:
  - `Path<T>`: Extracts path parameters from routes
  - `Json<T>`: Deserializes JSON payloads
  - `Cbor<T>`: Deserializes CBOR payloads
  - `State<T>`: Accesses shared application state
  - `Identity`: Extracts client identity from DTLS sessions
  - `ObserveFlag`: Detects CoAP observe requests
  - `Bytes`/`Raw`: Access raw payload data

**Observer Pattern** (`src/observer/`)
- Implements CoAP's observe mechanism for push notifications
- Two storage backends:
  - `MemoryObserver`: In-memory storage for development/testing
  - `SledObserver`: Persistent storage using Sled database
- Observer registration tracks device IDs, paths, and sender channels
- Supports automatic deregistration and value updates

**Server** (`src/serve.rs`)
- Main server implementation using Tokio for async I/O
- Handles both plain UDP and DTLS connections
- Manages observer registrations and push notifications
- Integrates with the router for request handling

**DTLS Integration**
- Uses `webrtc-dtls` for secure transport
- Supports PSK (Pre-Shared Key) authentication
- Configurable cipher suites and security parameters

### Request Flow

1. **Connection**: Client connects via UDP or DTLS
2. **Parsing**: Raw bytes parsed into `CoapRequest` using `coap-lite`
3. **Routing**: Router matches path and method to find handler
4. **Extraction**: Request data extracted using type-safe extractors
5. **Handler**: Async handler function processes request
6. **Response**: Handler result converted to `CoapResponse`
7. **Observer**: If observe flag set, client registered for updates

### Key Design Patterns

- **Builder Pattern**: Used extensively (RouterBuilder, Config)
- **Type Erasure**: Handlers use type erasure to store different function signatures
- **Async/Await**: All handlers and I/O operations are async
- **Arc/Mutex**: Shared state managed through Arc<Mutex<T>>
- **Channel-based**: Observer updates use Tokio channels

### Testing Structure

- Unit tests in module files (e.g., `src/router/mod.rs`)
- Integration tests in `tests/` directory focusing on observer functionality
- Examples serve as both documentation and integration tests
- Benchmarks for router performance in `benches/`
