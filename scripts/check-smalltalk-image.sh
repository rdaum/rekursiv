#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
python3 scripts/fetch-smalltalk-image.py
export REKURSIV_ST80_DIR="$PWD/artifacts/st80"
cargo test --locked -p rekursiv-smalltalk --test import
cargo test --locked -p rekursiv-smalltalk --test import -- --ignored
cargo test --locked -p rekursiv-smalltalk --test execution -- --ignored
# Original arithmetic methods traverse the complete image's class graph. Keep
# the full differential checks, but optimize their large record comparisons.
cargo test --release --locked -p rekursiv-smalltalk --test sends -- --ignored
cargo test --release --locked -p rekursiv-smalltalk --test startup -- --ignored
cargo test --release --locked -p rekursiv-emulator --no-default-features --test startup -- --ignored
cargo run --locked -p rekursiv-smalltalk -- import \
    "$REKURSIV_ST80_DIR/VirtualImage" "$REKURSIV_ST80_DIR/rekursiv-image.json"
cargo run --locked -p rekursiv-smalltalk -- audit \
    "$REKURSIV_ST80_DIR/VirtualImage" "$REKURSIV_ST80_DIR/primitive-audit.json"
