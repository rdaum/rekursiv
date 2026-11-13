#!/usr/bin/env bash
# Test the live performance counters and their decimal conversion.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p build
verilator --binary --timing -j 0 -Wno-fatal -Wno-lint -Wno-style --Mdir build/perf_counters_sim \
    --top-module perf_counters_tb sim/perf_counters_tb.sv rtl/perf_counters.sv \
    > build/perf_counters_sim.log 2>&1 || { tail -30 build/perf_counters_sim.log; exit 1; }
build/perf_counters_sim/Vperf_counters_tb
