# Object store on the SD card

Status: implemented by the board's disk processor firmware (`board/mega65/firmware/src/store.rs`),
which is tested on the host against the simulator's in-memory store. `mkstore` creates empty files.

The MEGA65 board keeps committed object records in one file, `REKURSIV.IMG`, on the card's FAT32
filesystem. The board's disk processor serves OBJEKT's
[streaming backing-store interface](interface.md#streaming-backing-store-interface) from this file.
The format is independent of the processor that serves it, so a microcode implementation can replace
the current RISC-V firmware and read the same files.

## Sharing the card

The card also holds MEGA65 cores, ROMs, and other files. The store never writes outside its own file
and never changes the filesystem: no directory entries, no FAT entries, no file size. A host tool
creates the file at its full size before first use. The file need not be contiguous.

At every start the disk processor finds the file in the root directory, follows its cluster chain,
and records the file's extents (runs of consecutive sectors). After that, the store addresses the
file by its own sector numbers, starting at zero. Each start repeats the lookup, because another
system may have rewritten the card in between.

## Layout

All multi-byte fields are little-endian. A machine word occupies eight bytes: its 40 bits in the low
five bytes, then three zero bytes. Positions below are file sector numbers.

| Sectors     | Region       | Contents                                                    |
| ----------- | ------------ | ----------------------------------------------------------- |
| 0–1         | Superblocks  | Two alternating copies of the store's parameters            |
| 2–(2+J−1)   | Journal      | The intent list of the most recent commit                   |
| following D | Directory    | One 4-byte index block sector per 64 identities, or zero    |
| upward      | Log          | Records, appended in commit order from the log start        |
| downward    | Index blocks | One sector of 64 index entries each, allocated from the end |

J and D are fixed when the file is created: the directory covers identities below the capacity, at
one sector per 8,192 identities, so 2^26 identities need 4 MB. An index block is allocated, from the
end of the file downward, the first time an identity in its range of 64 is saved. The index
therefore grows with the identities actually saved, not with the identity range. Records may not
grow into the index blocks; the superblock records the boundary.

### Superblock

| Offset | Size | Field                                                             |
| -----: | ---: | ----------------------------------------------------------------- |
|      0 |    8 | Magic `RKSTORE1`                                                  |
|      8 |    4 | Format version, currently 2                                       |
|     12 |    4 | Generation; the copy with the larger valid generation is current  |
|     16 |    4 | Journal start sector and                                          |
|     20 |    4 | journal length in sectors                                         |
|     24 |    4 | Directory start sector                                            |
|     28 |    4 | Capacity: the directory covers identities below this              |
|     32 |    4 | Log start sector                                                  |
|     36 |    4 | Log end sector (exclusive)                                        |
|     40 |    4 | Log head: the next free log sector                                |
|     44 |    4 | Journal state: 0 clean, 1 holds a complete commit not yet applied |
|     48 |    4 | Highest identity that has ever had a committed record             |
|     52 |    4 | Index top: the lowest sector of the index blocks                  |
|    508 |    4 | CRC-32 of bytes 0–507                                             |

The two copies are sectors 0 and 1. Every superblock write goes to the copy that is not current, so
a torn write leaves the previous copy intact. A new file has generation 1 in sector 0.

### Journal

|   Offset | Size | Field                                                                                                                  |
| -------: | ---: | ---------------------------------------------------------------------------------------------------------------------- |
|        0 |    8 | Magic `RKJOURNL`                                                                                                       |
|        8 |    4 | Entry count, at most 126                                                                                               |
| 16 + 16i |    8 | Identity of entry i (bits 31:0), its index block sector (62:32), and in bit 63 whether this entry allocates that block |
| 24 + 16i |    8 | New index entry for that identity                                                                                      |
| 16 + 16n |    4 | CRC-32 of every byte before it                                                                                         |

The journal spans four sectors; a commit writes only the sectors its entries use.

### Index entry

An entry is zero when the identity has no committed record. Otherwise it holds the record's first
log sector in bits 31:0 and its length in sectors in bits 55:32. Bits 63:56 are zero.

### Record

A record starts on a sector boundary.

| Offset | Size | Field                                    |
| -----: | ---: | ---------------------------------------- |
|      0 |    8 | Magic `RKRECORD`                         |
|      8 |    8 | Reference (machine word)                 |
|     16 |    8 | Class (machine word)                     |
|     24 |    4 | Body size in words                       |
|     28 |    4 | Flags: bit 0 cond                        |
|     32 |    8 | Commit generation that wrote this record |
|     40 |  8×n | Body words                               |

The record is padded to a whole sector. ReadWord for offset k reads byte 40 + 8k.

## Commits

CommitSave outside a batch, and CommitBatch, are commits. A commit is atomic: after a crash, either
every record in it is visible or none is.

1. BeginSave and WriteWord stream a record to free log sectors beyond the head. Nothing visible
   changes.
2. At commit, the disk processor chooses the index block of every record in the commit, allocating
   new blocks below the index top, and writes the journal: the identities, their new index entries
   and blocks, with a CRC.
3. It writes the superblock copy that is not current, with a larger generation, the new log head,
   the new index top, and journal state 1. This write is the commit point.
4. For each entry that allocates a block, it zeroes the block and enters it in the directory. It
   writes the new index entries, then a superblock with journal state 0. Repeating this step after a
   crash gives the same result.

At start, a current superblock with journal state 1 means step 4 did not finish. The disk processor
replays the journal before serving requests. AbortBatch and a reset before step 3 discard the staged
records: the log head never advanced past them.

## Lookups

FindRecord and Metadata read one directory entry and one index entry, then the record's first
sector. ReadWord reads one sector of the record. NextRecord scans index entries upward from the
given identity, skipping ranges of 64 identities that have no index block, and stops at the highest
identity recorded in the superblock. A one-sector cache serves repeated reads of directory, index
and record sectors.

## Limits of this version

- The log is never compacted. Replaced records stay in the file until a host tool rewrites it. When
  the log is full, saves fail with a store error.
- Each commit writes the journal and a superblock, so small commits cost several sector writes.
- There is no checksum on records; the journal and superblocks carry the CRCs.
- A batch holds at most 126 records, the journal's capacity.
- An identity at or above the capacity cannot be saved. Identities are never reused, so a long
  session approaches it; at 2^26 a heavily used session lasts about a day.
- A record must leave room below the index top for one commit's worth of new index blocks.
