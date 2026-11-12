# Memory hierarchy for the MEGA65

Status: design, partly built. Object bodies are in SDRAM (build step 1). The pager is still the
1,024-entry LUT-RAM table and still defines residency.

## Problem

Today an object is resident only while it holds a pager slot. The pager is direct-mapped, so the
1,025th allocation evicts the first object, and evicting a new or modified object saves it to the SD
card at about 20 ms per save. A Smalltalk image has tens of thousands of live objects. It would
spend its time paging through the card while most of the board's memory sat unused.

The goal is a hierarchy in which each level is larger and slower than the one above, and the SD card
is reached only when an object is truly not in memory.

## Levels

Metadata (where an object is, its class and size) and bodies (its words) are cached separately.

| Level  | Metadata                                      | Bodies                          | Cost of a hit      |
| ------ | --------------------------------------------- | ------------------------------- | ------------------ |
| L0     | Selected-object and prepared-access registers |                                 | 0 cycles           |
| L1     | Pager: 16,384 entries in block RAM            | Line cache in block RAM (later) | 1 cycle            |
| Memory | Object table: 262,144 entries in SDRAM        | All resident bodies in SDRAM    | about 10–20 cycles |
| Store  | Index in `REKURSIV.IMG`                       | Records in `REKURSIV.IMG`       | milliseconds       |

Cycle costs are estimates for a controller on the machine clock. They must be measured.

**Residency changes meaning.** An object is resident when its body is in SDRAM and the object table
has its entry. The pager becomes a pure cache of the object table. A pager miss is no longer an
architectural event: OBJEKT refills the slot from the table and the command proceeds. `NotResident`
means the object is not in memory at all.

## L0: registers

The selected snapshot and the prepared access already cache one entry each. Today every command
revalidates them by reading the pager. Instead they become authoritative: the single pager write
port already knows every replacement, invalidation, and relocation, and it invalidates or updates a
matching L0 register on the same edge. Commands on the selected object then need no pager read.
Field writes that change metadata (the modified flag) write through to L1 and the object table.

## L1 metadata: block-RAM pager

- 16,384 entries of 172 bits: about 86 RAMB36. 32,768 entries would need about 172.
- Two ports. Port A serves command lookups and all writes. Port B serves transfers, the collector,
  walks, and debug reads.
- Reads are registered. A page command (selecting a new object) takes one more cycle than today.
  Commands on the selected object are unchanged, because L0 serves them.
- On a miss, OBJEKT reads the object table entry, reads the body's first word for the cached
  representation, and installs the slot. Whatever the slot held is simply overwritten: the table
  remains the record of residency, so no save occurs.
- Collector commit and any other whole-table change clear every valid bit. Slots refill on demand.

This replaces the 1,024-entry LUT-RAM pager, which costs about 60,000 LUTs.

## Memory: SDRAM

The R6 carries a 64 MB SDRAM (IS42S16320F: 32M × 16 bits, 8,192 rows × 1,024 columns × 4 banks).

**Word packing.** One 40-bit machine word occupies four 16-bit columns, read or written as one burst
of four. That gives 2^23 words (8,388,608) with simple shifts for addressing, and a burst never
crosses a row. Packing three columns per word would give 11.2 million words, but words would
straddle rows and addressing would need a divide. The spare 24 bits per word are unused in bodies.

**Controller.** It sits behind OBJEKT's existing memory request and completion channel, which
tolerates any latency, so the core's body path does not change. It also offers OBJEKT a raw 64-bit
port for the object table. The first version runs on the machine clock with no clock crossing.

**Layout** (word addresses):

| Region       | Size            | Contents                                 |
| ------------ | --------------- | ---------------------------------------- |
| Semispace A  | 3,670,016 words | Object bodies                            |
| Semispace B  | 3,670,016 words | Object bodies                            |
| Object table | 1,048,576 words | 262,144 entries of four raw 64-bit words |

**Object table entry** (256 bits, read in one burst of sixteen columns): reference 40, class 40,
size 24, base 24, staged base 24, and flags: new, modified, cond, persistent, mark, staged,
complete, referenced. A zero reference marks an empty slot. The table is direct-mapped by the low 18
identity bits and verified against the full reference, like the pager. Metadata changes write
through to it, so it is always current. They are rare: flag transitions, allocation, and relocation.

**Allocation skips occupied slots.** Identities are never reused, so a direct-mapped table would
otherwise evict a long-lived object every 262,144 allocations. Instead the identity allocator
advances past any identity whose table slot holds a live object. The 37-bit identity space makes the
skipped identities free. Allocation then never evicts. A collision arises only when a refill from
the store finds its slot taken, and when more than 262,144 objects are live.

**Eviction to the store** therefore has two triggers, both rare:

1. A refill collides with a resident object. OBJEKT saves that object if it is new or modified, as
   the transfer engine does today, then replaces it.
2. The heap is still full after a collection. The collector then evicts objects whose referenced
   flag is clear, saving the dirty ones, until the request fits. The mechanism is an Evict control;
   the choice of victims is collector microcode policy.

## Collection

The collector's slot controls (Slot, Info, Mark, Stage, ReadBody, WriteBody) index object table
slots instead of pager slots. Marks, staged bases, and completion flags live in the table entries,
replacing the per-pager-entry registers. Begin and Commit walk the table: 262,144 entries at two
SDRAM accesses each is roughly 0.2 s at 25 MHz, which is acceptable per collection but should be
measured. Commit clears the pager.

## L1 bodies and the store cache (later)

Two further caches are worth building only if measurements call for them:

- A direct-mapped line cache in block RAM in front of the SDRAM controller: for example 4,096 lines
  of four words (about 20 RAMB36). Writes go through to SDRAM, so the collector and the table see
  one authoritative copy.
- The 8 MB HyperRAM as the disk processor's record cache, with write-behind to the card and several
  saves per commit. This cuts the 20 ms cost of the rare saves that remain.

## Block RAM budget

The XC7A200T has 365 RAMB36.

| Use                        | RAMB36 |
| -------------------------- | -----: |
| Control store, 8,192 words |     64 |
| Disk processor             |     17 |
| Pager, 16,384 entries      |     86 |
| Body line cache (later)    |     20 |
| Total                      |    187 |

The display framebuffer is not counted; it can live in SDRAM or take about 9 more for 640×480 at one
bit per pixel.

## Contract changes

These change documented behaviour and need matching changes in `docs/interface.md`,
`docs/recovery.md`, the Rust model, the emulator's cycle estimator, and their tests.

1. Page commands take one more cycle. A pager miss adds a table refill but is not an error.
2. `NotResident` means "not in memory". Fetch transfers from the store only then. Probe reports
   memory residency.
3. Victim saves follow table collisions and heap pressure, not pager collisions.
4. Allocation may skip identities. Programs must not assume consecutive identities.
5. Collector slot indices range over the object table, and its walks scale with the table.
6. In the Rust model, the entry array models the object table. The pager is invisible to the model
   and appears only in cycle estimates.

## Build order

1. **SDRAM controller** behind the existing memory channel, with a simulation model. Done: the board
   boots with bodies in SDRAM after a power-on check of the address and data lines. A full-range
   cell test is still owed.
2. **Object table and block-RAM pager together.** OBJEKT keeps per-slot state beside the pager
   today: validity, the persistent flag, the collector's mark, staged and complete bits, and the
   edge sets gathered during saves and exchanges. They are flip-flop vectors with whole-vector
   operations, which cannot grow to 16,384 slots. Enlarging the pager while it is still the record
   of residency would mean rebuilding all of that for the pager and then again for the table.
   Instead this step moves residency and every per-object flag into the SDRAM object table, with
   skip allocation and the collector on table slots, and makes the pager a cache that holds only
   entries and valid bits. Exit: all RTL suites pass against the revised contracts; a program with
   more live objects than pager slots runs with zero store requests; the pager's LUTs are recovered.
3. **Heap-pressure eviction.**
4. **Body cache and store cache,** if measurements justify them.

## Open questions

- The collector microcode iterates slots to find a tracing fixed point. Over 262,144 slots that may
  be too slow, and it may need a mark stack or a scan pointer. This needs reading and measuring
  before step 3.
- Whether the SDRAM controller should run on a faster clock than the machine. A 100 MHz controller
  would cut access time in machine cycles but adds a clock crossing.
- Table size and associativity. A two- or four-way table would make refill collisions rarer at the
  cost of wider bursts.
- NextObject's resident scan becomes a table scan. Its early exit keeps dense enumeration cheap, but
  a sparse scan visits every slot.
