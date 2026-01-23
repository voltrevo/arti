#!/usr/bin/env bash
set -euo pipefail

# Code coverage script using cargo-llvm-cov
# Generates HTML reports in target/llvm-cov/html/
#
# Usage:
#   ./coverage.sh          # Generate HTML report
#   ./coverage.sh --open   # Generate and open in browser

# Check if cargo-llvm-cov is installed
if ! command -v cargo-llvm-cov &> /dev/null; then
    echo "cargo-llvm-cov not found. Installing..."
    cargo install cargo-llvm-cov
fi

# Ensure llvm-tools component is installed
if ! rustup component list --installed | grep -q llvm-tools; then
    echo "Installing llvm-tools component..."
    rustup component add llvm-tools
fi

# Detect the native target (this project defaults to wasm32, so we override)
NATIVE_TARGET=$(rustc -vV | grep 'host:' | cut -d' ' -f2)
echo "Using native target: $NATIVE_TARGET"

echo "Running tests with coverage instrumentation..."
# Note: webtor crate excluded due to stack overflow in test_tor_client_creation
cargo llvm-cov --workspace --exclude webtor --exclude webtor-wasm --target "$NATIVE_TARGET" --html --output-dir target/llvm-cov

echo ""
echo "Coverage report generated at: target/llvm-cov/html/index.html"

if [[ "${1:-}" == "--open" ]]; then
    xdg-open "target/llvm-cov/html/index.html" 2>/dev/null || open "target/llvm-cov/html/index.html" 2>/dev/null || echo "Could not open browser automatically"
fi