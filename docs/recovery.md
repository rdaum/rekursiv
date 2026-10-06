# Machine collection and allocation recovery

The RAM collector executes as LOGIK microcode, using NUMERIK and privileged OBJEKT controls.
Allocation or refill exhaustion enters the collector automatically, then retries the interrupted
command. Rust loads the control store, emulates external devices, and checks test results. It does
not supply roots, mark objects, or choose relocation addresses during machine execution.

The implementation follows the book's RAM collector on pp. 135–139. It uses the project's 40-bit
values and control encodings. It is a stopped-mutator collector; persistent-store reclamation and
concurrent collection remain separate work.

## Loading and running

```rust
let image = Image::program(&instructions).with_ram_collector(128, 16)?;
h.load_processor(&image)?;
h.start_processor(0)?;
```

`with_ram_collector` assembles 68 ordinary microinstructions at the supplied address and rejects
overlap with existing code. The pager count must match the hardware configuration. Root bounds come
from the image's stack and control-store capacities. The wrapper has 16 pager entries, 32 stack
words, and 512 control words. The collector uses NUMERIK registers R0–R4 for loop state; hardware
preserves the mutator's complete arithmetic state.

The loader sets `gc_enable_i` and `gc_entry_i` while halted. Keep these settings fixed throughout
execution and across subsequent collections. Enable collection before allocation begins, with
bootstrap bodies in the lower semispace. A later enable cannot convert an existing full-memory heap
automatically. Images without a collector retain the standalone OBJEKT testing contract and halt on
exhaustion.

```sh
cargo run --locked -p rekursiv-sim -- --example machine-gc \
  --request-delay 3 --memory-latency 7 --response-stall 5
cargo test --locked -p rekursiv-sim --test machine_gc
```

The example allocates seven 120-word objects and completes five collections and retries. It issues
no host maintenance commands.

## Entry, ownership, and return

An `OUT_OF_SPACE` response from Allocate or Fetch can enter collection. That response arrives before
body reservation and identity consumption for the failed attempt. Other errors remain terminal
processor faults. A request can trigger one collection before its retry; a second failure halts
instead of looping indefinitely.

LOGIK saves PC, UPCOR, mark, condition history, all sixteen arithmetic registers, Q, product, flags,
symbol, and the previous object result/status. It retains the failed request's operands and captured
branch condition. Stacks, pointers, APC, and the NAM/CSMAP pipeline remain frozen throughout
collection. Collector instructions cannot issue mutator commands, change those frozen structures,
halt, or enter a service break.

OBJEKT rejects mutator and host service requests while LOGIK owns collection. The failed transfer
has completed and both device channels are drained before collector memory access begins. The
collector uses a separate internal command port but shares the normal physical RAM channel. Every
request remains stable until acceptance and has one completion.

Recovery Return restores the saved processor context and retries the original instruction. The
original local effects retire only when that retry succeeds. On a collector error, LOGIK restores
the context and halts at the interrupted microaddress. `last_status_o` exposes the detailed object
or collector response; fault 4 denotes an object-operation error. An explicit processor restart can
try again after a device error.

## Roots and retention

The ROOT control reads machine state directly. Its index layout is:

| Indices                   | Source                                                                        |
| ------------------------- | ----------------------------------------------------------------------------- |
| 0–7                       | OBJEKT value registers                                                        |
| 8–9                       | Selected reference and class, or zero when no selection exists                |
| 10–13                     | Valid compact-class mappings                                                  |
| 14–15                     | Interrupted D bus and enabled expected-class operand                          |
| 16–18                     | Frozen ESTKR, saved symbol, saved object result                               |
| 19                        | Class returned by the interrupted refill metadata lookup                      |
| 20 onward                 | Evaluation slots, with only `0..SP` contributing values                       |
| After stack capacity      | Two words per control-store slot: literal and enabled expected-class constant |
| After control-store roots | 32 explicit root words, programmed through boot space 3                       |

Unwritten code and explicit-root slots read as zero. The loader writes all 32 explicit roots from
`Image::roots`. A language runtime can anchor a scanned root-table object there or in a value
register, then update its fields with normal machine instructions. Rust-local references are not
machine roots. Indices, control-stack entries, and NUMERIK registers are untagged numeric state. The
representation cache contributes no independent edge, so opaque data cannot acquire pointer
semantics through that cache.

MARK accepts only complete, matching resident references. Raw words, compacts, nonresident
references, and pager collisions produce no mark and no backing-store request. Each marked object
contributes its class reference. Only scanned bodies contribute tagged field references.

Modified persistent objects are additional roots, including their resident descendants. A NEW entry
that acquired a backing record before a later transfer failure is also an additional root. OBJEKT
records successful victim saves in a separate per-slot persistence bit, even if the incoming
transfer later fails. A successful save also protects resident class and scanned-body references
named by the new backing record. Their NEW targets remain roots after the saved parent is evicted.
Protection persists until slot replacement, so obsolete disk edges can conservatively retain extra
NEW objects. This prevents collection from losing unsaved descendants accessible through a
persistent object without traversing disk records during collection.

Microcode repeats pager passes until no new marks appear. This avoids a recursive work stack and
handles cycles with storage bounded by pager capacity. It can require many passes for an
unfavourably ordered chain; no constant-time tracing claim is made. Unreachable new cycles and
unrooted clean RAM copies are discarded. All backing records remain unchanged, including records for
discarded RAM copies.

## Semispaces and publication

Collection requires an even `MEMORY_WORDS` capacity of at least two words. The lower half is
initially active; allocation and refill share its bump cursor and limit. The other half is reserved
for copying. Address zero remains usable, and zero-size objects consume no body words. Every
published source body must lie within the active half before collection begins.

Marks, staged bases, and copy-completion bits reside in OBJEKT hardware. They are separate from
persisted `cond`, NEW, and MOD flags. STAGE reserves a destination range for one marked object.
Microcode issues a source read and destination write for every body word. The datapath checks
offsets and permits writes only after a successful matching read, in ascending offset order.
Zero-size objects complete staging without RAM traffic.

COMMIT requires completed copies for all marked slots and room for the interrupted request. On one
edge it publishes retained bases, removes unretained mappings, refreshes a matching selected
snapshot, and changes the allocator cursor and semispace. It preserves reference identity, class,
size, representation, NEW, MOD, and `cond`. The identity counter never decreases.

No source body or published entry changes before COMMIT. A tracing error, copy error, or
insufficient destination capacity leaves the original heap authoritative. Successful writes into the
unused half can remain after failure; they have no published mappings. A later collection overwrites
them. No host body snapshot, relocation plan, or rollback command is needed.

This first policy reports terminal exhaustion when retained bodies plus the request cannot fit in
one half. It does not implement the book's optional MEMFLUSH fallback or disk garbage collection.
Two semispaces therefore trade half the physical RAM capacity for failure-safe copying.

## Machine collector inventory

The original work inventory is now implemented through these paths:

| Controls        | Implementation                                                                                                     |
| --------------- | ------------------------------------------------------------------------------------------------------------------ |
| GC01–GC03, GC18 | LOGIK exhaustion entry, saved continuation, register-sourced allocation size, semispace limit, and retry           |
| GC04–GC09       | OBJEKT resident-only full-reference marking, transient tags, slot enumeration, and retention flags                 |
| GC10–GC13       | Machine root ports, frozen mutator state, class/scanned-body tracing, and bounded pager passes                     |
| GC14–GC17       | Checked physical reads/writes, staged bases, completed-copy tracking, atomic publication, and RAM-only discard     |
| GC20            | Machine collection ownership excludes host and mutator commands                                                    |
| GC21            | Shared control encoding, assembler with label fixups, independent RAM-retention model, RTL tests, and build checks |
| GC19            | Optional standalone flush/save fallback remains unimplemented; exhaustion is explicit                              |

The [standalone microcode](../microcode/ram-collector.uc) controls tracing and copy loops. The
[assembler](assembler.md) resolves labels and encodes its control words. [LOGIK](../rtl/logik.sv)
owns entry, context preservation, and return. [OBJEKT's maintenance datapath](../rtl/objekt_gc.sv)
supplies the privileged operations. The [test model](../crates/rekursiv-model/src/ram_gc.rs) uses a
separate work-list traversal to predict retention and placement. It is not part of machine
execution.

Source evidence is chapter 8, pp. 135–139, and appendix 1, pp. 168–169, 177, 182, and 184. Chapter
7, p. 122 supplies the allocation-overflow call pattern. The project adapts that algorithm: repeated
passes replace recursion, dirty objects receive descendant closure, and publication waits for all
copies. Historical binary encodings and cycle-for-cycle control combinations are not required.
[DESIGN.md](../DESIGN.md#sources-and-design-authority) records the local source archives.

## Test-only host recovery utilities

The earlier `Harness::recover` and `execute_recovering` APIs remain for standalone OBJEKT regression
tests and Rust-sequenced examples. Their `Compact` mode packs resident bodies; `Collect` traces the
complete logical graph and can delete backing records. They use halted service commands under a
maintenance lock and retain host snapshots to resume failed overlapping relocations. Their semantics
differ from machine RAM collection. Neither API is called by the machine collector or the
`machine-gc` example. These utilities do not represent an auxiliary processor in the implemented
machine.
