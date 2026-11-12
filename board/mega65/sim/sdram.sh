#!/usr/bin/env bash
# Run the SDRAM controller against the chip model.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build
verilator --binary --timing -j 0 -Wno-fatal -Wno-lint -Wno-style --Mdir build/sdram_sim \
    --top-module sdram_tb sim/sdram_tb.sv sim/sdram_model.sv rtl/sdram.sv \
    > build/sdram_sim.log 2>&1 || { tail -30 build/sdram_sim.log; exit 1; }
build/sdram_sim/Vsdram_tb
