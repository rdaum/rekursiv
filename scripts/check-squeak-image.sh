#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
python3 scripts/fetch-squeak-image.py
export REKURSIV_SQUEAK_DIR="$PWD/artifacts/squeak-1.1"
cargo test --locked -p rekursiv-smalltalk --test squeak -- --include-ignored
cargo run --locked -p rekursiv-smalltalk -- audit-squeak \
    "$REKURSIV_SQUEAK_DIR/Squeak1.1.image" "$REKURSIV_SQUEAK_DIR/inventory.json"
cargo run --locked -p rekursiv-smalltalk -- import-squeak \
    "$REKURSIV_SQUEAK_DIR/Squeak1.1.image" "$REKURSIV_SQUEAK_DIR/rekursiv-image.json"

# Executes the saved context, restores the colour desktop and opens/dismisses a menu.
cargo test --release --locked -p rekursiv-emulator --test squeak -- --include-ignored
