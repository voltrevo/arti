# Work In Progress: Snowflake Directory Download Stalling

## Problem Summary

Directory downloads via Snowflake bridge stall at ~700KB after ~26 seconds. SMUX window updates are being sent correctly and acknowledged by the bridge, but data stops flowing from the bridge side.

## Running Test Examples

### Native Snowflake Test

Tests arti-client bootstrap via native (non-WASM) Snowflake WebSocket transport.

```bash
# With trace logging to file
RUST_LOG=trace cargo run --example readme_snowflake_native -p arti-client --features experimental-api 2>&1 | tee /tmp/snowflake_native.log

# With info logging only
RUST_LOG=info cargo run --example readme_snowflake_native -p arti-client --features experimental-api

# With timeout (recommended - stalls after ~30s)
timeout 45 bash -c 'RUST_LOG=info cargo run --example readme_snowflake_native -p arti-client --features experimental-api'
```

### WASM Snowflake Test

Tests arti-client with Snowflake transport in browser environment.

```bash
# 1. Build the WASM example
cargo build --example wasm_snowflake --target wasm32-unknown-unknown \
    --no-default-features --features pt-client,experimental-api -p arti-client

# 2. Generate JS bindings with wasm-bindgen
wasm-bindgen --target web --out-dir crates/arti-client/examples/wasm_snowflake_web \
    target/wasm32-unknown-unknown/debug/examples/wasm_snowflake.wasm

# 3. Serve with HTTP server
cd crates/arti-client/examples/wasm_snowflake_web
python3 -m http.server 8080

# 4. Open in browser:
# http://localhost:8080/
```

## Current Findings

1. **SMUX window updates ARE being sent** - consumed values progress from 42KB to 698KB
2. **Bridge acknowledges our updates** - KCP ACKs received
3. **Data stops after ~700KB** - only NOP keepalives arrive every 10 seconds
4. **Last data packet**: KCP sn=542 at 20:43:26.124
5. **Last window update**: consumed=698095, window=65535 at 20:43:26.125

## Key Log Patterns to Search

```bash
# Window updates sent
grep "queueing window update" /tmp/snowflake_native.log

# Window updates received from peer
grep "received UPD" /tmp/snowflake_native.log

# SENDME cells (Tor flow control)
grep "FlowCtrlUpdate.*Sendme" /tmp/snowflake_native.log

# KCP sequence numbers
grep "recv sn=" /tmp/snowflake_native.log | tail -20

# NOP keepalives (stall indicator)
grep "received NOP" /tmp/snowflake_native.log
```

## Root Cause Found

**Window sizes were WAY too small compared to Snowflake's settings!**

| Setting | Our Value | Snowflake Value |
|---------|-----------|-----------------|
| KCP snd_wnd | 128 | 65535 |
| KCP rcv_wnd | 128 | 65535 |
| SMUX window | 64KB | 1MB |

The small windows caused flow control backpressure that stalled data transfer.

## Fix Applied

1. `kcp_stream.rs`: Changed `snd_wnd` and `rcv_wnd` from 128 to 65535
2. `smux.rs`: Changed `DEFAULT_WINDOW` from 65535 (64KB) to 1048576 (1MB)

## Sources

- [Snowflake KCP/SMUX config](https://gitlab.torproject.org/tpo/anti-censorship/pluggable-transports/snowflake/-/blob/main/client/lib/snowflake.go)
- [Snowflake buffer size tuning discussion](https://archive.torproject.org/websites/lists.torproject.org/pipermail/anti-censorship-team/2021-July/000178.html)