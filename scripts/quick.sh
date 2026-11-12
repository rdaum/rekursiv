#!/usr/bin/env bash
# Day-to-day checks: RTL lint, the RTL-level suites, and the model and
# emulator unit tests, built with the optimized `quick` profile. Guest-image
# boots and Smalltalk regressions stay in scripts/check.sh.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
scripts/lint.sh
cargo test --profile quick --locked \
    -p rekursiv-asm -p rekursiv-model -p rekursiv-devices -p rekursiv-sim
cargo test --profile quick --locked -p rekursiv-emulator --lib \
    --test pager --test pipeline --test metrics
