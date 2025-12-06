#!/usr/bin/env node
/**
 * Tor E2E Performance Benchmark
 * 
 * Measures circuit creation time and fetch latency through Tor.
 * Results are reported to console and can be parsed by CI.
 * 
 * Usage:
 *   ./build.sh
 *   node tests/e2e/test-benchmark.mjs [--headed] [--quick]
 * 
 * Options:
 *   --headed  Run with visible browser
 *   --quick   Use WebSocket Snowflake (faster, less censorship resistant)
 */

import { chromium } from 'playwright';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';
import { appendFileSync } from 'fs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '../..');

// Configuration
const CONFIG = {
    serverPort: 8765,
    serverDir: join(projectRoot, 'webtor-demo', 'static'),
    timeout: 180000, // 3 minutes total
    headless: true,
    quick: false,
    testUrl: 'https://api64.ipify.org?format=json', // TLS 1.3 compatible
};

// Parse CLI args
const args = process.argv.slice(2);
if (args.includes('--headed') || args.includes('-h')) {
    CONFIG.headless = false;
}
if (args.includes('--quick') || args.includes('-q')) {
    CONFIG.quick = true;
}

// Threshold for sanity check (fail only on pathological values)
const MAX_CIRCUIT_TIME_MS = 120000; // 2 minutes
const MAX_FETCH_TIME_MS = 60000;    // 1 minute

let serverProcess = null;

async function startServer() {
    return new Promise((resolve, reject) => {
        serverProcess = spawn('python3', ['-m', 'http.server', CONFIG.serverPort.toString()], {
            cwd: CONFIG.serverDir,
            stdio: ['ignore', 'pipe', 'pipe'],
        });

        serverProcess.stdout.on('data', (data) => {
            if (CONFIG.debug) console.log(`[server] ${data}`);
        });

        serverProcess.stderr.on('data', (data) => {
            const msg = data.toString();
            if (msg.includes('Serving HTTP')) {
                console.log(`Server started on port ${CONFIG.serverPort}`);
                resolve();
            }
        });

        serverProcess.on('error', reject);
        
        // Give it time to start
        setTimeout(resolve, 1000);
    });
}

function stopServer() {
    if (serverProcess) {
        serverProcess.kill();
        serverProcess = null;
    }
}

async function runBenchmark() {
    console.log('=== Tor E2E Performance Benchmark ===\n');
    console.log(`Mode: ${CONFIG.quick ? 'Quick (WebSocket)' : 'Full (WebRTC)'}`);
    console.log(`Test URL: ${CONFIG.testUrl}`);
    console.log(`Headless: ${CONFIG.headless}`);
    console.log('');

    await startServer();

    const browser = await chromium.launch({
        headless: CONFIG.headless,
    });

    const context = await browser.newContext();
    const page = await context.newPage();
    
    // Set page timeout for long-running Tor operations
    page.setDefaultTimeout(CONFIG.timeout);

    // Listen to console
    page.on('console', msg => {
        const text = msg.text();
        if (text.includes('benchmark') || text.includes('Circuit') || text.includes('Fetch')) {
            console.log(`[browser] ${text}`);
        }
    });

    try {
        console.log('Loading demo page...');
        await page.goto(`http://localhost:${CONFIG.serverPort}/`, {
            waitUntil: 'networkidle',
            timeout: 30000,
        });

        // Wait for WASM to initialize
        await page.waitForFunction(() => window.webtor_demo !== undefined, {
            timeout: 30000,
        });
        console.log('WASM module loaded');

        // Run benchmark
        console.log(`\nRunning ${CONFIG.quick ? 'quick' : 'full'} benchmark...`);
        
        const benchmarkFn = CONFIG.quick ? 'runQuickBenchmark' : 'runTorBenchmark';
        
        // Note: page.evaluate doesn't accept a timeout option as 3rd arg
        // We set page.setDefaultTimeout() above instead
        const result = await page.evaluate(async ({ fn, url }) => {
            try {
                const result = await window.webtor_demo[fn](url);
                return {
                    success: true,
                    circuit_creation_ms: result.circuit_creation_ms,
                    fetch_latency_ms: result.fetch_latency_ms,
                };
            } catch (e) {
                return {
                    success: false,
                    error: e.toString(),
                };
            }
        }, { fn: benchmarkFn, url: CONFIG.testUrl });

        console.log('\n=== Benchmark Results ===');
        
        if (!result.success) {
            console.error(`FAILED: ${result.error}`);
            process.exit(1);
        }

        const circuitMs = Math.round(result.circuit_creation_ms);
        const fetchMs = Math.round(result.fetch_latency_ms);

        console.log(`Circuit Creation: ${circuitMs} ms`);
        console.log(`Fetch Latency:    ${fetchMs} ms`);
        console.log(`Total:            ${circuitMs + fetchMs} ms`);

        // Output JSON for CI parsing
        const jsonResult = JSON.stringify({
            circuit_creation_ms: circuitMs,
            fetch_latency_ms: fetchMs,
            mode: CONFIG.quick ? 'websocket' : 'webrtc',
            timestamp: new Date().toISOString(),
        });
        console.log(`\nJSON: ${jsonResult}`);

        // Write to GitHub Step Summary if available
        if (process.env.GITHUB_STEP_SUMMARY) {
            const summary = `
### Tor E2E Benchmark Results

| Metric | Value |
|--------|-------|
| Circuit Creation | ${circuitMs} ms |
| Fetch Latency | ${fetchMs} ms |
| Total | ${circuitMs + fetchMs} ms |
| Mode | ${CONFIG.quick ? 'WebSocket' : 'WebRTC'} |
`;
            appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
        }

        // Sanity check - fail only on pathological values
        let failed = false;
        
        if (circuitMs > MAX_CIRCUIT_TIME_MS) {
            console.error(`\nERROR: Circuit creation time (${circuitMs}ms) exceeds threshold (${MAX_CIRCUIT_TIME_MS}ms)`);
            failed = true;
        }
        
        if (fetchMs > MAX_FETCH_TIME_MS) {
            console.error(`\nERROR: Fetch latency (${fetchMs}ms) exceeds threshold (${MAX_FETCH_TIME_MS}ms)`);
            failed = true;
        }

        if (failed) {
            process.exit(1);
        }

        console.log('\n✅ Benchmark passed (within sanity thresholds)');

    } finally {
        await browser.close();
        stopServer();
    }
}

// Run
runBenchmark().catch(err => {
    console.error('Benchmark failed:', err);
    stopServer();
    process.exit(1);
});
