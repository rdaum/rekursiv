#!/usr/bin/env bash
# Build build/REKURSIV.COR for a MEGA65 R6, and the SD card files for a
# microprogram: build/REKURSIV.FW and build/REKURSIV.BOO. The core changes
# only when the hardware does; use push.sh to update the files alone.
#   board/mega65/build.sh [PROGRAM.uc]      default: board/mega65/hello.uc
# Needs Vivado (set VIVADO, default `vivado`) and MEGA65 bit2core (BIT2CORE).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
program="$(realpath "${1:-$here/hello.uc}")"
mkdir -p "$here/build"
"$here/files.sh" "$here/build" "$program"
"${VIVADO:-vivado}" -mode batch -nojournal -log "$here/build/vivado.log" -source "$here/build.tcl" </dev/null
"${BIT2CORE:-bit2core}" mega65r6 "$here/build/rekursiv.bit" rekursiv "$(date +%Y%m%d-%H%M)" "$here/build/REKURSIV.COR"
