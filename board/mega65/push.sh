#!/usr/bin/env bash
# Update a running setup without rebuilding or flashing the core: build the
# firmware and the boot stream for a microprogram and copy them to the
# MEGA65's SD card over Ethernet. The MEGA65 must be in its standard core
# with remote mode on. Then boot the rekursiv core's slot.
#   push.sh [PROGRAM.uc]        default: hello.uc
# Needs mega65_ftp (set MEGA65_FTP, default `mega65_ftp`).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
"$here/files.sh" "$here/build/card" "${1:-$here/hello.uc}"
cd "$here/build/card"
"${MEGA65_FTP:-mega65_ftp}" -e -c "put REKURSIV.FW REKURSIV.FW" -c "put REKURSIV.BOO REKURSIV.BOO" -c "exit" \
    | grep -E "Uploaded [0-9]+ bytes in|CRIT|ERRO" || true
