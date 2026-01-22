#!/bin/bash
set -e

echo "Building webtor-wasm for examples..."

# Build webtor-wasm once
wasm-pack build crates/webtor-wasm --target web --release

# Copy to showcase
echo "Copying to examples/showcase/pkg..."
mkdir -p examples/showcase/pkg
cp -r crates/webtor-wasm/pkg/* examples/showcase/pkg/

# Copy to simple
echo "Copying to examples/simple/pkg..."
mkdir -p examples/simple/pkg
cp -r crates/webtor-wasm/pkg/* examples/simple/pkg/

echo "Done! WASM build available in both examples."
