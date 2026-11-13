#!/usr/bin/env bash
# Test the cached object memory against the SDRAM chip model.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build
verilator --binary --timing -j 0 -Wno-fatal -Wno-lint -Wno-style --Mdir build/object_memory_sim \
    --top-module object_memory_tb sim/object_memory_tb.sv sim/sdram_model.sv rtl/object_memory.sv rtl/sdram.sv \
    > build/object_memory_sim.log 2>&1 || { tail -30 build/object_memory_sim.log; exit 1; }
build/object_memory_sim/Vobject_memory_tb
