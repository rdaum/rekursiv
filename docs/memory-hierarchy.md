# Memory hierarchy for the MEGA65

Status: partly built, and revised. Object bodies are in SDRAM. The pager is now a block-RAM table of
up to 32,768 entries and still defines residency, which is enough to hold a whole Smalltalk-80
image: its imported objects have identities below 32,768. The SDRAM object table described below is
not built. It remains the route to more resident objects than block RAM allows, such as a Squeak
image, and its cost is higher than this document first assumed: every table access becomes a
multi-cycle request, it needs a write-back cache, and the emulator's cycle estimates would have to
model that cache exactly.

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
skipped identities free. OBJEKT already allocates this way on the pager.

**Eviction to the store** has two triggers, both rare with a large table:

1. A refill collides with a resident object. OBJEKT saves that object if it is new or modified, as
   the transfer engine does today, then replaces it.
2. Every table slot is occupied. Allocation then takes the next identity and evicts its slot's
   occupant, saving it if it is new or modified. This is today's behaviour, reached only when the
   table is full. A later step can choose the victim better, using the referenced flag, and can
   collect before evicting.

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
2. **Block-RAM pager.** Done: the table and the collector's per-slot state are two-port RAMs with
   registered reads, the selected and prepared entries are authoritative registers, the collector
   traces with a work list, and allocation prefers empty slots. OBJEKT is under 10,000 LUTs at
   16,384 or 32,768 entries.
3. **SDRAM object table,** if a runtime needs more than 32,768 resident objects.
4. **Collection before eviction.** Done: with a collector, allocation into a full table collects
   first and evicts only if the table is still full. The emulator's Smalltalk-80 boot at the board's
   sizes went from 175,450 new objects saved to 177, with 7 collections costing 3.7% of the run. A
   better victim than the next identity's slot remains open.
5. **Body cache and store cache,** if measurements justify them.

## Revised contracts for the object table

This section is the proposal for build step 3. It is written against the [interface](interface.md)
and [recovery](recovery.md) contracts and would replace the matching parts of both. Nothing in it is
implemented.

### Configuration

OBJEKT gains `TABLE_BITS` beside `PAGER_BITS`, with `PAGER_BITS <= TABLE_BITS`. The board uses 18
and 14. Tests use small values, such as 4 and 2, so that table collisions and pager misses both
occur in short programs. The pager must also work with one or two entries.

### Object table entry

One entry per table slot, in four raw 64-bit memory words. The table is the only record of residency
and holds all per-object state that is flip-flops beside the pager today.

| Field               |             Bits | Replaces                                                     |
| ------------------- | ---------------: | ------------------------------------------------------------ |
| reference           |               40 | Pager entry; zero means the slot is empty                    |
| class               |               40 | Pager entry                                                  |
| size                |               24 | Pager entry                                                  |
| base                |               24 | Pager entry                                                  |
| new, modified, cond |                3 | Pager entry flags                                            |
| persistent          |                1 | `persistent` vector                                          |
| mark                |                1 | Collector `marks` vector                                     |
| complete            |                1 | Collector `staged` and `complete` vectors                    |
| staged base         |               24 | Collector `bases` RAM                                        |
| gray link           |     `TABLE_BITS` | New: next slot in the collector's work list                  |
| edge, edge link     | 1 + `TABLE_BITS` | `saved_edges` and `exchange_edges` vectors                   |
| referenced          |                1 | New: set when the pager loads the entry; for eviction policy |

The cached representation is not stored. A pager refill reads the body's first word.

### Pager

A pager entry is a copy of a table entry's reference, class, size, base, representation, and flags,
plus an epoch byte. An entry is valid when its epoch equals the pager's current epoch, so a
whole-pager invalidation is one increment. A sweep rewrites the entries when the epoch wraps.

- Any command that names an object looks in the pager first. A miss reads the table entry. If the
  table holds the reference, OBJEKT reads the first body word, writes the pager entry, sets the
  referenced flag if it was clear, and continues. No error and no store traffic result.
- A pager entry that is overwritten is simply dropped. Flag changes were already written through.
- Changes to the modified, cond, and persistent flags, and to base, write through to the table on
  the same commit as the pager entry.
- The selected snapshot and the prepared access are authoritative copies. OBJEKT updates or
  invalidates them when the object they name changes, moves, or leaves memory. Commands on the
  selected object read neither the pager nor the table.

### Commands

| Command                   | Change                                                                                                                      |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Page (all sources), Probe | One more cycle on a pager hit; a refill on a pager miss. `NotResident` only when the table has no matching entry.           |
| Fetch                     | Transfers from the store only when the object is not in the table.                                                          |
| Field read and write      | Unchanged on the selected or prepared object. The first write to a clean object also writes the modified flag to the table. |
| Allocate                  | Takes the next identity whose table slot is empty (see below). Saves a victim only when every table slot is occupied.       |
| Exchange                  | Residency of both operands comes from the table.                                                                            |
| NextObject, FindObject    | The resident scan reads table slots instead of pager slots.                                                                 |

**Allocation.** As on the pager today: the allocator takes the first identity whose table slot is
empty, probing one table entry per candidate. When all `2**TABLE_BITS` slots are occupied it takes
the next identity and evicts its slot's occupant. The halted ReserveIdentities floor still applies.

**Refill.** Fetch of an object that is not in the table reserves body space and streams the record
from the store, as today. If its table slot holds another object, that object is the victim: OBJEKT
saves it first when it is new or modified, exactly as the transfer engine does today for a pager
collision, then replaces it.

**Edges.** While a victim save or an exchange streams a record, each class and scanned body word
that names a resident NEW object must become a persistence dependency when the save commits. OBJEKT
sets the target's edge flag and links it into an edge list on first sight. A committed save walks
the list, setting persistent and clearing edge. A failed or abandoned save walks it clearing edge
only. Both walks visit only the listed entries.

### Collector

The collector already traces with a work list and copies in slot order (Next and NextMarked in
[the recovery contract](recovery.md)). For the object table:

- Slot indices range over the table. Slot, Next, and NextMarked select table slots.
- Marks, links, staged bases, and completion flags move from OBJEKT registers into table entries.
- Begin and Commit walk the table. Commit also invalidates the pager.
- NextMarked scans table slots; the occupancy bitmap below lets it skip empty ones.
- Until Commit, table entries keep their published base. A failed collection leaves marks, staged
  bases, and links in the table; the next Begin clears them. No rollback walk is needed.

### Whole-table walks

Begin, Commit, the halted raw-write overlap check, SetBodyCursor, and a sparse NextObject visit
every slot. At two memory accesses per slot, a 262,144-slot walk is roughly 5 million cycles: 0.4 s
at 12.6 MHz. Two measures keep this tolerable and should be built with the step:

- OBJEKT tracks the lowest base and highest end of all published bodies. A raw write or a cursor
  outside that range needs no walk. The halted loader installs bodies in ascending order, so its
  writes stay outside the range.
- An occupancy bitmap in block RAM (one bit per slot, 8 RAMB36 for the board) lets every walk skip
  empty slots 32 at a time. Walks then cost in proportion to the number of resident objects.

### Model, emulator, and tests

- The Rust model's entry array models the object table and gains the persistent flag. The pager does
  not appear in the model. `Model::new` takes the table size.
- The emulator's cycle estimator gains pager hit and miss costs and the table walk costs.
- New tests: pager misses with no store traffic; a program with more live objects than pager slots
  that issues no store requests; edge-list commit and abandon.

### Implementation order within the step

1. The work-list collector, on the existing pager. Done.
2. Skip allocation, on the existing pager, in the model, emulator, and RTL. Done. The model's entry
   array now behaves as the object table will.
3. RTL: the raw table port on the memory channel, the table engine, the pager cache, then the
   collector and transfer engines moved onto table entries.
4. Board: SDRAM table port, sizes 18 and 14, and measurement of walk and collection times.

## Open questions

- Whether the SDRAM controller should run on a faster clock than the machine. A 100 MHz controller
  would cut access time in machine cycles but adds a clock crossing.
- Table size and associativity. A two- or four-way table would make refill collisions rarer at the
  cost of wider bursts.
- NextObject's resident scan becomes a table scan. Its early exit keeps dense enumeration cheap, but
  a sparse scan visits every occupied slot, so enumerating every object costs the square of the
  resident count. An index ordered by identity would fix that if a runtime needs fast enumeration.
- Whether Mark should consult a mark bitmap in block RAM before reading the table, to make marking
  an already-marked object cheap.
