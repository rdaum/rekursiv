#!/usr/bin/env bash
# Build the disk processor firmware and write it as 32-bit words for $readmemh.
#   firmware/build.sh OUT.mem
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
out="$(realpath -m "$1")"
(cd "$here" && cargo build -q --release)
elf="$here/target/riscv32im-unknown-none-elf/release/disk-firmware"
objcopy="$(ls "$(rustc --print sysroot)"/lib/rustlib/*/bin/llvm-objcopy | head -1)"
"$objcopy" -O binary "$elf" "$out.bin"
python3 - "$out.bin" "$out" <<'PY'
import sys
data = open(sys.argv[1], "rb").read()
data += bytes(-len(data) % 4)
if len(data) > 65536:
    sys.exit(f"firmware is {len(data)} bytes; the disk processor has 65536")
with open(sys.argv[2], "w") as f:
    for i in range(0, len(data), 4):
        f.write(f"{int.from_bytes(data[i:i+4], 'little'):08x}\n")
PY
rm "$out.bin"
