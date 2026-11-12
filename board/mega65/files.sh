#!/usr/bin/env bash
# Build the files the board loads from its SD card into OUT_DIR:
#   REKURSIV.FW   disk processor firmware
#   REKURSIV.BOO  boot stream for PROGRAM.uc
# plus loader.mem, which is built into the bitstream.
#   files.sh OUT_DIR PROGRAM.uc [PAGER_ENTRIES]
# PAGER_ENTRIES (default 16384, the board's table) is a constant for programs.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$(realpath -m "$1")"
program="$(realpath "$2")"
"$here/firmware/build.sh" "$out"
(cd "$root" && cargo run -q --locked -p rekursiv-model --example boot_stream -- "$program" "$out/REKURSIV.BOO" 8192 "${3:-16384}")
