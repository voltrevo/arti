#!/usr/bin/env node

// Make a single anonymous HTTP request through Tor from Node.js
//
// Usage:   examples/webtor-node.js [url]
// Example: examples/webtor-node.js https://check.torproject.org/api/ip

import { readFile } from 'fs/promises';
import { createRequire } from 'module';

async function main() {
  const TorClient = await setup();

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

  // Wait just a little bit so that the last log is our output.
  await new Promise(resolve => setTimeout(resolve, 50));

  console.log(`\nStatus: ${response.status}`);
  console.log(`Time: ${elapsed}s`);
  console.log('Response:');
  console.log(response.text());
}

async function setup() {
  console.log('Loading WASM module...');

  let wasmInit, init, TorClient;
  try {
    ({ default: wasmInit, init, TorClient } = await import('../crates/webtor/pkg/webtor.js'));
  } catch (err) {
    throw new Error(
      'Failed to import webtor. You might need to run scripts/webtor/build.sh [--release].',
      { cause: err },
    );
  }

  // Load the WASM file manually (fetch doesn't work with file:// URLs in Node.js)
  const require = createRequire(import.meta.url);
  const wasmPath = require.resolve('../crates/webtor/pkg/webtor_bg.wasm');
  const wasmBuffer = await readFile(wasmPath);
  await wasmInit(wasmBuffer);

  // Initialize Rust tracing
  init();

  return TorClient;
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
