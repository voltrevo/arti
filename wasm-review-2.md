# WASM Branch Review (vs wasm-base)

**Branch:** A
**Base:** wasm-base
**Stats:** 120 files changed, ~23,700 lines added

---

## Overview

This diff adds WASM/browser support to arti by introducing three new crates
(`subtle-tls`, `tor-js`, `webtor-rs-lite`) and modifying ~50 existing crate
files. The architecture routes Tor traffic through Snowflake pluggable
transports (WebRTC or WebSocket), uses a custom pure-Rust TLS 1.3
implementation for relay connections, and exposes a `fetch()`-like JS API
via `wasm-bindgen`.

---

## Critical

### C1. RwLock deadlock in BoxedDirStore

`crates/tor-dirmgr/src/storage/custom.rs`

Methods like `expire_all` (line 312), `latest_consensus` (line 380),
`consensus_by_sha3_digest_of_signed_part` (line 445), and
`mark_consensus_usable` (line 472) acquire an `inner` RwLock read guard
then call `self.load_json()`, which acquires the same RwLock again. The Rust
standard library states that calling `read()` while already holding a read
lock may deadlock depending on the platform's RwLock implementation
(write-preferring locks will deadlock if any writer is waiting).

### C2. TLS inner-plaintext padding stripping is wrong (RFC 8446 Section 5.4)

`crates/subtle-tls/src/record.rs:182-191` (and duplicated at line 342 and `stream.rs:572`)

The code takes the absolute last byte as the content type. Per RFC 8446, the
content type is the last **non-zero** byte -- implementations may append zero
padding for traffic analysis resistance. The correct approach is to scan
backward from the end until finding a non-zero byte. Some TLS servers do add
padding, which would cause this to misinterpret content types.

### C3. No X.509 BasicConstraints / KeyUsage validation

`crates/subtle-tls/src/cert.rs:71-99`

`verify_chain` checks server name, validity period, and chain signatures, but
does **not** check:
- BasicConstraints (CA:TRUE required for intermediates)
- Key Usage / Extended Key Usage (serverAuth EKU on leaf)
- Path length constraints

A rogue leaf certificate could act as a CA and issue sub-certificates.

### C4. No X25519 all-zero shared secret check

`crates/subtle-tls/src/crypto.rs:200-221`

RFC 7748 Section 6.1 requires checking that the DH result is not all zeros.
`x25519-dalek::diffie_hellman()` does not perform this check. A malicious
server could send a low-order point, producing a predictable all-zero shared
secret.

### C5. Server cipher suite selection not validated against ClientHello offer

`crates/subtle-tls/src/handshake.rs:350-353`

The server's cipher suite is accepted unconditionally. RFC 8446 Section 4.1.3
requires aborting if the server selects a cipher suite not offered. A
malicious server could select AES-256-GCM-SHA384 (0x1302), which requires
SHA-384 for the key schedule, but the code unconditionally uses SHA-256,
producing wrong keys.

### C6. RSA-PSS hash algorithm hardcoded to SHA-256

`crates/subtle-tls/src/cert.rs:406-418` (and line 534-546)

When verifying RSA-PSS signatures, the hash algorithm is always SHA-256
regardless of the AlgorithmIdentifier parameters. Certificates signed with
RSA-PSS + SHA-384 (increasingly common with 3072+ bit keys) will have
incorrect signature verification.

---

## High

### H1. `state_dir()` called unconditionally in `create_inner`

`crates/arti-client/src/client.rs:897`

`config.state_dir()` is not gated behind `#[cfg(not(target_arch = "wasm32"))]`.
On WASM, filesystem path resolution will fail, preventing client creation even
when custom storage is provided. This is a latent defect -- the WASM path
likely works today only because the default config happens to not fail, but it
should be properly gated.

### H2. KCP `poll_write` double-sends data on `Pending`

`crates/webtor-rs-lite/src/kcp_stream.rs:434-472`

Data is queued in KCP via `kcp.send(buf)` before attempting transport write.
If the transport returns `Pending`, the caller will retry `poll_write` with the
same data (per the `AsyncWrite` contract), causing the data to be enqueued in
KCP again -- resulting in duplicate delivery.

### H3. `export_keying_material` returns zeros instead of error

`crates/webtor-rs-lite/src/snowflake.rs:265-268` and `snowflake_ws.rs:175-178`

Returns `Ok(vec![0u8; len])` for keying material export. If any Tor protocol
component uses this for key derivation, all keys would be zero. Should return
`Err` so callers know the operation is unsupported.

### H4. Header injection in tor-js fetch

`crates/tor-js/src/fetch.rs:65-67`

User-supplied header names/values are inserted into raw HTTP without any
CR/LF validation. A value containing `\r\n` could inject headers or enable
HTTP request smuggling.

### H5. `unsafe impl Send` on TlsStream without cfg guard

`crates/subtle-tls/src/stream.rs:66-69`

`unsafe impl<S: Send> Send for TlsStream<S>` is not gated behind
`#[cfg(target_arch = "wasm32")]`. The struct contains `Rc<ReadySignal>`,
`Cell`, and `RefCell` -- none of which are `Send`. On native multi-threaded
runtimes this would be unsound. While currently only used on WASM, the impl
should be cfg-gated.

### H6. EC curve defaults to P-256 on parse failure

`crates/subtle-tls/src/cert.rs:482-485`

If the EC curve cannot be determined from key parameters, the code defaults to
P-256 and logs a warning. A P-384 certificate whose parameters fail to parse
would be verified with the wrong curve. Should return an error.

### H7. CA bundle fetch has no size limit and no status check

`crates/tor-js/src/lib.rs:468-480`

`read_to_end` with no size limit. A malicious exit node could serve an
arbitrarily large response. The HTTP status code is also not checked -- a 302
redirect or 500 error page could be treated as valid PEM.

### H8. Incremental download path has zero test coverage

`crates/tor-dirmgr/src/bootstrap.rs:554-603`

The production code path (streaming downloads, `#[cfg(not(test))]`) is never
exercised by the test suite, which only tests the batch path.

### H9. `skip_verification` exposed as public struct field

`crates/subtle-tls/src/lib.rs:64-72`

`TlsConfig::skip_verification` is a public `bool` with no feature-gate.
Production TLS libraries typically gate this behind a feature flag like
`danger_accept_invalid_certs`.

---

## Medium

### M1. SMUX keepalive interval 500ms (comment says 5 seconds)

`crates/webtor-rs-lite/src/smux.rs:239`

The constant is `500` (ms) but the comment says "send NOP every 5 seconds".
The Go Snowflake client uses 10 seconds. This is 20x too aggressive.

### M2. SMUX NOP echo creates ping-pong risk

`crates/webtor-rs-lite/src/smux.rs:469-477`

Received NOPs are echoed back. The Go Snowflake implementation does NOT echo
NOPs. If the server also echoes, this creates unbounded network traffic.

### M3. Trust store matching by Subject DN string comparison

`crates/subtle-tls/src/trust_store.rs:255-276`

Root CA matching uses `to_string()` comparison of X.500 names, which has
encoding ambiguities (UTF-8 vs PrintableString, whitespace normalization).
Comparing raw DER-encoded issuer/subject bytes would be more robust.

### M4. No secret zeroization

`crates/subtle-tls/src/handshake.rs:68-97`

All TLS secrets (`handshake_secret`, `client_app_secret`,
`server_app_secret`, `exporter_master_secret`) are stored as `Vec<u8>` and not
zeroed on drop. Consider `zeroize::Zeroizing<Vec<u8>>`.

### M5. JS storage lock never released on `close()`

`crates/tor-js/src/lib.rs:287-293` and `storage.rs:360-368`

`TorClient::close()` drops the inner client but never calls `unlock()` on JS
storage. The lock acquired in `CachedJsStorage::new()` persists until page
unload.

### M6. `User-Agent: tor-js/0.1.0` enables fingerprinting

`crates/tor-js/src/fetch.rs:55`

The default User-Agent makes it trivial for exit nodes or destinations to
identify traffic from this library. The Tor Browser uses a specific Firefox
User-Agent for anonymity.

### M7. `init()` panics on double-call

`crates/tor-js/src/lib.rs:73-87`

`tracing_subscriber::registry().init()` is not idempotent. Calling `init()`
twice will panic or silently fail.

### M8. JS errors are plain objects, not `Error` instances

`crates/tor-js/src/error.rs:71-75`

`serde_wasm_bindgen::to_value` produces `{code: "...", kind: "..."}` rather
than a `new Error(...)`. `instanceof Error` checks fail, `.stack` is missing,
and console output shows `[object Object]`.

### M9. `unsafe impl Send/Sync` for JsStorage / CachedJsStorage

`crates/tor-js/src/storage.rs:87-88, 190-191`

Justified for single-threaded WASM but will become unsound if WASM threads
(`SharedArrayBuffer` + atomics) are enabled. Consider gating behind
`#[cfg(not(target_feature = "atomics"))]`.

### M10. Debug microdesc batch size in production code

`crates/tor-dirmgr/src/docid.rs:188-205`

`MICRODESC_N` is 20 on WASM (vs 500 native) with a `/// DEBUG:` comment. If
20 is the intended production value, remove the comment; if not, revert it.

### M11. WASM `Instant::now()` panics if `performance` API unavailable

`crates/webtor-rs-lite/src/time.rs:16`

`unwrap()` on `performance` access. In environments without the Performance
API (some Web Workers, non-browser WASM hosts), every `Instant::now()` call
panics.

### M12. No timeout on native broker HTTP request

`crates/webtor-rs-lite/src/snowflake_broker.rs:293-362`

No timeout on `TcpStream::connect` or `tls_stream.read_to_end`. A
non-responsive broker hangs the client indefinitely.

### M13. Verbose info-level logging in crypto module

`crates/subtle-tls/src/crypto.rs` (passim, e.g. lines 460-508)

Seven `info!()` calls inside `AesGcm::decrypt`. These are clearly debug
artifacts and should be demoted to `trace` or `debug`.

### M14. Missing HelloRetryRequest handling

`crates/subtle-tls/src/stream.rs:207-254`

The code does not detect HelloRetryRequest (ServerHello with special
`server_random` value per RFC 8446 Section 4.1.4). Connections to servers
requiring a different key share group fail with an opaque error.

### M15. ALPN configuration ignored

`crates/subtle-tls/src/lib.rs:69` and `handshake.rs:232`

`TlsConfig::alpn_protocols` is never read. `build_alpn_extension` hardcodes
`["http/1.1"]` regardless of the config.

### M16. Read timeout changed from 10s to 120s for all platforms

`crates/tor-dirclient/src/lib.rs:405-430`

Changed from a fixed 10-second total timeout to a 120-second idle timeout.
This affects native too, not just WASM. 120s of idle time could mask stalled
connections on native.

### M17. `reconfigure` computes `state_cfg` unconditionally on WASM

`crates/arti-client/src/client.rs:1303-1306`

`expand_state_dir` runs on WASM for no reason (the result is only used in a
`#[cfg(not(target_arch = "wasm32"))]` block). May fail unnecessarily.

---

## Low

### L1. SMUX payload truncation

`crates/webtor-rs-lite/src/smux.rs:125`

Payload length encoded as `u16` -- data > 65535 bytes silently truncates.
No runtime guard.

### L2. Unbounded channels in WebSocket/WebRTC

`crates/webtor-rs-lite/src/websocket.rs:40` and `webrtc_stream.rs:92`

`mpsc::unbounded()` for incoming data with no backpressure at this layer.

### L3. Duplicated code across WASM/native file pairs

- `snowflake_ws.rs` / `snowflake_ws_native.rs` are near-duplicates
- `arti_transport.rs` / `arti_transport_native.rs` are near-duplicates
- `hmac_sha256` duplicated in `handshake.rs` and `crypto.rs`

### L4. `create_snowflake_stream` ignores its parameters

`crates/webtor-rs-lite/src/snowflake.rs:326-333`

Both `broker_url` and `connection_timeout` are accepted but ignored.

### L5. HTTP method matching is case-sensitive

`crates/tor-js/src/lib.rs:521-538`

The Fetch spec normalizes methods to uppercase. `"get"` or `"post"` here
returns `INVALID_METHOD`.

### L6. `TlsVersion` enum has one variant, `version` field unused

`crates/subtle-tls/src/lib.rs:56-61`

Dead abstraction.

### L7. Dead random bytes in `X25519KeyPair::generate()`

`crates/subtle-tls/src/crypto.rs:185-188`

32 bytes allocated and generated via `getrandom` then immediately discarded.
The actual key generation uses `OsRng` separately.

### L8. `#[allow(dead_code)]` on multiple struct fields

- `subtle-tls/src/crypto.rs:262-263, 358-359` (`key_size`)
- `webtor-rs-lite/src/kcp_stream.rs:109,111,114`
- `webtor-rs-lite/src/webrtc_stream.rs:57,63-68`

### L9. `Blocking` trait panics on WASM

`crates/tor-rtcompat/src/wasm.rs:178-201`

`spawn_blocking` and `reenter_block_on` panic. Correct for WASM but any
library code reaching these without a cfg guard causes a runtime crash.

### L10. Fire-and-forget `spawn_local` writes in CachedJsStorage

`crates/tor-js/src/storage.rs:265-282`

If JS storage persistence fails (e.g., quota exceeded), the in-memory cache
diverges from persistent storage. Errors are only logged.

### L11. Bridge fingerprint logged at INFO level

`crates/webtor-rs-lite/src/snowflake.rs:125`, `snowflake_ws.rs:102`,
`tor-js/src/lib.rs:321`

In contexts where the bridge is private or unlisted, this leaks which bridge
the user is connecting to.

---

## Testing Gaps

1. **No end-to-end TLS handshake test** in subtle-tls (no loopback or
   known-good test vector validation)
2. **No certificate chain validation test** -- only `test_skip_verification`
3. **No test for `poll_read`/`poll_write`** `AsyncRead`/`AsyncWrite` impls on
   `TlsStream`
4. **No test for hostname verification edge cases** (IP SAN, null byte
   injection, IDN)
5. **No test for record fragmentation/reassembly**
6. **Production streaming download path** (`#[cfg(not(test))]`) never tested
7. **No unit tests for webtor-rs-lite** -- entire crate has zero `#[test]`
   functions

---

## Positive Observations

- **Storage abstraction** (`KeyValueStore` -> `split_storage`) is clean and
  well-designed, giving callers a single trait to implement
- **`wasm_compat::Send`** pattern for removing `Send` bounds on WASM is
  elegant
- **Time consolidation** into `tor-time` crate is good refactoring
- **Conditional compilation** is mostly thorough and correct across ~50 files
- **Fuzz targets** for subtle-tls are a good start
- **`portable_test` / `portable_test_async`** macros enable cross-platform
  test authoring
