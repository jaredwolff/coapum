# Coapum Repository Analysis Prompt

Use this prompt with an AI assistant or as a structured review checklist for the coapum codebase.

---

## Project Context

Coapum is a modern, ergonomic CoAP (Constrained Application Protocol) library for Rust (~5k LoC, 18 source files + 10 in coapum-senml) that provides async request routing, DTLS transport, observer pattern support, and SenML encoding. It follows a modular architecture with distinct subsystems: Router, Handler, Extractor, Observer, Server, and Config. It targets IoT/embedded use cases with constrained devices and uses Tokio, coap-lite, dimpl (sans-IO DTLS 1.2), and route-recognizer. A companion crate `coapum-senml` provides SenML pack encoding/decoding in JSON, CBOR, and XML.

## Analysis Areas

### 1. Architecture & Module Boundaries

- Are module responsibilities cleanly separated (router, handler, extract, observer, serve, config)?
- Does the `Observer` trait properly abstract over storage backends (memory, sled, redb)?
- Is the type erasure in `handler/mod.rs` (Handler -> ErasedHandler) sound and maintainable?
- Are public re-exports in `lib.rs` intentional and minimal — does the public API surface match what downstream users need?
- Is the `RouterBuilder` API ergonomic and hard to misuse? Can invalid states be constructed?
- Review the relationship between `serve_basic`, `serve`, and `serve_with_client_management` — is the layering clean or should these be unified?

### 2. Security Audit

- **DTLS Configuration**: Verify that default DTLS settings (cipher suites, PSK handling) are secure. Are weak cipher suites rejected? Is PSK material handled safely (zeroed after use, not logged)?
- **Path Validation**: Review `validate_observer_path` in `serve.rs` — does it prevent path traversal, null byte injection, and overly long paths? Are all user-supplied paths validated consistently?
- **Buffer Limits**: The buffer size is capped at 64KB (`Config::MAX_BUFFER_SIZE`). Is this enforced everywhere incoming data is read? Could a malformed packet bypass the limit?
- **Connection Rate Limiting**: Review `MIN_RECONNECT_INTERVAL` and `MAX_RECONNECT_ATTEMPTS` in `serve.rs` — are these effective against connection flooding? Can they be bypassed by rotating source addresses?
- **Client Management**: In `ClientManager` / `ClientStore`, can identities be spoofed? Are PSKs validated before use? Is there a risk of TOCTOU between authentication and handler execution?
- **Input Deserialization**: Do the `Json<T>`, `Cbor<T>`, and SenML deserializers impose size limits? Could a crafted payload cause excessive memory allocation or CPU usage (e.g., deeply nested structures, hash collision attacks)?
- **Observer Notifications**: Can a client register observers on paths it shouldn't access? Is there authorization at the observer layer or only at the routing layer?
- **Error Information Leakage**: Do error responses expose internal details (file paths, stack traces, backend errors) that could aid an attacker?

### 3. Error Handling & Robustness

- Are errors from observer backends (sled, redb) properly propagated and mapped to CoAP response codes?
- Do handlers avoid `unwrap()`/`expect()` in request-handling code paths? Search for panics that could crash the server.
- What happens when a DTLS handshake fails mid-connection — is cleanup handled correctly?
- If an observer's sender channel is closed (client disconnected), is the registration cleaned up promptly?
- Review `StatusCode` mapping — are CoAP response codes used correctly per RFC 7252?

### 4. Performance & Resource Management

- **Concurrency**: Are `Arc<Mutex<T>>` usages around shared state (router, observer) potential bottlenecks under high connection counts? Would `RwLock` or lock-free structures be more appropriate?
- **Observer Storage**: For `MemoryObserver`, is there a limit on the number of registrations? Could an attacker exhaust memory by registering many observers?
- **Sled/Redb Backends**: Are database operations blocking the Tokio runtime? Should they use `spawn_blocking`?
- **Channel Backpressure**: What happens when observer notification channels fill up? Is backpressure handled, or are notifications silently dropped?
- **Allocations**: In the hot path (packet receive -> route -> extract -> handle -> respond), are there unnecessary allocations (e.g., cloning `Value`, string allocations in path matching)?
- **Benchmarks**: Does `router_bench.rs` cover realistic workloads? Are there benchmarks for observer operations and serialization?

### 5. CoAP Protocol Compliance (RFC 7252)

- Are all required CoAP response codes supported and used correctly?
- Is the observe mechanism (RFC 7641) implemented correctly — sequence numbers, freshness, deregistration on RST?
- Are message IDs and tokens handled properly for request/response matching?
- Is blockwise transfer (RFC 7959) supported or documented as unsupported?
- Are Content-Format options set correctly for JSON (application/json) and CBOR (application/cbor) payloads?
- Does the server handle confirmable (CON) vs non-confirmable (NON) messages appropriately?

### 6. SenML Compliance (RFC 8428)

- Does `coapum-senml` correctly implement base value resolution (base name, base time, base unit)?
- Is normalization (`normalize.rs`) producing valid resolved records per the RFC?
- Are all required SenML fields supported? Are unknown fields handled correctly (ignored vs rejected)?
- Is CBOR encoding using the correct integer labels per RFC 8428 Section 12?
- Does validation (`validation.rs`) catch all invalid packs (e.g., missing value fields, invalid base names)?
- Are edge cases handled: empty packs, records with only base fields, numeric precision?

### 7. Testing

- With 12 integration test files — what is the coverage of critical paths (DTLS handshake, observer lifecycle, error conditions)?
- Are there tests for malicious/malformed inputs (fuzz-like edge cases)?
- Do observer integration tests cover all three backends (memory, sled, redb)?
- Are there concurrency tests that exercise the server under parallel connections?
- Is SenML round-trip tested (encode -> decode -> compare) across all three formats?
- Are the 9 examples tested in CI (`example_integration_tests.rs`) and kept in sync with API changes?

### 8. Code Quality & Maintainability

- Run `cargo clippy --all-features -- -D warnings` — are there any suppressed or ignored lints?
- Are there large functions (>80 lines) that should be decomposed? (`serve_basic` at ~290 lines is a candidate.)
- Is there code duplication between observer backends (memory, sled, redb) that could be reduced with shared logic?
- Are the handler trait implementations (up to 9 parameters) generated via macro or manually duplicated?
- Is `test_utils.rs` only compiled for tests, or does it leak into production builds?

### 9. API Design & Ergonomics

- Is the extractor pattern (inspired by Axum) discoverable for users unfamiliar with the pattern?
- Can users easily add custom extractors by implementing `FromRequest`?
- Is the `State<T>` extractor safe to use with non-`Send`/`Sync` types, or will it produce confusing compiler errors?
- Are error messages from failed extraction (wrong content type, missing path param) actionable?
- Is the builder API consistent — do all builders follow the same conventions?

### 10. Dependencies & Supply Chain

- Run `cargo audit` for known vulnerabilities.
- Are dependency versions pinned appropriately (not using `*` or overly broad ranges)?
- Review workspace dependency centralization — are all shared deps in `[workspace.dependencies]`?
- Is `dimpl` (DTLS transport) sound? Are there known issues with its DTLS 1.2 implementation?
- Are optional features (`sled-observer`, `redb-observer`) properly gated with `#[cfg(feature = ...)]` everywhere?
- Can any dependency features be trimmed to reduce compile time and binary size?

---

## How to Use This Prompt

Paste this into a conversation with an AI assistant along with relevant source files, or use it as a checklist for manual review. For AI-assisted analysis, feed files in order of priority:

1. `src/lib.rs`, `src/router/mod.rs` — public API and routing core
2. `src/serve.rs` — server implementation and security validation
3. `src/handler/mod.rs` — handler trait and type erasure
4. `src/extract/` — extractor system (payload.rs, state.rs, path.rs)
5. `src/observer/mod.rs`, `src/observer/memory.rs` — observer trait and default backend
6. `src/observer/sled.rs`, `src/observer/redb.rs` — persistent observer backends
7. `src/config/mod.rs` — configuration and validation
8. `coapum-senml/src/` — SenML crate (pack.rs, normalize.rs, validation.rs)
9. `tests/` — integration test coverage
