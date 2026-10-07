#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/lint.sh
scripts/synth.sh
scripts/synth-float.sh
cargo run --locked -p rekursiv-sim -- --example all --request-delay 3 --memory-latency 7 --response-stall 5 --trace-file artifacts/examples.vcd
test -s artifacts/examples.vcd
