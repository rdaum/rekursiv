#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
verilator --lint-only -Wall -Irtl -I. -Ivendor/hardfloat/source -Ivendor/hardfloat/source/RISCV --top-module objekt_tb objekt_tb.sv rtl/objekt.sv rtl/objekt_transfer.sv rtl/objekt_exchange.sv rtl/objekt_directory.sv rtl/objekt_gc.sv rtl/logik.sv rtl/logik_io.sv rtl/logik_store.sv rtl/logik_stacks.sv rtl/logik_sequencer.sv rtl/numerik.sv rtl/numerik_alu.sv rtl/numerik_fp32.sv rtl/numerik_fp64.sv rtl/hardfloat.sv
verilator --lint-only -Wall -Irtl -I. -Ivendor/hardfloat/source -Ivendor/hardfloat/source/RISCV --top-module objekt -GPAGER_BITS=1 -GMEMORY_WORDS=32 rtl/objekt.sv rtl/objekt_transfer.sv rtl/objekt_exchange.sv rtl/objekt_directory.sv rtl/objekt_gc.sv
verilator --lint-only -Wall -Irtl -I. -Ivendor/hardfloat/source -Ivendor/hardfloat/source/RISCV --top-module logik -GCODE_WORDS=16 -GSTACK_WORDS=4 -GNAM_WORDS=16 rtl/logik.sv rtl/logik_io.sv rtl/logik_store.sv rtl/logik_stacks.sv rtl/logik_sequencer.sv rtl/numerik.sv rtl/numerik_alu.sv rtl/numerik_fp32.sv rtl/numerik_fp64.sv rtl/hardfloat.sv
