#!/bin/bash

set -euo pipefail

cargo_strict() {
    echo
    echo cargo_strict "$@"
    echo "==============================================================================================="
    RUSTFLAGS="${RUSTFLAGS:-} -D warnings" cargo "$@"
}

cargo_strict check
cargo_strict check --all-features
cargo_strict check -p arti-client --no-default-features --target wasm32-unknown-unknown

cargo_strict clippy
cargo_strict clippy --all-features
cargo_strict clippy -p arti-client --no-default-features --target wasm32-unknown-unknown

echo
echo "====================="
echo "= All checks passed ="
echo "====================="
echo
