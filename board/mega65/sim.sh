#!/usr/bin/env bash
# Simulate the board machine on a microprogram and print its debug display.
# The simulated machine has a 1,024-entry pager table, so paging tests stay short.
#   board/mega65/sim.sh [PROGRAM.uc]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
program="$(realpath "${1:-$here/hello.uc}")"
mkdir -p "$here/build/sim"
"$here/files.sh" "$here/build/sim" "$program" 1024
(cd "$here/firmware" && cargo run -q --target x86_64-unknown-linux-gnu --example mkstore -- "$here/build/sim/store.img" 4 16384)
"$here/sim/make_card.py" "$here/build/sim/card.img" REKURSIV.IMG="$here/build/sim/store.img" \
    REKURSIV.FW="$here/build/sim/REKURSIV.FW" REKURSIV.BOO="$here/build/sim/REKURSIV.BOO"
cd "$here"
verilator --binary --timing -j 0 -Wno-fatal -Wno-lint -Wno-style --Mdir build/sim/obj --top-module system_tb \
    -I"$root/rtl" -I"$root" -I"$root/vendor/hardfloat/source" -I"$root/vendor/hardfloat/source/RISCV" \
    system_tb.sv sim/sd_card_model.sv sim/sdram_model.sv rtl/rekursiv_system.sv rtl/display.sv rtl/workstation.sv rtl/keyboard.sv rtl/mouse.sv rtl/object_memory.sv rtl/sdram.sv rtl/disk_processor.sv rtl/sd_spi.sv \
    "$root"/vendor/picorv32/picorv32.v "$root"/rtl/hardfloat.sv "$root"/rtl/logik*.sv \
    "$root"/rtl/numerik*.sv "$root"/rtl/objekt*.sv > build/sim/verilator.log 2>&1 || { tail -30 build/sim/verilator.log; exit 1; }
build/sim/obj/Vsystem_tb "${@:2}"
