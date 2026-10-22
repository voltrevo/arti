# Diff Analysis: Changes from upstream (`zydou/main`)

Changes in `crates/`. For non-crate changes
(scripts, examples, CI, root config), see `non-crate-changes.md`.

Upstream's `web-time-compat` crate and its codebase-wide migration
(`std::time` → `web_time_compat`, `coarsetime` → `CoarseInstant`, etc.)
are already merged. This document covers our additional changes on top
of that baseline.

---

## tor-rtcompat (+199 -46)

Arti is runtime-agnostic — it works with Tokio, async-std, or anything
implementing its `Runtime` trait. This crate provides those
implementations.

The WASM runtime itself (`WasmRuntime` + its JS socket/TLS plumbing,
formerly `src/wasm.rs`) now lives in the separate **tor-js** repository.
What remains here are the changes that make `tor-rtcompat` *compile* for
`wasm32` — feature gating, time shims, and a WASM spawn path — so the
external crate can build against it.

### `src/lib.rs`
**What:**
- All PreferredRuntime-related `#[cfg]` blocks gain `not(target_arch = "wasm32")`.
- Various feature gate combinations updated.

**Why:** WASM support — PreferredRuntime doesn't exist on WASM, so the
native backends (tokio/async-std/smol) and their TLS providers are all
gated out of the `wasm32` build.

### `src/traits.rs`
**What:**
- `SpawnExt::spawn()`: On WASM, uses `wasm_bindgen_futures::spawn_local`
  instead of `spawn_obj` (cfg-gated implementation).

**Why:** WASM has no multi-threaded executor; `spawn_local` is the only
option.

### `src/coarse_time.rs`
**What:** WASM fallback added — `CoarseInstant` wraps
`web_time_compat::Instant` on WASM (instead of `coarsetime::Instant`
which doesn't support WASM). WASM arithmetic impls added for
`CoarseInstant` ± `CoarseDuration`.

**Why:** `coarsetime` crate doesn't compile on WASM.

---

## Storage Changes

Arti normally persists two kinds of data: **state** (guard lists,
bridge configs — small JSON blobs keyed by name) and **directory cache**
(consensus documents, microdescriptors, authority certs — large,
structured, with complex query patterns). On native, state goes to
the filesystem via `FsStateMgr` and the directory cache goes to SQLite
via `SqliteStore`. Neither exists on WASM.

The solution is a single `KeyValueStore` trait: get/set/delete strings
by key, plus a lock. Users implement this once (e.g., backed by
IndexedDB in the browser, or the filesystem in Node.js). Internally,
the same store is shared by two consumers:

- **`AnyStateMgr`** (tor-persist) handles state. It prefixes keys with
  `"state:"`, serializes Rust values to JSON via serde, and stores the
  JSON string. Loading deserializes back. On native, `AnyStateMgr`
  dispatches to `FsStateMgr` instead.

- **`BoxedDirStore`** (tor-dirmgr) handles the directory cache. It
  implements the full 20+ method `Store` trait by mapping each operation
  to key-value calls. Consensus documents are stored as
  `dir:consensus:microdesc:<sha3>`, microdescriptors as
  `dir:microdesc:<digest>`, etc. Each is wrapped in a JSON-serializable
  struct that captures the metadata (timestamps, hashes, flags) alongside
  the content.

`split_storage()` in arti-client just wraps the user's store in an
`Arc` and passes the same Arc to both `AnyStateMgr` and `BoxedDirStore`.
They share the lock.

### arti-client (+510 -17)

#### `src/storage.rs`
**What:** Re-exports `KeyValueStore` and `StorageError` from `tor-persist`.
Provides `split_storage()` which wraps a `KeyValueStore` in an Arc and
passes it to both `AnyStateMgr::from_custom()` and `BoxedDirStore::new()`.

#### `examples/readme_custom_storage.rs`
**What:** Example demonstrating the `KeyValueStore` trait with a
file-backed implementation.

#### `src/builder.rs`
**What:** Adds `statemgr` and `dirstore` fields to `TorClientBuilder`.
New methods: `state_mgr()`, `dir_store()`, `storage()` (convenience
that calls `split_storage`). Builder passes these through to
`TorClient::create_inner`.

#### `src/client.rs`
**What:**
- `statemgr` field changed from `FsStateMgr` to `AnyStateMgr`.
- `create_keymgr()` returns `Ok(None)` on WASM.
- `create_inner()` takes `statemgr` and `dirstore` params.
- `statemgr_from_config()` cfg-gated to non-WASM only; builder's
  `resolve_statemgr()` calls it for the default case.
- `DirMgrStore` construction dispatches to custom or default.
- `reconfigure()`: state directory comparison gated behind `not(wasm32)`.
- `wait_for_stop()`: split into native/WASM versions.

### tor-persist (+394 -1)

#### New file: `src/custom.rs`
**What:** `KeyValueStore` trait and `AnyStateMgr` enum dispatching
between `Fs(FsStateMgr)` and `Custom(Arc<dyn KeyValueStore>)`.

#### `src/err.rs`
**What:** New `Resource::Memory` variant and public error constructors.

### tor-dirmgr (+695 -8)

#### `src/lib.rs`
**What:** `DirMgrStore::from_custom_store()` method. Re-exports `BoxedDirStore`.

#### `src/storage.rs`
**What:** `File`, `IoResult`, `sqlite` module, `InputString::load()` gated
behind `not(wasm32)`. New `pub(crate) mod custom;` and `pub use custom::BoxedDirStore;`.
`router_descs` field gets `allow(dead_code)` on WASM.

#### New file: `src/storage/custom.rs`
**What:** `BoxedDirStore` implementing the full `Store` trait via
key-value calls with JSON serialization.

---

## tor-proto (+101 -26)

### `src/util/ts.rs`
**What:** `AtomicOptTimestamp` — moved here from the deleted `tor-time`
crate. WASM version uses `js_sys::Date::now()` instead of `coarsetime`.

### `src/channel.rs`, `src/lib.rs`
**What:** `duration_unused()` and `time_since_last_incoming_traffic()`
get cfg-gated returns for WASM where `CoarseDuration` is already
`std::time::Duration`.

### `src/channel/handshake.rs`, `src/relay/channel/handshake.rs`
**What:** Replace direct `coarsetime::Instant` usage with our
`CoarseInstant` wrapper type (which uses `web_time_compat::Instant`
on WASM). Affects `send_versions_cell` return type and `Netinfo`
timestamp captures.

---

## tor-dirclient (+82 -1)

### `src/lib.rs`
**What:** New `RuzstdDecoder` — pure-Rust zstd decoder using `ruzstd`
for WASM (where C `zstd-sys` is unavailable). Gated behind `zstd-wasm`
feature.

### `src/request.rs`
**What:** `all_encodings()` advertises `x-zstd` when `zstd-wasm` is enabled.

---

## tor-circmgr

### `src/hspool.rs`
**What:** `#[allow(clippy::unused_async)]` on `maybe_extend_stem_circuit`.

---

## tor-hsservice (+13 -1)

### `src/lib.rs`
**What:** `PowManager::new()` call uses cfg-gated argument wrapping
for `hs-pow-full` vs stub.

### `src/pow/v1_stub.rs`
**What:** Doc comments and `#[allow(clippy::...)]` on stub methods.

---

## Minor Changes

**WASM cfg guards:**
- **fs-mistrust** `file_access.rs` — `expect(clippy::drop_non_drop)` on WASM
- **hashx** `register.rs` — compiler feature gated with `not(wasm32)`
- **tor-dirmgr** `config.rs`, `err.rs`, `storage.rs` — SQLite/filesystem gated behind `not(wasm32)`
- **tor-rtcompat** `dyn_time.rs`, `impls.rs`, `impls/streamops.rs` — native-only code gated

**Clippy fixes:**
- **tor-memquota** `config.rs` — `expect(identity_op)` on `1 * GIB`; refactored 64-bit check
- **tor-ptmgr** `ipc.rs` — `expect(drop_non_drop)` on WASM

**Cargo.toml changes:**
- **arti-client** — `mmap` feature extracted (optional for WASM); WASM dev-deps added
- **tor-dirmgr** — `rusqlite`/`fslock` behind `cfg(not(wasm32))`; `zstd-wasm` feature
- **tor-rtcompat** — `coarsetime` in native deps; WASM deps (send_wrapper, wasm-bindgen, js-sys, gloo-timers, rustls-rustcrypto, getrandom 0.2 with js)
- **tor-proto** — `js-sys` for WASM (AtomicOptTimestamp)

**Other:**
- **arti** `proxy.rs` — `non_exhaustive` on stub `RpcMgr`
- **arti** `rpc_stub.rs` — removed `visibility::make(pub)` attr
- **arti-client** `lib.rs` — `pub mod storage;` re-export
