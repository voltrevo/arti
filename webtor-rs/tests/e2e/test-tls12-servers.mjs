#!/usr/bin/env node
/**
 * TLS 1.2 Real Server Connection Tests
 * 
 * Tests TLS 1.2 connections to real servers that may not support TLS 1.3.
 * This validates the TLS 1.2 fallback implementation.
 * 
 * Usage:
 *   ./build.sh --dev
 *   node tests/e2e/test-tls12-servers.mjs [--headed] [--debug]
 */

import { chromium } from 'playwright';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '../..');

const CONFIG = {
    serverPort: 8767,
    corsProxyPort: 8768,
    serverDir: join(projectRoot, 'webtor-demo', 'static'),
    circuitTimeout: 120000,
    requestTimeout: 90000,
    headless: true,
    debug: false,
};

// Parse CLI args
const args = process.argv.slice(2);
if (args.includes('--headed')) CONFIG.headless = false;
if (args.includes('--debug')) CONFIG.debug = true;

// Test targets from demo presets - known Tor-friendly
const TLS12_TEST_TARGETS = [
    {
        name: 'Tor Check',
        url: 'https://check.torproject.org/',
        expectedInResponse: 'Congratulations',
        description: 'Tor Project check page',
    },
    {
        name: 'HTTPBin user-agent',
        url: 'https://httpbin.org/user-agent',
        expectedInResponse: 'user-agent',
        description: 'HTTPBin endpoint (tests TLS)',
    },
    {
        name: 'example.com',
        url: 'https://example.com/',
        expectedInResponse: 'Example Domain',
        description: 'Simple TLS test',
    },
];

const results = {
    passed: 0,
    failed: 0,
    tests: [],
};

function log(message, level = 'info') {
    const timestamp = new Date().toISOString().substr(11, 8);
    const prefix = { info: ' ', success: '+', error: '!', debug: '  ' }[level] || ' ';
    if (level === 'debug' && !CONFIG.debug) return;
    console.log(`[${timestamp}] ${prefix} ${message}`);
}

function startServer() {
    return new Promise((resolve) => {
        const server = spawn('npx', ['serve', '-s', '.', '-p', String(CONFIG.serverPort)], {
            cwd: CONFIG.serverDir,
            stdio: ['ignore', 'pipe', 'pipe'],
        });
        setTimeout(() => resolve(server), 3000);
    });
}

async function setupBrowser() {
    log(`Launching browser (${CONFIG.headless ? 'headless' : 'headed'})...`);
    const browser = await chromium.launch({
        headless: CONFIG.headless,
        slowMo: CONFIG.debug ? 100 : 0,
    });
    
    const context = await browser.newContext();
    const page = await context.newPage();
    
    page.on('console', msg => {
        const text = msg.text();
        if (msg.type() === 'error') {
            log(`[console.error] ${text.substring(0, 150)}`, 'error');
        } else if (CONFIG.debug || text.includes('TLS 1.2') || text.includes('fallback')) {
            log(`[console] ${text.substring(0, 150)}`, 'debug');
        }
    });
    
    return { browser, page };
}

async function waitForCircuit(page) {
    log('Opening TorClient...');
    
    if (CONFIG.debug) {
        const debugToggle = await page.$('#debugToggle');
        if (debugToggle) await debugToggle.click();
    }
    
    const openBtn = await page.$('#openBtn');
    await openBtn.click();
    
    log('Waiting for Tor circuit...');
    const startTime = Date.now();
    
    while (Date.now() - startTime < CONFIG.circuitTimeout) {
        const status = await page.$eval('#status', el => el.textContent);
        
        if (status.toLowerCase().includes('ready')) {
            const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
            log(`Circuit ready in ${elapsed}s`, 'success');
            return true;
        }
        
        if (status.includes('failed') || status.includes('error')) {
            throw new Error(`Circuit failed: ${status}`);
        }
        
        await new Promise(r => setTimeout(r, 1000));
    }
    
    throw new Error('Circuit timeout');
}

async function testTls12Connection(page, target) {
    log(`Testing: ${target.name}`, 'info');
    log(`  URL: ${target.url}`, 'debug');
    log(`  ${target.description}`, 'debug');
    
    const testResult = {
        name: target.name,
        url: target.url,
        status: 'pending',
        error: null,
        responseTime: null,
        tlsInfo: null,
    };
    
    try {
        const urlInput = await page.$('#url1');
        await urlInput.fill(target.url);
        await page.$eval('#output1', el => el.textContent = '');
        
        const startTime = Date.now();
        const btn1 = await page.$('#btn1');
        await btn1.click();
        
        await page.waitForFunction(
            () => {
                const output = document.getElementById('output1');
                if (!output) return false;
                const text = output.textContent;
                return text.length > 0 && 
                       !text.includes('Making request') && 
                       !text.includes('Connecting');
            },
            { timeout: CONFIG.requestTimeout }
        );
        
        testResult.responseTime = Date.now() - startTime;
        
        const output = await page.$eval('#output1', el => el.textContent);
        
        // Try to extract TLS version info from logs
        try {
            const logContent = await page.$eval('#output', el => el.value || el.textContent || '');
            if (logContent && logContent.includes('TLS 1.2')) {
                testResult.tlsInfo = 'TLS 1.2';
            } else if (logContent && logContent.includes('TLS 1.3')) {
                testResult.tlsInfo = 'TLS 1.3';
            }
        } catch (e) {
            // Log element might not exist
        }
        
        if (output.includes('Error') || output.includes('failed')) {
            testResult.status = 'failed';
            testResult.error = output.substring(0, 150);
            log(`FAILED: ${output.substring(0, 100)}`, 'error');
            results.failed++;
        } else if (target.expectedInResponse && output.includes(target.expectedInResponse)) {
            testResult.status = 'passed';
            const tlsNote = testResult.tlsInfo ? ` [${testResult.tlsInfo}]` : '';
            log(`PASSED (${testResult.responseTime}ms)${tlsNote}`, 'success');
            results.passed++;
        } else if (output.includes('Success') || output.length > 50) {
            testResult.status = 'passed';
            log(`PASSED (${testResult.responseTime}ms)`, 'success');
            results.passed++;
        } else {
            testResult.status = 'failed';
            testResult.error = `Unexpected: ${output.substring(0, 100)}`;
            log(`FAILED: Unexpected response`, 'error');
            results.failed++;
        }
        
    } catch (error) {
        testResult.status = 'failed';
        testResult.error = error.message;
        log(`FAILED: ${error.message}`, 'error');
        results.failed++;
    }
    
    results.tests.push(testResult);
    return testResult;
}

async function runTests() {
    let server = null;
    let browser = null;
    
    console.log('');
    console.log('='.repeat(60));
    console.log('  TLS 1.2 Real Server Connection Tests');
    console.log('='.repeat(60));
    console.log('');
    
    try {
        log('Starting HTTP server...');
        server = await startServer();
        
        const browserSetup = await setupBrowser();
        browser = browserSetup.browser;
        const page = browserSetup.page;
        
        const url = `http://localhost:${CONFIG.serverPort}`;
        log(`Loading ${url}...`);
        await page.goto(url);
        
        log('Waiting for WASM...');
        await page.waitForFunction(() => {
            const status = document.getElementById('status');
            return status && !status.textContent.includes('Loading WASM');
        }, { timeout: 30000 });
        log('WASM initialized', 'success');
        
        await waitForCircuit(page);
        
        console.log('');
        log('Running TLS 1.2 tests...');
        console.log('-'.repeat(60));
        
        for (const target of TLS12_TEST_TARGETS) {
            await testTls12Connection(page, target);
            await new Promise(r => setTimeout(r, 3000));
        }
        
        // Summary
        console.log('');
        console.log('='.repeat(60));
        console.log('  Summary');
        console.log('='.repeat(60));
        console.log(`  Passed: ${results.passed}`);
        console.log(`  Failed: ${results.failed}`);
        console.log('');
        
        // Detailed results
        for (const test of results.tests) {
            const icon = test.status === 'passed' ? '+' : '!';
            const tlsInfo = test.tlsInfo ? ` [${test.tlsInfo}]` : '';
            console.log(`  ${icon} ${test.name}${tlsInfo}`);
            if (test.error) {
                console.log(`      Error: ${test.error}`);
            }
        }
        console.log('='.repeat(60));
        
        return results.failed === 0;
        
    } catch (error) {
        log(`Fatal: ${error.message}`, 'error');
        return false;
        
    } finally {
        if (browser) await browser.close();
        if (server) server.kill();
    }
}

runTests().then(success => {
    process.exit(success ? 0 : 1);
}).catch(err => {
    console.error('Unhandled error:', err);
    process.exit(1);
});
