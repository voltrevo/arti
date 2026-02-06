# tor-js

WebAssembly bindings for [arti-client](https://gitlab.torproject.org/tpo/core/arti), the official Tor Project client library written in Rust.

## Features

- Make HTTP/HTTPS requests through the Tor network
- Snowflake pluggable transport (WebSocket and WebRTC modes)
- Works in browsers and Node.js
- TypeScript definitions included

## Installation

Build the WASM package:

```bash
scripts/tor-js/build.sh
```

The package will be available at `crates/tor-js/pkg/`.

## Usage

### Browser

```javascript
import initWasm, { init, TorClient, TorClientOptions, setLogCallback } from './pkg/tor_js.js';

// Initialize WASM module
await initWasm();
init();

// Optional: receive log messages
setLogCallback((level, target, message) => {
    console.log(`[${level}] ${message}`);
});

// Create client with Snowflake bridge
const options = new TorClientOptions(
    'wss://snowflake.pse.dev/',
    '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194'  // pse.dev bridge fingerprint
);

const client = await new TorClient(options);

// Make a request through Tor
const response = await client.fetch('https://check.torproject.org/api/ip');
console.log(response.status);      // 200
console.log(response.text());      // {"IsTor":true,"IP":"..."}

// Clean up
await client.close();
```

### Node.js

```javascript
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

// Load WASM module
const module = await import('./pkg/tor_js.js');
const __dirname = dirname(fileURLToPath(import.meta.url));
const wasmBytes = await readFile(join(__dirname, './pkg/tor_js_bg.wasm'));
await module.default(wasmBytes);
module.init();

// Create client and fetch (same as browser)
const options = new module.TorClientOptions(
    'wss://snowflake.pse.dev/',
    '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194'
);
const client = await new module.TorClient(options);
const response = await client.fetch('https://check.torproject.org/api/ip');
console.log(response.text());
await client.close();
```

## API

### `init()`

Initialize the WASM module. Must be called before creating any `TorClient` instances.

### `setLogCallback(callback)`

Set a callback to receive log messages from Rust.

```javascript
setLogCallback((level, target, message) => {
    // level: "ERROR", "WARN", "INFO", "DEBUG", "TRACE"
    // target: Rust module path
    // message: Log message
});
```

### `TorClientOptions`

Options for creating a TorClient.

```javascript
// WebSocket Snowflake (works in browsers and Node.js)
new TorClientOptions(snowflakeUrl, fingerprint)

// WebRTC Snowflake (browser only, more censorship resistant)
TorClientOptions.snowflakeWebRtc(fingerprint)
```

**Parameters:**
- `snowflakeUrl` - WebSocket URL (e.g., `wss://snowflake.pse.dev/`)
- `fingerprint` - Bridge fingerprint (40-character hex string)

**Known bridges:**
| Bridge | URL | Fingerprint |
|--------|-----|-------------|
| pse.dev | `wss://snowflake.pse.dev/` | `664A92FF3EF71E03A2F09B1DAABA2DDF920D5194` |
| torproject.net | `wss://snowflake.torproject.net/` | `2B280B23E1107BB62ABFC40DDCC8824814F80A72` |

Note: The pse.dev bridge accepts non-browser connections (Node.js), while torproject.net may reject them.

### `TorClient`

```javascript
const client = await new TorClient(options);
```

#### `client.fetch(url, init?)`

Make an HTTP request through Tor.

```javascript
const response = await client.fetch('https://example.com', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ key: 'value' })
});
```

**Parameters:**
- `url` - URL to fetch
- `init` - Optional fetch options:
  - `method` - HTTP method (GET, POST, PUT, DELETE, etc.)
  - `headers` - Request headers object
  - `body` - Request body (string, Uint8Array, or ArrayBuffer)

**Returns:** `JsHttpResponse`

#### `client.close()`

Close the client and release resources.

### `JsHttpResponse`

```javascript
response.status   // HTTP status code (number)
response.headers  // Response headers (object)
response.body     // Response body (Uint8Array)
response.url      // Final URL after redirects (string)
response.text()   // Body as UTF-8 string
response.json()   // Body parsed as JSON
```

## Examples

### Node.js CLI

```bash
# Fetch URL through Tor
node examples/tor-fetch.js https://check.torproject.org/api/ip
```

### Browser Demo

```bash
# Build and serve the demo
scripts/tor-js/build.sh
examples/tor-js-showcase/run.sh
# Open http://localhost:8000
```

## Build Options

```bash
# Default: web target (ES modules for browsers and modern runtimes)
scripts/tor-js/build.sh

# With specific target
scripts/tor-js/build.sh --target web      # ES modules (default)
scripts/tor-js/build.sh --target nodejs   # CommonJS for Node.js
scripts/tor-js/build.sh --target bundler  # ES modules for webpack, etc.

# Release build (optimized)
scripts/tor-js/build.sh --release
```

## Architecture

tor-js wraps [arti-client](https://gitlab.torproject.org/tpo/core/arti) (the official Tor implementation in Rust) with:

- **Snowflake transport** via [webtor-rs-lite](../webtor-rs-lite) - pluggable transport for censorship circumvention
- **TLS 1.3** via [subtle-tls](../subtle-tls) - pure Rust/WASM TLS using WebCrypto
- **WASM runtime** via [tor-rtcompat](../tor-rtcompat) - async runtime for WebAssembly

## License

MIT OR Apache-2.0
