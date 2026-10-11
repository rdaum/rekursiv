#!/usr/bin/env bash
# Small synthetic programs only: no guest-image boots or long regressions.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
cargo test --release --locked -p rekursiv-devices --lib idle_tests
cargo test --release --locked -p rekursiv-emulator \
  --test jit --test jit_objekt --test jit_stack_fetch
