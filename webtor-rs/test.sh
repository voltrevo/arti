#!/bin/bash
set -e

echo "=== Check ==="

cargo check --all-targets

echo "=== Fmt ==="

cargo fmt --all -- --check

echo "=== Clippy ==="

cargo clippy --all-targets -- -D warnings

echo "Running wasm-pack tests for all crates..."

for crate in crates/*/; do
    if [ -f "${crate}Cargo.toml" ]; then
        name=$(basename "$crate")
        echo ""
        echo "=== Testing $name ==="
        wasm-pack test --node "$crate"
    fi
done

echo ""
echo "All tests passed!"