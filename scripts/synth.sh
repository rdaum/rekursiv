#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
mkdir -p artifacts
if ! yosys -Q -T -p 'read_verilog -sv -Irtl -I. -Ivendor/hardfloat/source -Ivendor/hardfloat/source/RISCV rtl/objekt.sv rtl/objekt_transfer.sv rtl/objekt_exchange.sv rtl/objekt_directory.sv rtl/objekt_gc.sv; hierarchy -check -top objekt; synth -top objekt; check -assert; stat; write_json artifacts/objekt.json' > artifacts/yosys-synth.log 2>&1; then
    cat artifacts/yosys-synth.log
    exit 1
fi
tail -40 artifacts/yosys-synth.log

# Keep inferred memories in the generic netlist. The full control store holds
# two million bits; memory_map would expand those into individual flip-flops
# and muxes. FPGA technology mapping selects their physical implementation.
echo 'Synthesizing LOGIK/NUMERIK: 8192 control words, inferred memories retained'
if ! yosys -Q -T -p 'read_verilog -sv -Irtl -I. -Ivendor/hardfloat/source -Ivendor/hardfloat/source/RISCV rtl/logik.sv rtl/logik_io.sv rtl/logik_store.sv rtl/logik_stacks.sv rtl/logik_sequencer.sv rtl/numerik.sv rtl/numerik_alu.sv rtl/numerik_fp32.sv rtl/hardfloat.sv; hierarchy -check -top logik -chparam CODE_WORDS 8192; synth -top logik -run begin:fine; opt -full; techmap; opt -fast; abc -fast; opt -fast; hierarchy -check; check -assert; stat; write_json artifacts/logik.json' > artifacts/yosys-logik.log 2>&1; then
    cat artifacts/yosys-logik.log
    exit 1
fi
tail -40 artifacts/yosys-logik.log
