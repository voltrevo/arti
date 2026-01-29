#!/bin/bash
set -e

# Parse arguments
NATIVE=false
INTEGRATION=false
TARGET_ARGS=""
for arg in "$@"; do
    case $arg in
        --native)
            NATIVE=true
            TARGET_ARGS="--target $(rustc -vV | grep 'host:' | cut -d' ' -f2)"
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

cargo clippy $TARGET_ARGS --all-targets -- -D warnings

if [ "$NATIVE" = true ]; then
    FEATURES=""
    if [ "$INTEGRATION" = true ]; then
        FEATURES="--features integration-tests"
        echo ""
        echo "Running native tests with integration tests ($TARGET_ARGS)..."
    else
        echo ""
        echo "Running native tests ($TARGET_ARGS)..."
    fi
    cargo test --workspace $TARGET_ARGS $FEATURES
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