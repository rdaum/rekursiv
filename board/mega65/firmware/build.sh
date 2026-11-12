#!/usr/bin/env bash
# Build the disk processor's two programs into OUT_DIR:
#   loader.mem    the first-stage loader, as 32-bit words for $readmemh;
#                 it is built into the bitstream
#   REKURSIV.FW   the main firmware, loaded from the card at start-up
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$(realpath -m "$1")"
mkdir -p "$out"
objcopy="$(ls "$(rustc --print sysroot)"/lib/rustlib/*/bin/llvm-objcopy | head -1)"
cd "$here"
for program in loader:loader.x disk-firmware:firmware.x; do
    cargo rustc -q --release --bin "${program%%:*}" -- -C "link-arg=-T${program##*:}"
    "$objcopy" -O binary "target/riscv32im-unknown-none-elf/release/${program%%:*}" "$out/${program%%:*}.bin"
done
python3 - "$out" <<'PY'
import sys, zlib
out = sys.argv[1]
loader = open(f"{out}/loader.bin", "rb").read()
loader += bytes(-len(loader) % 4)
with open(f"{out}/loader.mem", "w") as f:
    for i in range(0, len(loader), 4):
        f.write(f"{int.from_bytes(loader[i:i+4], 'little'):08x}\n")
firmware = open(f"{out}/disk-firmware.bin", "rb").read()
header = b"RKFW" + len(firmware).to_bytes(4, "little") + zlib.crc32(firmware).to_bytes(4, "little")
open(f"{out}/REKURSIV.FW", "wb").write(header + firmware)
print(f"loader {len(loader)} bytes, firmware {len(firmware)} bytes")
PY
rm "$out/loader.bin" "$out/disk-firmware.bin"
