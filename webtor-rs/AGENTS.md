# Agents Guide

## Build Commands

```bash
# Check native build
cargo check --package webtor

# Check WASM build
cargo check --package webtor --target wasm32-unknown-unknown
cargo check --package webtor-wasm --target wasm32-unknown-unknown

# Build WASM demo (includes embedded consensus)
wasm-pack build webtor-demo --target web --out-dir pkg --release

# Run example locally (alternative, requires npm)
cd example && npm install && npm run dev
```

## Test Commands

```bash
# Unit tests
cargo test -p webtor

# Property-based tests (proptest)
cargo test -p webtor proptest

# E2E tests (requires network)
npm run test:tls

# Criterion microbenchmarks
cargo bench -p webtor --bench circuit_params

# Fuzz tests (requires nightly)
cd subtle-tls/fuzz && cargo +nightly fuzz run fuzz_server_hello
```

## Project Structure

- `webtor/` - Core Tor client library
- `webtor-wasm/` - WASM bindings for webtor
- `webtor-demo/` - Demo application library
- `example/` - Web demo (Vite + TypeScript)
- `subtle-tls/` - SubtleCrypto-based TLS for WASM
- `scripts/` - Build and consensus fetch scripts

## Key Files

- `webtor/src/client.rs` - Main TorClient implementation
- `webtor/src/circuit.rs` - Circuit management
- `webtor/src/directory.rs` - Consensus fetching and parsing
- `webtor/src/relay.rs` - Relay selection
- `webtor/src/cached/` - Embedded consensus data (updated daily)

## Architecture Notes

- For WASM builds, consensus is embedded at compile time from `webtor/src/cached/`
- The daily GitHub Action (`daily-consensus-update.yml`) refreshes cached consensus
- Snowflake bridge is used for WASM; WebTunnel is available for native builds
- `ring` crate doesn't compile to WASM, so `subtle-tls` provides TLS via SubtleCrypto

## Style Preferences

- **No emojis** in documentation, README, or markdown files - use plain text instead
- Use plain ASCII characters (`+`, `-`, `|`) instead of Unicode box-drawing characters (┌, ─, │, etc.) for diagrams in markdown - they render more reliably across platforms
- Use `[x]` for checkboxes, not emoji checkmarks

## Version Bumping

- **Always bump the UI version** in `webtor-demo/static/index.html` (footer) on any UI-related changes
- Current version format: `v0.X.Y`

## Release Workflow

- **NEVER release when CI is failing** - always verify all checks pass first
- **Create a release after every PR merged** to this repo (only if CI passes)
- **Every release must include a build artifact** (not just source code)
- Steps:
  1. **Verify CI passes**: `gh pr view <PR> --json statusCheckRollup` or check GitHub Actions
  2. Update CHANGELOG.md (move Unreleased to new version)
  3. Commit and push changes
  4. **Wait for CI to pass on main** before tagging
  5. Create and push git tag: `git tag vX.Y.Z && git push origin main --tags`
  6. Build WASM: `wasm-pack build webtor-demo --target web --out-dir pkg --release`
  7. Package build: `cd webtor-demo && zip -r ../webtor-demo-vX.Y.Z.zip pkg/`
  8. Create release with artifact: `gh release create vX.Y.Z --title "vX.Y.Z" --notes "..." webtor-demo-vX.Y.Z.zip`
