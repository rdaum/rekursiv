#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
python3 scripts/fetch-smalltalk-image.py
export REKURSIV_ST80_DIR="$PWD/artifacts/st80"
cargo test --locked -p rekursiv-smalltalk --test import
cargo test --locked -p rekursiv-smalltalk --test import -- --ignored
cargo test --locked -p rekursiv-smalltalk --test execution -- --ignored
cargo run --locked -p rekursiv-smalltalk -- import \
    "$REKURSIV_ST80_DIR/VirtualImage" "$REKURSIV_ST80_DIR/rekursiv-image.json"
