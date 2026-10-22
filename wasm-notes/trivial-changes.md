# Trivial Changes

Changes that are purely mechanical fixes with no behavioral impact.

- **tor-config-path** `addr.rs` — `return Err(...)` → `Err(...)` (clippy `needless_return`)
- **tor-ptmgr** `ipc.rs` — `#[cfg_attr(target_arch = "wasm32", expect(clippy::drop_non_drop))]` on a `drop()` call
