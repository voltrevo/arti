#!/bin/bash
set -e

# Parse arguments
NATIVE=false
INTEGRATION=false
for arg in "$@"; do
    case $arg in
        --native)
            NATIVE=true
            shift
            ;;
        --integration)
            INTEGRATION=true
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
    FEATURES=""
    if [ "$INTEGRATION" = true ]; then
        FEATURES="--features integration-tests"
        echo ""
        echo "Running native tests with integration tests (target: $NATIVE_TARGET)..."
    else
        echo ""
        echo "Running native tests (target: $NATIVE_TARGET)..."
    fi
    cargo test --workspace --target "$NATIVE_TARGET" $FEATURES
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