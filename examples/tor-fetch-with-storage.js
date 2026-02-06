#!/usr/bin/env node

// Make an HTTP request through Tor with persistent filesystem storage
//
// Build:   scripts/tor-js/build.sh
// Usage:   examples/tor-fetch-with-storage.js [url]
// Example: examples/tor-fetch-with-storage.js https://check.torproject.org/api/ip
//
// State is persisted to ~/.local/state/tor-js/
// Subsequent runs will load cached state for faster bootstrap.

import { readFile, writeFile, unlink, readdir, mkdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

// ============================================================================
// FilesystemStorage - TorStorage implementation using Node.js fs
// ============================================================================

class FilesystemStorage {
  constructor(baseDir) {
    this.baseDir = baseDir;
  }

  async init() {
    if (!existsSync(this.baseDir)) {
      await mkdir(this.baseDir, { recursive: true });
      console.log(`Created storage directory: ${this.baseDir}`);
    }
  }

  // Encode key to be filesystem-safe
  keyToPath(key) {
    const encoded = encodeURIComponent(key);
    return join(this.baseDir, encoded);
  }

  async get(key) {
    const path = this.keyToPath(key);
    try {
      return await readFile(path, 'utf-8');
    } catch (err) {
      if (err.code === 'ENOENT') return null;
      throw err;
    }
  }

  async set(key, value) {
    const path = this.keyToPath(key);
    await writeFile(path, value, 'utf-8');
  }

  async delete(key) {
    const path = this.keyToPath(key);
    try {
      await unlink(path);
    } catch (err) {
      if (err.code !== 'ENOENT') throw err;
    }
  }

  async keys(prefix) {
    try {
      const files = await readdir(this.baseDir);
      return files
        .map(f => decodeURIComponent(f))
        .filter(k => k.startsWith(prefix));
    } catch (err) {
      if (err.code === 'ENOENT') return [];
      throw err;
    }
  }
}

// ============================================================================
// Main
// ============================================================================

async function main() {
  const { TorClient, TorClientOptions, init } = await setup();

  const url = process.argv[2] ?? 'https://check.torproject.org/api/ip';
  const storageDir = join(homedir(), '.local', 'state', 'tor-js');

  // Initialize filesystem storage
  const storage = new FilesystemStorage(storageDir);
  await storage.init();

  console.log(`\nStorage: ${storageDir}`);
  console.log(`Creating TorClient with persistent storage...\n`);

  const startTime = performance.now();

  // Create options with WebSocket Snowflake and filesystem storage
  const options = new TorClientOptions(
    'wss://snowflake.pse.dev/',
    '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194'
  ).withStorage(storage);

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
    const __dirname = dirname(fileURLToPath(import.meta.url));
    const module = await import(join(__dirname, '../crates/tor-js/pkg/tor_js.js'));
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
