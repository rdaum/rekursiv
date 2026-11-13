#!/usr/bin/env bash
# Build the files that boot Smalltalk-80 on the board into OUT_DIR:
#   REKURSIV.FW   disk processor firmware
#   REKURSIV.BOO  boot stream: interpreter, collector and boot services
#   REKURSIV.IMG  object store holding the converted image
# plus loader.mem, which is built into the bitstream.
#   smalltalk.sh OUT_DIR [VirtualImage [STORE_MIB]]
# VirtualImage defaults to artifacts/st80/VirtualImage, which
# scripts/fetch-smalltalk-image.py downloads. Copy the three files to the
# card's root directory.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$(realpath -m "$1")"
image="$(realpath "${2:-$root/artifacts/st80/VirtualImage}")"
"$here/firmware/build.sh" "$out"
# The board's machine: 32,768 pager entries, 2^23 words, a 640x480 display.
(cd "$root" && cargo run -q --release --locked -p rekursiv-emulator --example board_smalltalk -- \
    "$image" "$out" 32768 8388608 640 480)
(cd "$here/firmware" && cargo run -q --target x86_64-unknown-linux-gnu --example mkstore -- \
    "$out/REKURSIV.IMG" "${3:-32}" 262144 "$out/records.bin")
rm "$out/records.bin"
