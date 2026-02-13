TODO: Consider this.

# Code Review: WASM Arti Client

Branch `wasm-arti-client` (at `b399d41`) vs `zydou/main` (at `206e629`) — **280 files changed**, ~25k lines added.

## Overview

This branch adds WebAssembly support to the Arti Tor client. Major new crates: `tor-js` (WASM bindings), `subtle-tls` (TLS 1.3 for WASM), `tor-time` (cross-platform time), `webtor-rs-lite` (Snowflake transport), `tor-wasm-compat` (async trait compat). Extensive modifications to core Arti crates for WASM compatibility.

> **Note on Tor TLS:** Tor relays speak TLS to each other using self-signed
> certificates — authentication happens via Tor's own CERTS cells (Ed25519/RSA
> identity keys), not WebPKI. This is why `skip_verification: true` exists and
> is used intentionally for relay connections. The `skip_verification: false`
> path is used for HTTPS to external services (Snowflake broker, tor-js fetch)
> and *does* need proper WebPKI validation.

---

## CRITICAL Issues

### 1. Certificate chain validation not enforced in subtle-tls
`crates/subtle-tls/src/cert.rs:253-268` — When a certificate chain doesn't terminate at a trusted root, the code **logs a warning and continues** instead of rejecting the connection. This defeats the purpose of certificate validation and enables MITM attacks.

> **FIXED.** Untrusted chains now return `Err(TlsError::certificate(...))`.
> This only affects the `skip_verification: false` path; Tor relay connections
> (`skip_verification: true`) return early and never reach this code.

### 2. Incomplete intermediate CA verification in subtle-tls
`crates/subtle-tls/src/cert.rs:242-251` — When an intermediate CA is issued by a trusted root but the root isn't in the chain, the code accepts based on **issuer name matching alone** without verifying the root's signature. An attacker can forge intermediate certs with matching issuer names.

> **FIXED.** Renamed `is_issued_by_trusted_root` → `find_issuing_trusted_root`,
> which returns the root's DER bytes. The caller now cryptographically verifies
> the root's signature on the last chain certificate via `verify_signature()`.

### 3. Timing side-channel in TLS Finished verification
`crates/subtle-tls/src/handshake.rs:571-580` — Uses non-constant-time `!=` comparison for the Finished message verify data. An attacker can use timing analysis to forge Finished messages byte-by-byte. Should use `constant_time_eq` or equivalent.

> **FIXED.** Added `subtle = "2"` dependency; comparison now uses
> `subtle::ConstantTimeEq::ct_eq()`.

### 4. Panic on oversized Turbo frame
`crates/webtor-rs-lite/src/turbo.rs:86` — `panic!("Frame too large")` instead of returning a `Result`. Any oversized frame will crash the entire WASM application. Must return an error instead.

> **Fixed.** `encode()` now returns `Result<Vec<u8>>` and the panic is replaced with `Err(TorError::Protocol(...))`.

---

## HIGH Issues

### 5. Nonce reuse risk in record layer
`crates/subtle-tls/src/record.rs:47-54` — The sequence counter uses `wrapping_add(1)`, which silently wraps at 2^64. While practically infeasible, the TLS spec requires rejecting records once the limit is reached. Long-lived connections in unusual deployment scenarios should enforce this.

> **FIXED.** `increment_sequence()` now returns `Result<()>` and rejects at
> `u64::MAX` per RFC 8446 Section 5.3. All 6 call sites in `record.rs` and
> the async start methods in `stream.rs` propagate the error.

### 6. skip_verification disables CertificateVerify
`crates/subtle-tls/src/stream.rs:384-410` — When `skip_verification` is true, CertificateVerify (server's proof of key possession) is also skipped. CertificateVerify should always be validated regardless of certificate chain verification mode.

> **FIXED.** Restructured `process_encrypted_handshake` so that
> CertificateVerify is always verified when the server sends a Certificate
> (regardless of `skip_verification`). Only chain validation (WebPKI trust)
> is skipped when `skip_verification: true`. This adds defense-in-depth for
> Tor relay connections.

### 7. No locking for concurrent browser tabs
`crates/tor-js/src/storage.rs:423-430` — `upgrade_to_readwrite()` always grants the lock. Multiple browser tabs sharing IndexedDB can corrupt each other's directory cache and guard state. The FIXME is already present acknowledging this.

> **TODO.** Not yet addressed. Could use the Web Locks API.

### 8. Mutex poisoning panics in KCP
`crates/webtor-rs-lite/src/kcp_stream.rs:37,43,49` — `.lock().unwrap()` on mutex will panic if the lock is poisoned. Should handle `PoisonError` or use `parking_lot::Mutex`.

> **Done.** Replaced `.lock().unwrap()` with `.lock().unwrap_or_else(|e| e.into_inner())`
> to recover from poisoned mutexes instead of panicking. Safe because the
> protected data is just a `Vec<u8>` with no invariants.

### 9. No bootstrap timeout
`crates/tor-js/src/lib.rs:389-393` — `tor_client.bootstrap().await` can hang indefinitely if the Snowflake bridge is unresponsive. In a browser, this blocks the single-threaded event loop. Should wrap with a timeout.

> **TODO.** Not yet addressed.

### 10. Unsanitized SNI hostname
`crates/subtle-tls/src/handshake.rs:248-268` — No validation that the server name contains valid DNS characters before sending in the SNI extension.

> **TODO.** Not yet addressed. Low real-world risk since hostnames come from
> internal Tor config, not user input.

---

## MEDIUM Issues

### 11. Duration serde round-trip bug
`crates/tor-time/src/serde_time.rs:37-40` — Nanoseconds serialized as `format!("{}.{}s", secs, nanos)` produces e.g. `"1.100s"` for 1s+100ns, which deserializes to 100ns instead of the intended value. The fractional part needs zero-padding to 9 digits for correct round-tripping.

> **Fixed.** Serialization now uses `{:09}` to zero-pad nanos to 9 digits. Deserialization right-pads the fractional part to 9 digits before parsing, so both new and legacy formats round-trip correctly.

### 12. Misleading comment in wallclock()
`crates/tor-rtcompat/src/wasm.rs:129` — Comment says "Use Performance.now()" but code uses `js_sys::Date::now()`. These are completely different APIs (monotonic vs wall clock).

> **Fixed.** Comment updated to say "Date.now()" instead of "Performance.now()".

### 13. Aggressive SMUX keepalive
`crates/webtor-rs-lite/src/smux.rs:240` — `KEEPALIVE_INTERVAL_MS = 500ms` sends NOP every 500ms. Official Snowflake clients typically use 5-30 second intervals. This wastes bandwidth over Tor.

> **TODO.** Not yet addressed.

### 14. Fragile error classification via string matching
`crates/tor-js/src/error.rs:78-92` and `crates/webtor-rs-lite/src/snowflake_broker.rs:144-145` — Error codes are assigned by matching on `.to_string()` content (e.g. `message.contains("bootstrap")`). Fragile and could misclassify errors.

> **Done.** `tor-js/src/error.rs`: replaced string matching with structured `ErrorKind` matching via `arti_client::HasKind`. `snowflake_broker.rs`: string matching is unavoidable since the broker protocol returns errors as plain strings; added comment documenting this.

### 15. Possible string slicing panic
`crates/tor-js/src/lib.rs:144-145` — `self.message[1..self.message.len()-1]` slicing after checking for `starts_with('"')` could panic on non-ASCII boundaries. Use `trim_matches('"')` instead.

> **TODO.** Not yet addressed.

### 16. Duplicate trait definitions in tor-persist
`crates/tor-persist/src/custom.rs:48-91` — `CustomStateMgr` trait is defined twice (once for WASM, once for native) with identical definitions. Should be unified.

> **TODO.** Not yet addressed.

### 17. InMemoryStore inconsistent readonly behavior
`crates/tor-dirmgr/src/storage/inmemory.rs:416-418` — `store_bridgedesc()` returns `Ok(())` when readonly, while other write methods return errors. Inconsistent behavior.

> **TODO.** Not yet addressed.

### 18. Bridge fingerprint not validated
`crates/tor-js/src/lib.rs:296-299` — Fingerprint is required but not validated for format (40 hex chars). Invalid fingerprints are accepted silently.

> **TODO.** Not yet addressed.

---

## Strengths

- Well-structured WASM/native separation using `cfg(target_arch = "wasm32")`
- Comprehensive storage abstractions (InMemory, Custom, JS-backed)
- Good error types in tor-js with retryability info
- `unsafe impl Send/Sync` for WASM types are correctly justified (single-threaded)
- Extensive fuzz testing (subtle-tls, webtor-rs-lite)
- Kani verification proofs for ECDSA DER conversion
- Clean `tor-time` crate consolidating cross-platform time handling
- `tor-wasm-compat` proc macro for conditional `?Send` on async traits

---

## Priority Recommendations

1. ~~**Fix subtle-tls cert validation** (items 1-2) — these are authentication bypasses~~ **DONE**
2. ~~**Use constant-time comparison** for Finished message (item 3)~~ **DONE**
3. **Replace panic with Result** in turbo.rs (item 4)
4. **Add bootstrap timeout** (item 9)
5. **Fix serde round-trip bug** (item 11) — data corruption risk