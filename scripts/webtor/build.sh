#!/bin/bash

set -e

echo "🚀 Building Webtor-rs project..."

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

print_usage() {
    print_status "Usage:"
    print_status "  ./build.sh            # Development build (fast compile, no optimization)"
    print_status "  ./build.sh --release  # Production build (optimized, slower compile)"
}

# Parse arguments
BUILD_MODE="--dev"

while [[ $# -gt 0 ]]; do
    case $1 in
        --dev)
            BUILD_MODE="--dev"
            shift
            ;;
        --release)
            BUILD_MODE="--release"
            shift
            ;;
        *)
            print_error "Unrecognized arg: \"$1\""
            print_usage
            exit 1
            ;;
    esac
done

# Check if required dependencies are installed
check_deps() {
    local missing_deps=()
    
    if ! command -v cargo &> /dev/null; then
        missing_deps+=("Rust/Cargo")
    fi
    
    if ! command -v wasm-pack &> /dev/null; then
        missing_deps+=("wasm-pack")
    fi
    
    if ! rustup target list 2>/dev/null | grep -q "wasm32-unknown-unknown (installed)"; then
        missing_deps+=("wasm32-unknown-unknown target")
    fi
    
    if [ ${#missing_deps[@]} -gt 0 ]; then
        print_error "Missing dependencies: ${missing_deps[*]}"
        print_error ""
        print_error "Run the setup script to install all dependencies:"
        print_error "  ./setup-deps.sh"
        exit 1
    fi
}

check_deps

# Warn if wasm-opt is not available (optional but recommended for release builds)
if [ "$BUILD_MODE" = "--release" ] && ! command -v wasm-opt &> /dev/null; then
    print_warning "wasm-opt is not installed. Install it for better WASM optimization:"
    print_warning "  ./setup-deps.sh"
fi

print_status "Build mode: $BUILD_MODE"

print_status "Building webtor-wasm (WebAssembly bindings)..."
cd crates/webtor-wasm
wasm-pack build --target web --out-dir pkg $BUILD_MODE
if [ $? -ne 0 ]; then
    print_error "Failed to build webtor-wasm"
    exit 1
fi
cd ../..

print_status "Copying WASM to examples..."
mkdir -p examples/showcase/pkg
mkdir -p examples/simple/pkg
cp -r crates/webtor-wasm/pkg/* examples/showcase/pkg/
cp -r crates/webtor-wasm/pkg/* examples/simple/pkg/

# Run wasm-opt if available (for additional size optimization)
optimize_wasm() {
    local path="$1"
    if [ -f "$path" ] && command -v wasm-opt &> /dev/null; then
        print_status "Running wasm-opt on $(basename "$path")..."
        wasm-opt -Oz --strip-dwarf --strip-producers \
            -o "${path}.opt" "$path"
        mv "${path}.opt" "$path"
    fi
}

# Print WASM sizes (uncompressed and gzipped)
print_wasm_size() {
    local path="$1"
    if [ -f "$path" ]; then
        local size=$(ls -lh "$path" | awk '{print $5}')
        local gz_size=$(gzip -c -9 "$path" | wc -c | awk '{printf "%.2f MB", $1/1024/1024}')
        print_status "$(basename "$path"): $size (uncompressed), $gz_size (gzipped)"
    fi
}

# Optimize WASM binaries if wasm-opt is available
if [ "$BUILD_MODE" = "--release" ]; then
    optimize_wasm crates/webtor-wasm/pkg/webtor_wasm_bg.wasm
    # Also optimize the copies in examples
    optimize_wasm examples/showcase/pkg/webtor_wasm_bg.wasm
    optimize_wasm examples/simple/pkg/webtor_wasm_bg.wasm
fi

# Show WASM sizes
echo ""
print_wasm_size crates/webtor-wasm/pkg/webtor_wasm_bg.wasm

echo ""
print_status "Build completed successfully!"
echo ""
print_usage
echo ""
print_status "To run the showcase example:"
print_status "  cd examples/showcase && python3 -m http.server 8000"
print_status "  Open http://localhost:8000 in your browser"
echo ""
print_status "To run the simple React example:"
print_status "  cd examples/simple && npm install && npm run dev"
