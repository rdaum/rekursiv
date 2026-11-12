#!/usr/bin/env bash
# Simulate the board machine on a microprogram and print its debug display.
#   board/mega65/sim.sh [PROGRAM.uc]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
program="$(realpath "${1:-$here/hello.uc}")"
mkdir -p "$here/build"
(cd "$root" && cargo run -q --locked -p rekursiv-model --example boot_stream -- "$program" "$here/build/boot.mem")
cd "$here"
verilator --binary -j 0 -Wno-fatal -Wno-lint -Wno-style --Mdir build/sim --top-module system_tb \
    -I"$root/rtl" -I"$root" -I"$root/vendor/hardfloat/source" -I"$root/vendor/hardfloat/source/RISCV" \
    system_tb.sv rtl/rekursiv_system.sv "$root"/rtl/hardfloat.sv "$root"/rtl/logik*.sv \
    "$root"/rtl/numerik*.sv "$root"/rtl/objekt*.sv > build/sim.log 2>&1 || { tail -30 build/sim.log; exit 1; }
build/sim/Vsystem_tb
