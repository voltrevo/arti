# webtor-fetch

Experimental Node.js example that attempts to make a single HTTP request through Tor using the WASM module.

## Current Status: Not Yet Working

The WASM module successfully loads in Node.js and establishes a WebSocket connection to Snowflake, but fails during TLS handshake because the `subtle-tls` crate uses browser-specific `web_sys::window()` API calls that don't work in Node.js.

**What works:**
- WASM module loads and initializes
- WebSocket connects to Snowflake bridge
- Turbo, KCP, SMUX layers initialize
- TLS handshake starts

**What doesn't work:**
- TLS handshake fails with "No window" error
- The `subtle-tls` crate needs to be updated to use `globalThis.crypto` instead of `window.crypto`

## Setup

```bash
cd examples/webtor-fetch
npm install
```

## Usage

```bash
node index.js [url]
```

## How it would work (once fixed)

The script uses `TorClient.fetchOneTime()` which:
1. Creates a fresh Tor circuit via Snowflake (WebSocket)
2. Makes the HTTP request through the circuit
3. Returns the response and closes the circuit

## Fix Required

The `subtle-tls` crate needs to be updated to support Node.js by:
1. Using `globalThis.crypto.subtle` instead of `window.crypto.subtle`
2. Or checking for both `window` and `globalThis` as crypto sources

Files that need changes:
- `crates/subtle-tls/src/handshake.rs:642`
- `crates/subtle-tls/src/cert.rs:17`

## Requirements

- Node.js 18+ (for native crypto.subtle and WebSocket)
- The `ws` package provides WebSocket polyfill
