#!/usr/bin/env node

// webtor-fetch: Make a single anonymous HTTP request through Tor from Node.js
//
// Usage: node index.js [url]
// Example: node index.js https://api.ipify.org?format=json
//
// NOTE: Currently fails during TLS handshake - see README.md

import { readFile } from 'fs/promises';
import { createRequire } from 'module';
import { WebSocket } from 'ws';

// Set up browser-like globals that the WASM module expects
if (!globalThis.WebSocket) {
  globalThis.WebSocket = WebSocket;
}

// Attempt to provide window shim (doesn't fully work due to web_sys bindings)
if (!globalThis.window) {
  globalThis.window = globalThis;
}

// Import webtor after globals are set up
import wasmInit, { init, TorClient, setLogCallback } from 'webtor';

async function main() {
  const url = process.argv[2] || 'https://api.ipify.org?format=json';

  console.log('Loading WASM module...');

  try {
    // Load the WASM file manually (fetch doesn't work with file:// URLs in Node.js)
    const require = createRequire(import.meta.url);
    const wasmPath = require.resolve('webtor/webtor_bg.wasm');
    const wasmBuffer = await readFile(wasmPath);
    await wasmInit(wasmBuffer);

    // Initialize Rust tracing
    init();

    // Log important events
    setLogCallback((level, _target, message) => {
      if (level === 'INFO' || level === 'ERROR' || level === 'WARN') {
        console.log(`[${level}] ${message}`);
      }
    });

    console.log(`\nFetching ${url} via Tor...\n`);

    const startTime = performance.now();

    const response = await TorClient.fetchOneTime(
      'wss://snowflake.pse.dev/',
      url,
      '664A92FF3EF71E03A2F09B1DAABA2DDF920D5194',
      60000,  // connection timeout (ms)
      60000   // circuit timeout (ms)
    );

    const elapsed = ((performance.now() - startTime) / 1000).toFixed(1);

    console.log(`\nStatus: ${response.status}`);
    console.log(`Time: ${elapsed}s`);
    console.log('\nResponse:');
    console.log(response.text());

  } catch (err) {
    console.error('\nError:', err.message || err);
    process.exit(1);
  }
}

main();
