# Code Review: WASM Arti Client

**Branch:** `main` (includes merged `wasm-arti-client`)
**Base:** `zydou/main` (upstream)
**Last audited:** 2026-03-27

## Overview

This branch adds WebAssembly/browser support to the Arti Tor client. Major new
crates: `tor-js` (WASM bindings + TS wrapper), `tor-time` (cross-platform
time), `tor-async-compat` (conditional Send bounds). Extensive modifications to
core Arti crates for WASM compatibility. Traffic routes through a gateway server
(WebRTC or WebSocket relay proxying) in browsers, or via direct TCP in Node.js/
Deno. Uses `rustls` + `rustls-rustcrypto` (pure-Rust TLS compiled to WASM) for
relay connections, and exposes a `fetch()`-like JS API via `wasm-bindgen`.

> **Note on Tor TLS:** Tor relays use self-signed certificates --
> authentication happens via Tor's own CERTS cells (Ed25519/RSA identity
> keys), not WebPKI. The relay TLS layer uses a custom `ServerCertVerifier`
> (`TorCertVerifier`) that skips certificate validation but still verifies
> TLS handshake signatures -- this is the same pattern used by the native
> rustls provider. HTTPS to external services (tor-js fetch) uses standard
> WebPKI validation via `webpki-roots` (Mozilla CA bundle embedded at
> compile time).

For per-crate change details, see `changes-explained.md`.

---

*All medium items have been resolved. See Already Fixed section below.*

---

*All low items have been resolved. See Already Fixed section below.*

---

## Testing

- **Custom storage**: Unit tests for `AnyStateMgr` and `BoxedDirStore` with
  in-memory `KeyValueStore`. Missing: concurrent access patterns, error cases
  (write failures, lock failures).
- **No WASM integration tests** visible in the diff (tests run on native via
  `wasm-bindgen-test` but no browser-level end-to-end test).
- **Smoke test**: `tor-fetch.js --websocket` successfully connects through Tor
  and returns `{"IsTor":true}`.

---

## Positive Observations

- Well-structured WASM/native separation using `cfg(target_arch = "wasm32")`
- Clean storage abstraction: single `KeyValueStore` trait, no intermediate layers
- `SendWrapper` + `SendFut` eliminate `unsafe impl Send` without a proc-macro crate
- `tor-time` creates solid foundation for cross-platform time handling
- Good error types in tor-js with retryability info and structured error codes
- `unsafe impl Send/Sync` for WASM types are correctly justified (single-threaded)
- TLS handled by well-audited `rustls` rather than custom implementation
- Batch persistence optimization (`set_many()`) acquires lock once
- Fast-failure path: connection errors surfaced to `ready()` for quick feedback
- Hardware-accelerated SHA-256 via `crypto.subtle.digest()` in fast bootstrap
- CR/LF validation in fetch headers prevents HTTP header injection
- `wait_for_unlock()` future enables async lock coordination
- `ArtiSocketProvider` cleanly abstracts over direct TCP / WebRTC / WebSocket
  with auto-detection and fallback
- `ruzstd` (pure-Rust zstd) enables x-zstd directory compression on WASM

---

## Already Fixed

These items were identified in earlier reviews and have been resolved:

1. **Header injection in tor-js fetch** -- CR/LF validation added
2. **`init()` panics on double-call** -- now uses `try_init()` which is
   idempotent
3. **CA bundle fetch had no size limit / status check** -- Eliminated entirely;
   `webpki-roots` embeds Mozilla CA bundle at compile time
4. **Duration serde round-trip bug** -- Now uses `{:09}` zero-padding
5. **Misleading comment in wallclock()** -- Corrected
6. **Fragile error classification via string matching** -- tor-js now uses
   structured `ErrorKind` matching
7. **Possible string slicing panic** -- Replaced with `trim_matches('"')`
8. **Duplicate trait definitions in tor-persist** -- Consolidated: single
   `KeyValueStore` trait used directly by `AnyStateMgr` and `BoxedDirStore`
9. **RwLock deadlock in BoxedDirStore** -- `BoxedDirStore` now wraps
   `Arc<dyn KeyValueStore>` directly, no internal RwLock
10. **All subtle-tls issues** -- Resolved by replacing the custom TLS
    implementation with `rustls` + `rustls-rustcrypto`
11. **`MICRODESC_N` debug batch size** -- Reverted to standard batch size (500)
12. **Read timeout idle→total** -- Reverted to upstream's total 10s timeout
13. **Commented-out debug tracing in tor-proto** -- Removed
14. **`init()` reload handle desync** -- `init()` now uses idempotent
    `try_init()`, reload handle properly managed
15. **Streaming bootstrap untested** -- Reverted to upstream's batch approach
    (see `potential-improvements.md`)
16. **wasm.ts double-init** -- Idempotent pattern; rejected promise cached
    permanently, which is correct behavior (no silent retry of broken init)
17. **Blocking panics on WASM** -- By design. `spawn_blocking` and
    `reenter_block_on` are unreachable on WASM (only called from native-only
    code paths: arti CLI, PoW solver, Tokio runtime)
18. **Fire-and-forget writes in CachedJsStorage** -- By design. Async
    write-back with error logging is the intended pattern for bridging sync
    Rust reads with async JS storage. Errors don't affect correctness of
    the in-memory cache for the current session
19. **rustls-rustcrypto is alpha** -- External dependency, nothing to fix.
    Monitor for updates
20. **Cross-tab storage locking** -- Already handled by TS wrapper's
    `locking.ts`: first tab acquires real lock (Web Locks API), writes to
    IndexedDB. Other tabs fall back to in-memory overlay — reads from
    IndexedDB, writes to memory. No cross-tab corruption.
21. **`state_dir()` unconditional** -- Gated behind `#[cfg(not(wasm32))]`.
    Only used by native-only features (pt-client, onion-service-service)
22. **No bootstrap timeout** -- By design. `ready()` stays pending while
    Arti keeps trying to connect (no terminal "give up" state). If the
    gateway is unreachable, `create()` itself fails after exhausting
    directory download retries. `ready()` only runs after `create()`
    succeeds, meaning the network is reachable. Arti retries guards
    indefinitely, and the event stream accurately reflects status.
23. **Lock release on close()** -- By design. Drop spawns `spawn_local`
    to release the JS lock asynchronously. If the event loop stops (page
    unload), the Web Locks API releases automatically. Node.js file locks
    are cleaned up on process exit or detected stale via heartbeat.
24. **User-Agent fingerprinting** -- Default User-Agent now forwards
    `navigator.userAgent` from the browser. No UA in Node.js. Header checks
    are case-insensitive via `has_header()`.
25. **JS errors are plain objects** -- `into_js_value()` now returns proper
    `js_sys::Error` with `code`/`kind`/`retryable` properties.
26. **unsafe impl Send/Sync** -- Replaced with `send_wrapper::SendWrapper`
    on struct fields and `SendFut` for async futures. No more unsafe.
27. **reconfigure state_cfg on WASM** -- `expand_state_dir()` moved inside
    `cfg(not(wasm32))` block.
28. **Abort signal not raced into awaits** -- Known limitation, matches
    browser fetch() behavior. Checkpoints cover before connect, before send,
    and between body chunks. Gap during connect/TLS is typically seconds.
29. **Duplicate/collapsed HTTP headers** -- Request: caller's Host/
    Content-Length skipped with warning. Response: `Vec<(String, String)>`
    preserves duplicates for `Headers::append()`.
30. **1xx interim responses** -- Parser now loops past 1xx, rejects 101.
31. **Chunked truncation** -- EOF mid-chunk now returns error.
32. **HTTP method case** -- Normalized to uppercase per Fetch spec.
33. **README out of sync** -- Crate README rewritten as developer doc,
    points to ts-wrapper/README.md for usage.
34. **Fast bootstrap skips verification** -- By design. The
    `dangerously_assume_wellsigned()`/`dangerously_assume_timely()` calls
    are only for metadata extraction (key IDs, timestamps for storage keys).
    Arti re-verifies all cached data cryptographically in `add_from_cache`:
    timeliness checks, authority signature validation, cert chain verification.
    A malicious gateway cannot inject fake data.
