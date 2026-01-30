#!/usr/bin/env node

// Make an HTTP request through Tor from Node.js using a TorClient instance
//
// Usage:   examples/webtor-node.js [url]
// Example: examples/webtor-node.js https://check.torproject.org/api/ip

import { readFile } from 'fs/promises';
import { createRequire } from 'module';

async function main() {
  const { TorClient, TorClientOptions } = await setup();

  const url = process.argv[2] ?? 'https://check.torproject.org/api/ip';

  console.log(`\nCreating TorClient...\n`);

  const startTime = performance.now();

  // FIXME: Avoid class for options. Should use plain object.
  const options = new TorClientOptions('wss://snowflake.pse.dev/');
  // FIXME: also specify:
  // '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194', // fingerprint
  // 60000,  // connection timeout (ms)
  // 60000,  // circuit timeout (ms)

  // FIXME: new TorClient really does return a promise (which is wrong)
  const client = await new TorClient(options);

  const connectTime = ((performance.now() - startTime) / 1000).toFixed(1);
  console.log(`\nConnected in ${connectTime}s, fetching ${url}...\n`);

  const fetchStart = performance.now();
  const response = await client.fetch(url);
  const fetchTime = ((performance.now() - fetchStart) / 1000).toFixed(1);

  await client.close();

  // Wait just a little bit so that the last log is our output.
  await new Promise(resolve => setTimeout(resolve, 50));

  console.log(`\nStatus: ${response.status}`);
  console.log(`Connect time: ${connectTime}s`);
  console.log(`Fetch time: ${fetchTime}s`);
  console.log('Response:');
  console.log(response.text());
}

async function setup() {
  console.log('Loading WASM module...');

  let wasmInit, init, TorClient, TorClientOptions;
  try {
    ({ default: wasmInit, init, TorClient, TorClientOptions } = await import('../crates/webtor/pkg/webtor.js'));
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

  init();

  return { TorClient, TorClientOptions };
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
