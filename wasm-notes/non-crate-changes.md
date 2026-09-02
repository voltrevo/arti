# Non-Crate Changes: `wasm-basic-compat` → `main`

Changes outside of `crates/` (excluding `wasm-notes/`).

---

## Root config

### `Cargo.toml` (+4 -2)
**What:** Adds `crates/tor-js` to workspace members. Reorders
`tor-async-compat` slightly (moved before `retry-error`).

### `Cargo.lock` (+338 -16)
**What:** New dependencies pulled in by tor-js and WASM support
(wasm-bindgen, js-sys, gloo-timers, web-sys, ruzstd, etc.).

### `clippy.toml` (+3 -1)
**What:** Adds `allow-invalid = true` to the `rand::Rng::random_range`
disallowed method entry, with a comment explaining that on wasm32,
`rsa 0.9` pulls in `rand 0.8` via `num-bigint-dig`, so clippy can't
resolve the `rand 0.9` path.

### `.gitignore` (+1)
**What:** Adds `node_modules` (for tor-js TypeScript wrapper).

---

## Scripts

### New file: `scripts/check.sh` (+24)
**What:** Runs `cargo check` and `cargo clippy` across three
configurations: default features, `--all-features`, and
`-p tor-js --target wasm32-unknown-unknown`. All with `-D warnings`.

**Why:** Single script to verify the codebase compiles cleanly on both
native and WASM targets.

### New file: `scripts/todos.sh` (+15)
**What:** Greps the codebase for TODO/FIXME comments and summarizes them.

### New file: `scripts/tor-js/build.sh` (+46)
**What:** Builds the tor-js WASM package: runs `wasm-pack build`,
copies README, then builds the TypeScript wrapper via `npm install`
and `npm run build`.

### New file: `scripts/tor-js/push-hash-artifact.sh` (+131)
**What:** Publishes WASM binary to a `hash-artifacts` git branch
for CDN distribution. The binary is stored by its SHA-256 hash so
the CDN entry point can integrity-check it.

---

## GitHub Actions

### New file: `.github/workflows/showcase.yml` (+54)
**What:** GitHub Actions workflow to build and deploy the tor-js
browser showcase to GitHub Pages.

---

## Examples (tor-js)

### `examples/tor-js/tor-fetch.js` (+105)
**What:** Node.js CLI example: fetches a URL through Tor with
persistent filesystem storage. Supports `--websocket <gateway>` and
`--in-memory` flags.

### `examples/tor-js/tor-fetch-abort.js` (+138)
**What:** Tests AbortSignal support: pre-aborted signal, mid-stream
abort, and unused signal.

### `examples/tor-js/tor-fetch-streaming.js` (+102)
**What:** Streams a large file through Tor, counting text matches
incrementally. Demonstrates the streaming Response API.

### `examples/tor-js/tor-fetch-singleton.js` (+26)
**What:** Minimal example using the singleton API (`tor.fetch()`).

### `examples/tor-js/showcase/` (+1167)
**What:** Browser demo — single-page HTML app with inline CSS/JS
that connects to Tor and makes requests. Includes `run.sh` (local
HTTP server) and `tor.svg` logo.
