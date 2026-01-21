#!/bin/bash
set -e

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