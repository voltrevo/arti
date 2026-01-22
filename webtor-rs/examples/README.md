# webtor-rs Examples

This directory contains example applications demonstrating how to use webtor-rs.

## Examples

### showcase - Comprehensive WASM Demo

Location: `examples/showcase/`

A full-featured web application demonstrating all webtor-wasm capabilities:
- Connect via Snowflake WebSocket or WebRTC bridges
- Build circuits and fetch content through Tor
- Real-time circuit status monitoring
- Circuit management (update, view relays)
- Isolated requests (one-time circuits)

**Running locally:**
```bash
# Build WASM (from repository root)
./examples/build-wasm.sh

# Serve the example
cd examples/showcase
python3 -m http.server 8080
# Open http://localhost:8080
```

**Live demo:** https://igor53627.github.io/webtor-rs/

### simple - Minimal React Example

Location: `examples/simple/`

A minimal React + TypeScript example showing basic webtor-wasm integration:
- Simple TorClient initialization
- Basic HTTP fetch through Tor
- Error handling

**Running locally:**
```bash
# Install dependencies
cd examples/simple
npm install

# Start dev server (builds WASM automatically)
npm run dev
```

## Building WASM

Both examples use the same webtor-wasm package. Build it with:

```bash
# From repository root
./examples/build-wasm.sh

# Or manually
wasm-pack build crates/webtor-wasm --target web --out-dir ../../examples/showcase/pkg
wasm-pack build crates/webtor-wasm --target web --out-dir ../../examples/simple/pkg
```

## Architecture

```
JavaScript App
    |
    v
webtor-wasm (WASM bindings)
    |
    v
webtor (core Tor client)
```

Both examples use the webtor-wasm JavaScript API directly, eliminating unnecessary abstraction layers.

## Using webtor-wasm in Your Project

Install from npm:
```bash
npm install webtor-wasm
```

Or use the CDN build:
```html
<script type="module">
  import init, { TorClient, TorClientOptions } from 'https://webtor-wasm.53627.org/webtor-wasm/latest/webtor_wasm.js';
  await init();
  
  const options = new TorClientOptions("wss://snowflake.torproject.net/");
  const client = await new TorClient(options);
  await client.waitForCircuit();
  
  const response = await client.fetch("https://check.torproject.org/api/ip");
  console.log(await response.text());
</script>
```

## API Documentation

See the webtor-wasm crate documentation for the full API reference.
