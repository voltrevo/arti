#!/bin/bash

set -euo pipefail

cd "$(dirname "$0")"
GIT_ROOT=$(git rev-parse --show-toplevel)

if [ ! -d "$GIT_ROOT/crates/tor-js/pkg" ]; then
    echo "Error: pkg directory not found at $GIT_ROOT/crates/tor-js/pkg"
    echo "Run: scripts/tor-js/build.sh"
    exit 1
fi

rm -rf pkg
cp -a "$GIT_ROOT"/crates/tor-js/pkg pkg

python3 -m http.server 8000
