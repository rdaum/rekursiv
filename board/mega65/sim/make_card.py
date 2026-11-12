#!/usr/bin/env python3
"""Build a simulated SD card: an MBR with one FAT32 partition whose root
directory holds the given files. Each file larger than one sector is split
into two extents, to exercise cluster-chain following.
    make_card.py OUT.img NAME=PATH ...      e.g. REKURSIV.IMG=build/store.img
Needs sfdisk and mkfs.fat (dosfstools)."""
import struct, subprocess, sys

out = sys.argv[1]
files = [a.split("=", 1) for a in sys.argv[2:]]
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
        f.write(data.ljust(512, b"\0"))
    boot = sector(START)
    reserved, fats, fat_size, root = (struct.unpack_from("<H", boot, 0x0e)[0], boot[0x10],
                                      struct.unpack_from("<I", boot, 0x24)[0],
                                      struct.unpack_from("<I", boot, 0x2c)[0])
    fat_start = START + reserved
    data_start = fat_start + fats * fat_size
    directory = sector(data_start + (root - 2))
    slot = next(e for e in range(0, 512, 32) if directory[e] == 0)
    next_cluster = 10
    for name, path in files:
        data = open(path, "rb").read()
        count = max(1, -(-len(data) // 512))
        half = count // 2
        chain = list(range(next_cluster, next_cluster + half)) + \
            list(range(next_cluster + half + 3, next_cluster + 3 + count))
        next_cluster += count + 8
        for i, cluster in enumerate(chain):
            following = chain[i + 1] if i + 1 < len(chain) else 0x0FFFFFFF
            for copy in range(fats):
                s = fat_start + copy * fat_size + cluster // 128
                b = sector(s)
                struct.pack_into("<I", b, (cluster % 128) * 4, following)
                put(s, bytes(b))
            put(data_start + cluster - 2, data[i * 512:(i + 1) * 512])
        base, _, extension = name.upper().partition(".")
        entry = bytearray(32)
        entry[0:11] = base.ljust(8).encode() + extension.ljust(3).encode()
        entry[11] = 0x20
        struct.pack_into("<H", entry, 0x14, chain[0] >> 16)
        struct.pack_into("<H", entry, 0x1a, chain[0] & 0xffff)
        struct.pack_into("<I", entry, 0x1c, len(data))
        directory[slot:slot + 32] = entry
        slot += 32
        print(f"{name}: {len(data)} bytes")
    put(data_start + (root - 2), bytes(directory))
