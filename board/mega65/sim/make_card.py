#!/usr/bin/env python3
"""Build a simulated SD card: an MBR with one FAT32 partition holding
REKURSIV.IMG, deliberately split into two extents.
    make_card.py OUT.img STORE.img
STORE.img (from mkstore) becomes the file's contents.
Needs sfdisk and mkfs.fat (dosfstools)."""
import struct, subprocess, sys

out = sys.argv[1]
store = open(sys.argv[2], "rb").read()
SECTORS = 81920            # 40 MB
START = 2048               # partition start sector
open(out, "wb").truncate(SECTORS * 512)
subprocess.run(["/usr/sbin/sfdisk", "-q", out], input=f"{START},,c\n", text=True, check=True)
subprocess.run(["/usr/sbin/mkfs.fat", "-F", "32", "-s", "1", "--offset", str(START), "-n", "TESTCARD",
                out, str((SECTORS - START) // 2)], check=True, capture_output=True)

with open(out, "r+b") as f:
    def sector(n):
        f.seek(n * 512)
        return bytearray(f.read(512))
    def put(n, data):
        f.seek(n * 512)
        f.write(data)
    boot = sector(START)
    reserved, fats, fat_size, root = (struct.unpack_from("<H", boot, 0x0e)[0], boot[0x10],
                                      struct.unpack_from("<I", boot, 0x24)[0],
                                      struct.unpack_from("<I", boot, 0x2c)[0])
    fat_start = START + reserved
    data_start = fat_start + fats * fat_size
    count = len(store) // 512
    half = count // 2
    chain = list(range(10, 10 + half)) + list(range(20 + half, 20 + count))
    for i, cluster in enumerate(chain):
        nxt = chain[i + 1] if i + 1 < len(chain) else 0x0FFFFFFF
        for copy in range(fats):
            s = fat_start + copy * fat_size + cluster // 128
            b = sector(s)
            struct.pack_into("<I", b, (cluster % 128) * 4, nxt)
            put(s, b)
    # Root directory: a volume label entry exists already; add ours after it.
    d = sector(data_start + (root - 2))
    slot = next(e for e in range(0, 512, 32) if d[e] == 0)
    entry = bytearray(32)
    entry[0:11] = b"REKURSIVIMG"
    entry[11] = 0x20
    struct.pack_into("<H", entry, 0x14, chain[0] >> 16)
    struct.pack_into("<H", entry, 0x1a, chain[0] & 0xffff)
    struct.pack_into("<I", entry, 0x1c, len(chain) * 512)
    d[slot:slot + 32] = entry
    put(data_start + (root - 2), d)
    for i, cluster in enumerate(chain):
        put(data_start + cluster - 2, store[i * 512:(i + 1) * 512])
    print(f"REKURSIV.IMG: {len(chain)} sectors in 2 extents")
