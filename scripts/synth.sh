#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
mkdir -p artifacts
if ! yosys -Q -T -p 'read_verilog -sv -Irtl rtl/objekt.sv rtl/objekt_transfer.sv rtl/objekt_gc.sv; hierarchy -check -top objekt; synth -top objekt; check -assert; stat; write_json artifacts/objekt.json' > artifacts/yosys-synth.log 2>&1; then
    cat artifacts/yosys-synth.log
    exit 1
fi
tail -40 artifacts/yosys-synth.log

if ! yosys -Q -T -p 'read_verilog -sv -Irtl rtl/logik.sv rtl/logik_store.sv rtl/logik_stacks.sv rtl/logik_sequencer.sv rtl/numerik.sv rtl/numerik_alu.sv; hierarchy -check -top logik; synth -top logik; check -assert; stat; write_json artifacts/logik.json' > artifacts/yosys-logik.log 2>&1; then
    cat artifacts/yosys-logik.log
    exit 1
fi
tail -40 artifacts/yosys-logik.log
