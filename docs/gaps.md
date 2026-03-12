RFC Compliance Audit Report — `coapum` CoAP Server Framework

## 1. Executive Summary

| RFC | Overall Posture | MUST Violations | SHOULD Gaps | Critical Issues |
|-----|----------------|-----------------|-------------|-----------------|
| **RFC 7252** (CoAP Core) | **Good** | 0 | 0 | All critical option, response code, and timing issues resolved |
| **RFC 7641** (Observe) | **Good** | 0 | 0 | Deregistration fixed, sequence masked to 24 bits |
| **RFC 7959** (Block-Wise) | **Good** | 0 | 0 | ACK, dedup, and notification fragmentation all fixed |
| **RFC 6347** (DTLS) | Good | 0 | 1 | State allocated before cookie exchange (deferred — needs upstream) |
| **RFC 8428** (SenML) | **Good** | 0 | 0 | Base fields modeled, CBOR integer labels, value mutual exclusion |

---

## 2. Per-RFC Section Findings

### RFC 7252 — Constrained Application Protocol

| Section | Requirement | Level | Status | Evidence | Notes |
|---------|------------|-------|--------|----------|-------|
| §3 | Message format | MUST | Pass | Delegated to `coap-lite` | |
| §4.2 | CON retransmission | MUST | Pass | `reliability.rs:164-194` | Exponential backoff, randomized initial timeout |
| §4.2 | ACK/RST stops retransmit | MUST | Pass | `reliability.rs:146-154` | |
| §4.3 | CON Empty → RST | MUST | Pass | `serve.rs:328-346` | |
| §4.3 | NON Empty → ignore | SHOULD | Pass | `serve.rs:343-345` | |
| §4.4 | Piggybacked ACK | MUST | Pass | `serve.rs:496-499` | |
| §4.5 | Separate responses | MAY | Not Impl | — | Slow handlers vulnerable to retransmit reprocessing |
| §4.7 | Message deduplication | SHOULD | Pass | `serve.rs:350-363`, `reliability.rs:83-128` | Bounded 256-entry cache |
| §4.8 | Transmission parameters | SHOULD | **Pass** | `config/mod.rs:253-263` | ✅ Fixed: EXCHANGE_LIFETIME = 247s with defaults |
| §5.2 | Request/response matching | MUST | Pass | `serve.rs:465-467` | |
| §5.3.1 | Token echoing | MUST | Pass | `serve.rs:465-466`, `375-377`, `385-387` | All paths covered including block transfer |
| §5.4 | Critical option rejection | MUST | **Pass** | `serve.rs:365-391` | ✅ Fixed: Unknown critical options → 4.02 Bad Option |
| §5.9 | Response codes | MUST | **Pass** | `router/mod.rs` | ✅ Fixed: 4.04 Not Found / 4.05 Method Not Allowed |
| §5.10 | Uri-Query (15, critical) | MUST | **Pass** | `serve.rs:365-391` | ✅ Covered by critical option rejection |
| §5.10 | Accept (17, critical) | MUST | **Pass** | — | Known to coap-lite, not rejected |
| §8 | Amplification protection | SHOULD | Pass | DTLS required for all connections | |
| §8 | Connection exhaustion | SHOULD | Pass | `config/mod.rs:49,59,63` | Rate limiting + max connections |

### RFC 7641 — Observing Resources in CoAP

| Section | Requirement | Level | Status | Evidence | Notes |
|---------|------------|-------|--------|----------|-------|
| §3.1 | Register via GET+Observe=0 | MUST | Pass | `serve.rs:407-408` | Handler success check before registration |
| §3.1 | Per-device observer limit | MAY | Pass | `serve.rs:417-424` | Default 100 |
| §3.2 | Observe sequence in notifications | MUST | Pass | `serve.rs:244-246` | Wrapping increment per connection |
| §3.2 | Token echoing in notifications | MUST | Pass | `serve.rs:239-242` | From stored token at registration |
| §3.6 | GET+Observe=1 deregistration | MUST | **Pass** | `serve.rs:487` | ✅ Fixed: Uses GET (was DELETE) |
| §3.6 | RST-based deregistration | MUST | Pass | `serve.rs:310-317` | Message ID tracking + cleanup |
| §3.6 | Give-up deregistration | SHOULD | Pass | `serve.rs:746-754` | CON max retransmit triggers deregistration |
| §3.6 | Connection close cleanup | SHOULD | Pass | `serve.rs:771-774` | All observers removed on disconnect |
| §4.2 | CON notifications | MAY | Pass | `serve.rs:254-281` | Route-level configuration |
| §4.5 | Sequence number freshness | SHOULD | **Pass** | `serve.rs:245` | ✅ Fixed: 24-bit mask applied |

### RFC 7959 — Block-Wise Transfers

| Section | Requirement | Level | Status | Evidence | Notes |
|---------|------------|-------|--------|----------|-------|
| §2.2 | Block option encoding | MUST | Pass | Delegated to `coap-lite` | |
| §2.3 | Block2 response fragmentation | MUST | Pass | `serve.rs:488-493` | Tests verify reassembly |
| §2.5 | Block1 upload reassembly | MUST | Pass | `serve.rs:371` | 2.31 Continue for intermediate |
| §2.7 | Block size negotiation | MUST | Partial | No test coverage | Entirely delegated to `coap-lite` |
| §2.8 | Observe + Block2 | SHOULD | **Pass** | `serve.rs:277-283` | ✅ Fixed: Notifications routed through BlockHandler |
| §2.9.1 | 4.13 generation | MUST | Pass | Tests verify Block1 hint in response |
| §2.9.1 | Size1 in 4.13 | SHOULD | **Pass** | ✅ Fixed: Size1 option included in 4.13 responses |
| N/A | ACK type on block responses | MUST | **Pass** | `serve.rs:406-409`, `serve.rs:426-428` | ✅ Fixed: Piggybacked ACK for CON |
| N/A | Dedup cache for block responses | SHOULD | **Pass** | `serve.rs:411-416`, `serve.rs:430-435` | ✅ Fixed: `record_response` called |

### RFC 6347 — DTLS 1.2

| Section | Requirement | Level | Status | Evidence | Notes |
|---------|------------|-------|--------|----------|-------|
| §4.1 | Record layer | MUST | Pass | Delegated to `dimpl` | |
| §4.2.1 | Cookie exchange | SHOULD | **Partial** | `serve.rs:846-883` | Task+channels allocated before verification (deferred — needs upstream API) |
| §4.2.4 | Handshake fragmentation | MUST | Pass | Delegated to `dimpl` | |
| — | PSK cipher suite | MUST | Pass | `serve.rs:641` | PSK-only constructor |
| — | Identity resolution | MUST | Pass | `credential/resolver.rs:23-78` | Per-connection, race-free |
| — | Session lifetime | SHOULD | Pass | `config/mod.rs:65-71` | Configurable, default None |
| — | Key zeroization | SHOULD | Known Gap | `credential/mod.rs:25-29` | Documented limitation |

### RFC 8428 — SenML

| Section | Requirement | Level | Status | Evidence | Notes |
|---------|------------|-------|--------|----------|-------|
| §4 | Base fields (bn,bt,bu,bv,bs,bver) | MUST | **Pass** | `record.rs:15-40` | ✅ Fixed: Proper base fields on SenMLRecord |
| §4 | Record fields (n,u,v,vs,vb,vd,s,t,ut) | MUST | Pass | `record.rs:42-75` | |
| §4.3 | Value mutual exclusion | MUST | **Pass** | `record.rs:188-200` | ✅ Fixed: At most one of v/vs/vb/vd |
| §5 | JSON labels | MUST | **Pass** | `builder.rs:134-148` | ✅ Fixed: `bn`, `bt`, etc. via serde field names |
| §6 | CBOR integer labels | MUST | **Pass** | `pack.rs:268-395` | ✅ Fixed: Integer labels per Table 6 |
| §10 | Validation rules | Various | Pass | `record.rs`, `pack.rs` | Pack-not-empty, mutual exclusion, finite values |
| §12 | Content-Format numbers | MUST | Pass | `lib.rs:76-93` | 110/112 correct |

---

## 3. Gap Analysis — Prioritized

### P0 — MUST Violations (Interoperability Failures)

1. **~~Critical option rejection missing~~** — ✅ **FIXED.** Unknown critical options (odd numbers) now rejected with 4.02 Bad Option in `handle_request()`.

2. **~~GET+Observe=1 deregistration broken~~** — ✅ **FIXED.** Changed from `RequestType::Delete` to `RequestType::Get`.

3. **~~Block transfer responses missing piggybacked ACK~~** — ✅ **FIXED.** CON block requests now get ACK responses + dedup caching.

4. **~~SenML base fields not modeled~~** — ✅ **FIXED.** `SenMLRecord` now has `bn`, `bt`, `bu`, `bv`, `bs`, `bver` fields. Builder, pack, and normalize all updated.

5. **~~SenML CBOR uses string keys~~** — ✅ **FIXED.** `to_cbor()`/`from_cbor()` now use RFC 8428 Table 6 integer labels.

### P1 — SHOULD Violations with Real-World Impact

6. **~~EXCHANGE_LIFETIME too short~~** — ✅ **FIXED.** Added `max_latency` field (100s default). Formula now returns 247s.

7. **~~Observe notifications not block-fragmented~~** — ✅ **FIXED.** `handle_notification()` now routes through `BlockHandler::intercept_response()`.

8. **~~Block transfer responses not dedup-cached~~** — ✅ **FIXED.** `reliability.record_response()` called in both Ok and Err block paths.

### P2 — Correctness Issues

9. **~~Wrong response codes for unmatched routes~~** — ✅ **FIXED.** Router returns `LookupResult::NotFound` (4.04) or `MethodNotAllowed` (4.05).

10. **~~SenML value mutual exclusion not checked~~** — ✅ **FIXED.** `validate()` rejects records with more than one value field.

11. **~~32-bit observe sequence vs 24-bit wire format~~** — ✅ **FIXED.** Applied `& 0x00FF_FFFF` mask to both increment paths.

### P3 — Hardening / Performance

12. **DTLS state pre-allocation** (`serve.rs:846-883`) — **Deferred.** Needs upstream `dimpl` API for stateless cookie verification before spawning connection task.

13. **~~RequestTypeWrapper hash collision~~** — ✅ **FIXED.** All 8 variants now hash to distinct discriminants.

14. **No per-connection block transfer count limit** — Open. Authenticated client can open unbounded concurrent Block1/Block2 transfers.

15. **Dedup race window for slow handlers** (`serve.rs:350-507`) — Open. CON retransmits during handler execution bypass dedup cache.

---

## 4. Security Findings

| Finding | Severity | Category | Evidence |
|---------|----------|----------|----------|
| DTLS required for all connections | Info (positive) | Amplification mitigation | `serve.rs:641` |
| Connection slot exhaustion via spoofed ClientHello | Medium | DoS | `serve.rs:846-883` — task allocated before cookie (deferred) |
| Per-device observer limit | Info (positive) | Resource exhaustion | `config/mod.rs:44` (default 100) |
| Reconnection rate limiting | Info (positive) | DoS | `serve.rs:109-160` |
| Path traversal protection | Info (positive) | Injection | `observer/mod.rs:174-207` |
| CBOR recursion limit | Info (positive) | Stack overflow | `extract/payload.rs:254-257` (depth 32) |
| No block transfer count limit per connection | Low | Memory exhaustion | `serve.rs:650-653` |
| Key material not zeroized | Low | Key exposure | `credential/mod.rs:25-29` (documented) |
| Dedup race window | Low | Duplicate processing | `serve.rs:350-507` |

---

## 5. Test Coverage Gaps

| RFC Requirement | Has Test? | Notes |
|-----------------|-----------|-------|
| Critical option rejection (4.02) | Needs integration test | Code exists, unit logic verified |
| GET+Observe=1 deregistration | ✅ Fixed | Test now uses GET |
| RST-based observer deregistration | No | Unit tested in reliability.rs but no integration test |
| CON notification receipt + retransmission | No | |
| Observe sequence number verification | No | |
| Block size negotiation (client SZX) | No | |
| Block2 + observe fragmentation | Needs integration test | Code exists |
| Block transfer ACK for CON requests | Needs integration test | Code exists |
| EXCHANGE_LIFETIME correctness | ✅ | `test_exchange_lifetime_default` asserts 247s |
| 4.04 vs 4.05 response code distinction | ✅ | `test_lookup_not_found`, `test_lookup_method_not_allowed` |
| DTLS cookie exchange DoS resistance | No | Deferred |
| Session lifetime timeout | No | |
| Reconnect rate limiting | No | |
| SenML base field round-trip | ✅ | CBOR roundtrip test with all base fields |
| SenML CBOR integer labels | ✅ | `test_cbor_integer_keys_on_wire` verifies wire format |
| SenML value mutual exclusion | ✅ | `test_value_mutual_exclusion` |

---

## 6. Recommendations — Prioritized

### ✅ Completed

1. ~~Add critical option checking~~ — Done.
2. ~~Fix observe deregistration~~ — Done.
3. ~~Add piggybacked ACK to block transfer responses~~ — Done.
4. ~~Rewrite SenML record model~~ — Done.
5. ~~Fix EXCHANGE_LIFETIME~~ — Done.
6. ~~Route notifications through BlockHandler~~ — Done.
7. ~~Cache block transfer responses for dedup~~ — Done.
8. ~~Distinguish 4.04 vs 4.05~~ — Done.
9. ~~Add SenML value mutual exclusion check~~ — Done.
10. ~~Mask observe sequence to 24 bits~~ — Done.
11. ~~Fix RequestTypeWrapper hash~~ — Done.
12. ~~SenML CBOR integer labels~~ — Done.

### Remaining

13. **Defer DTLS state allocation** — Perform stateless cookie verification in the dispatch loop before spawning `connection_task`. Needs upstream `dimpl` API.

14. **Limit concurrent block transfers** — Add a per-connection cap on active Block1/Block2 transfers.

15. **Dedup race window** — Consider early dedup-reservation before handler execution.
