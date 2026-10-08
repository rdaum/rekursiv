#!/usr/bin/env bash
# Target-family resource mapping; this is not placement, routing, or timing closure.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
precision=${1:-32}
case "$precision" in 32|64) ;; *) echo "usage: $0 [32|64]" >&2; exit 2 ;; esac
mkdir -p artifacts
yosys -Q -T -p "read_verilog -sv -Irtl -I. -Ivendor/hardfloat/source -Ivendor/hardfloat/source/RISCV rtl/numerik_fp${precision}.sv rtl/hardfloat.sv; synth_xilinx -family xcup -top numerik_fp${precision} -noiopad; check -assert; stat; write_json artifacts/numerik-fp${precision}-xcup.json" > artifacts/numerik-fp${precision}-xcup.log 2>&1
tail -27 artifacts/numerik-fp${precision}-xcup.log
