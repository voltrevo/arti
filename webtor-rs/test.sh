#!/bin/bash
set -e

# Parse arguments
NATIVE=false
for arg in "$@"; do
    case $arg in
        --native)
            NATIVE=true
            shift
            ;;
    esac
done

echo "=== Check ==="

cargo check --all-targets

echo "=== Fmt ==="

cargo fmt --all -- --check

echo "=== Clippy ==="

cargo clippy --all-targets -- -D warnings

if [ "$NATIVE" = true ]; then
    NATIVE_TARGET=$(rustc -vV | grep 'host:' | cut -d' ' -f2)
    echo ""
    echo "Running native tests (target: $NATIVE_TARGET)..."
    # Exclude webtor-wasm (WASM-only) and webtor (has stack overflow in test)
    cargo test --workspace --target "$NATIVE_TARGET"
else
    echo ""
    echo "Running wasm-pack tests for all crates..."
    for crate in crates/*/; do
        if [ -f "${crate}Cargo.toml" ]; then
            name=$(basename "$crate")
            echo ""
            echo "=== Testing $name ==="
            wasm-pack test --node "$crate"
        fi
    done
fi

echo ""
echo "All tests passed!"