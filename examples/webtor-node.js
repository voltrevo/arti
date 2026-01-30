#!/usr/bin/env node

// Make a single anonymous HTTP request through Tor from Node.js
//
// Usage:   examples/webtor-node.js [url]
// Example: examples/webtor-node.js https://check.torproject.org/api/ip

import { readFile } from 'fs/promises';
import { createRequire } from 'module';

import wasmInit, { init, TorClient } from '../crates/webtor/pkg/webtor.js';

async function main() {
  await setup();

  const url = process.argv[2] ?? 'https://check.torproject.org/api/ip';

  console.log(`\nFetching ${url} via Tor...\n`);

  const startTime = performance.now();

  const response = await TorClient.fetchOneTime(
    'wss://snowflake.pse.dev/',
    url,
    '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194',
    60000,  // connection timeout (ms)
    60000,  // circuit timeout (ms)
  );

  const elapsed = ((performance.now() - startTime) / 1000).toFixed(1);

  console.log(`Status: ${response.status}`);
  console.log(`Time: ${elapsed}s`);
  console.log('Response:');
  console.log(response.text());
}

async function setup() {
  console.log('Loading WASM module...');

  // Load the WASM file manually (fetch doesn't work with file:// URLs in Node.js)
  const require = createRequire(import.meta.url);
  const wasmPath = require.resolve('../crates/webtor/pkg/webtor_bg.wasm');
  const wasmBuffer = await readFile(wasmPath);
  await wasmInit(wasmBuffer);

  // Initialize Rust tracing
  init();
}

main().catch(err => {
  console.error('\nError:', err.message || err);
  process.exit(1);
});
