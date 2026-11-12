#!/usr/bin/env bash
# Run the SD controller against the card model.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
mkdir -p build
python3 -c '
import sys
data = bytearray()
for sector in range(64):
    data += bytes((sector * 7 + i) & 0xff for i in range(512))
open("build/sd_test.img", "wb").write(data)
'
verilator --binary --timing -j 0 -Wno-fatal -Wno-lint -Wno-style --Mdir build/sd_sim \
    --top-module sd_spi_tb sim/sd_spi_tb.sv sim/sd_card_model.sv rtl/sd_spi.sv \
    > build/sd_sim.log 2>&1 || { tail -30 build/sd_sim.log; exit 1; }
build/sd_sim/Vsd_spi_tb "$@"
