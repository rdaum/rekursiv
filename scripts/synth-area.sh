#!/usr/bin/env bash
# UltraScale+ primitive counts for the complete processor and OBJEKT wrapper.
# Object RAM is external. No board memory controller, PCIe, or placement is included.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
mkdir -p artifacts
if ! yosys -Q -T -p 'read_verilog -sv -DREKURSIV_SIM_MEMORY_WORDS=16777216 -Irtl -I. -Ivendor/hardfloat/source -Ivendor/hardfloat/source/RISCV objekt_tb.sv rtl/hardfloat.sv rtl/logik.sv rtl/logik_io.sv rtl/logik_sequencer.sv rtl/logik_stacks.sv rtl/logik_store.sv rtl/numerik.sv rtl/numerik_alu.sv rtl/numerik_fp32.sv rtl/numerik_fp64.sv rtl/objekt.sv rtl/objekt_directory.sv rtl/objekt_exchange.sv rtl/objekt_gc.sv rtl/objekt_transfer.sv; synth_xilinx -family xcup -top objekt_tb -noiopad; check -assert; stat; write_json artifacts/rekursiv-xcup.json' > artifacts/rekursiv-xcup.log 2>&1; then
    tail -100 artifacts/rekursiv-xcup.log
    exit 1
fi
tail -45 artifacts/rekursiv-xcup.log
