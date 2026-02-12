#!/usr/bin/env node

// Make an HTTP request through Tor from Node.js using arti-client via tor-js
//
// Build:   scripts/tor-js/build.sh
// Usage:   examples/tor-fetch.js [url]
// Example: examples/tor-fetch.js https://check.torproject.org/api/ip
//
// Uses in-memory storage (state is lost when the process exits).
// For persistent storage, see tor-fetch-with-storage.js.

// ============================================================================
// MemoryStorage - TorStorage implementation using a Map
// ============================================================================

class MemoryStorage {
  constructor() {
    this.data = new Map();
  }

  async get(key) {
    return this.data.get(key) ?? null;
  }

  async set(key, value) {
    this.data.set(key, value);
  }

  async delete(key) {
    this.data.delete(key);
  }

  async keys(prefix) {
    return [...this.data.keys()].filter(k => k.startsWith(prefix));
  }
}

// ============================================================================
// Main
// ============================================================================

async function main() {
  const { TorClient, TorClientOptions, init } = await setup();

  const url = process.argv[2] ?? 'https://check.torproject.org/api/ip';

  console.log(`\nCreating TorClient (arti-client based)...\n`);

  const startTime = performance.now();

  // Create options with WebSocket Snowflake and in-memory storage
  // The pse.dev bridge accepts non-browser connections (Node.js)
  const options = new TorClientOptions(
    'wss://snowflake.pse.dev/',
    '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194'  // pse.dev bridge fingerprint
  ).withStorage(new MemoryStorage());

  // Create client (returns Promise)
  const client = await new TorClient(options);

  const connectTime = ((performance.now() - startTime) / 1000).toFixed(1);
  console.log(`\nConnected in ${connectTime}s, fetching ${url}...\n`);

  // Make fetch request
  const fetchStart = performance.now();
  const response = await client.fetch(url);
  const fetchTime = ((performance.now() - fetchStart) / 1000).toFixed(1);

  // Cleanup
  await client.close();

  // Wait just a little bit so that the last log is our output
  await new Promise(resolve => setTimeout(resolve, 50));

  console.log(`\nStatus: ${response.status}`);
  console.log(`Connect time: ${connectTime}s`);
  console.log(`Fetch time: ${fetchTime}s`);
  console.log('Response:');
  console.log(response.text());
}

async function setup() {
  console.log('Loading WASM module...');

  let initWasm, init, TorClient, TorClientOptions;
  try {
    // Web target: default export is WASM initializer, named exports are the API
    const module = await import('../crates/tor-js/pkg/tor_js.js');
    initWasm = module.default;
    init = module.init;
    TorClient = module.TorClient;
    TorClientOptions = module.TorClientOptions;
  } catch (err) {
    throw new Error(
      'Failed to import tor-js. Run: scripts/tor-js/build.sh',
      { cause: err },
    );
  }

  // Initialize WASM module (required for web target)
  // In Node.js, read the WASM file directly since fetch may not work with file:// URLs
  const { readFile } = await import('node:fs/promises');
  const { fileURLToPath } = await import('node:url');
  const { dirname, join } = await import('node:path');

  const __dirname = dirname(fileURLToPath(import.meta.url));
  const wasmPath = join(__dirname, '../crates/tor-js/pkg/tor_js_bg.wasm');
  const wasmBytes = await readFile(wasmPath);
  await initWasm(wasmBytes);

  // Initialize tor-js (sets up panic hook and tracing)
  init();

  return { TorClient, TorClientOptions, init };
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
