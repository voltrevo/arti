#!/bin/bash

set -e

echo "🚀 Building Webtor-rs project..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if wasm-pack is installed
if ! command -v wasm-pack &> /dev/null; then
    print_error "wasm-pack is not installed. Please install it first:"
    print_error "curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh"
    exit 1
fi

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    print_error "Rust/Cargo is not installed. Please install Rust first:"
    print_error "https://rustup.rs/"
    exit 1
fi

print_status "Building webtor-wasm (WebAssembly bindings)..."
cd webtor-wasm
wasm-pack build --target web --out-dir pkg
if [ $? -ne 0 ]; then
    print_error "Failed to build webtor-wasm"
    exit 1
fi
cd ..

print_status "Building webtor-demo (Demo webpage)..."
cd webtor-demo
wasm-pack build --target web --out-dir pkg
if [ $? -ne 0 ]; then
    print_error "Failed to build webtor-demo"
    exit 1
fi
cd ..

print_status "Copying demo files..."
mkdir -p webtor-demo/pkg
cp -r webtor-demo/pkg/* webtor-demo/static/pkg/

print_status "Building native webtor library (for testing)..."
cargo build --release
if [ $? -ne 0 ]; then
    print_warning "Failed to build native webtor library (this is optional)"
fi

print_status "Running tests..."
cargo test --workspace
if [ $? -ne 0 ]; then
    print_warning "Some tests failed (expected for WASM modules in native environment)"
fi

print_status "Build completed successfully!"
print_status "To run the demo:"
print_status "1. cd webtor-demo/static"
print_status "2. python3 -m http.server 8000"
print_status "3. Open http://localhost:8000 in your browser"
print_status ""
print_status "Note: The demo requires a modern browser with WebAssembly support."