#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

echo ""
print_status "Setting up WASM build dependencies..."
echo ""

# Check and install Rust if needed
if ! command -v cargo &> /dev/null; then
    print_status "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
else
    print_status "Rust/Cargo is already installed"
fi

# Add wasm32 target if not present
if ! rustup target list | grep -q "wasm32-unknown-unknown (installed)"; then
    print_status "Installing wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
else
    print_status "wasm32-unknown-unknown target is already installed"
fi

# Install wasm-pack if not present
if ! command -v wasm-pack &> /dev/null; then
    print_status "Installing wasm-pack..."
    cargo install --locked wasm-pack
else
    print_status "wasm-pack is already installed"
fi

# Install wasm-opt if not present (optional but recommended for release builds)
if ! command -v wasm-opt &> /dev/null; then
    print_status "Installing wasm-opt for optimization..."
    if cargo install --locked wasm-opt; then
        print_status "wasm-opt installed successfully"
    else
        print_warning "Failed to install wasm-opt (optional, but recommended for optimized builds)"
    fi
else
    print_status "wasm-opt is already installed"
fi

echo ""
print_status "Verifying installation..."
echo ""

cargo --version
rustup target list | grep wasm32-unknown-unknown
wasm-pack --version

if command -v wasm-opt &> /dev/null; then
    wasm-opt --version
else
    print_warning "wasm-opt is not installed (optional)"
fi

echo ""
print_status "WASM setup complete!"
echo ""
print_status "You can now run: ./build.sh"
echo ""
