# OBJEKT and processor interfaces

This document defines the OBJEKT interface and the first LOGIK/NUMERIK processor checkpoint. The
OBJEKT sections cover stages 1–5; the [processor section](#processor-checkpoint-logik-and-numerik)
defines the stage 6 controls. Stage 5 [command routines](microprograms.md) use the existing
operations without changing their encodings. The operation numbers are provisional project choices.
They do not encode the book's complete Objective chipset control word.

Language runtimes use this machine contract without redefining its word formats or control fields.
The class metadata reference has no hardware-defined body layout or inheritance policy. Guest
bytecodes, object layouts, method lookup, and primitive numbers belong to runtime code and
microcode. The [Smalltalk port plan](../DESIGN.md#smalltalk-80-port) defines this boundary and the
first language target.

## Configuration and values

`objekt` has `PAGER_BITS` and `MEMORY_WORDS` parameters. The pager contains `2**PAGER_BITS`
direct-mapped entries. Use a positive pager width and a memory capacity from 1 through `2**24`
words. The supplied wrapper fixes these values at 4 and 512. The current lint checks cover pager
widths 1 and 4.

Each memory word has 40 bits. Each pager entry contains a valid bit and 171 metadata bits: reference
40, size 24, class 40, base 24, representation 40, and three flags. The slot comes from the low
identity bits; hits compare the complete 40-bit reference, including its scan bit.

| Value            | Bits and restrictions                                                              |
| ---------------- | ---------------------------------------------------------------------------------- |
| Raw              | Bit 39 is zero; bits 38:0 contain an unsigned payload                              |
| Reference        | Bits 39:38 are `10`; bit 37 is the scan flag; bits 36:0 contain a nonzero identity |
| Compact          | Bits 39:38 are `11`; bits 37:32 contain the code; bits 31:0 contain the payload    |
| Nil              | Code 0, payload zero: `0xc000000000`                                               |
| Boolean          | Code 1, payload zero or one                                                        |
| Signed integer   | Code 2, signed 32-bit payload                                                      |
| Unsigned integer | Code 3, unsigned 32-bit payload                                                    |

Compact codes 4–63 are reserved and return `BadValue` when probed. The four supported codes require
service-installed class references before use. A compact selection has size zero, base zero, clear
flags, and a zero-extended payload as its representation. Decoding a compact value causes no pager
or object-memory transaction.

Body words and VRs can contain any 40-bit pattern. Probing interprets tags and rejects noncanonical
references or compact values. Index operands instead interpret all 40 bits as a signed
two's-complement integer. The index range is `-2**39` through `2**39-1`.

## Provisional control fields

`Command::encode()` produces `Ports`; each member drives a separate RTL input. `Ports::decode()`
validates and reconstructs a symbolic command. `Command` implements `Display` for readable listings.
This standalone OBJEKT port encoding has no packed binary image or byte-order convention. Processor
microinstructions use the separate 256-bit encoding in the processor section.

| Input             | Width | Numeric operation values                                                                                                           |
| ----------------- | ----: | ---------------------------------------------------------------------------------------------------------------------------------- |
| `pager_i`         |     4 | None=0, ProbeBus=1, ProbeVr=2, ProbeType=3, ProbeRepresentation=4, Fetch=5, Allocate=6, Exchange=7, NextObject=8, FindObject=9     |
| `index_i`         |     4 | None=0, Load=1, Clear=2, One=3, Two=4, Increment=5, Decrement=6, Step=7, Next=8, FromReg=9                                         |
| `register_i`      |     3 | None=0, Load=1, Increment=2, Decrement=3, FromIndex=4                                                                              |
| `memory_i`        |     2 | None=0, Read=1, Write=2                                                                                                            |
| `prepare_i`       |     1 | Prepare an address from the old selection and new index                                                                            |
| `prepared_i`      |     1 | Use the previous prepared address for this memory operation                                                                        |
| `read_i`          |     4 | None=0, Vr=1, Reference=2, Size=3, Type=4, Base=5, Representation=6, Index=7, IndexReg=8, Flags=9, FreeWords=10, FreeIdentities=11 |
| `load_vr_i`       |     1 | Load `data_i` into the selected VR when set                                                                                        |
| `vr_i`            |     3 | VR selector, 0–7                                                                                                                   |
| `data_i`          |    40 | Bus value, write value, or signed index operand                                                                                    |
| `check_type_i`    |     1 | Require equality between the old selected class and `expected_type_i`                                                              |
| `expected_type_i` |    40 | Canonical class reference; ignored when `check_type_i` is clear                                                                    |
| `alloc_size_i`    |    24 | Body word count for Allocate                                                                                                       |
| `alloc_scan_i`    |     1 | Scanning flag for Allocate                                                                                                         |

All unlisted operation numbers return `BadCommand`. `Fetch` selects `data_i` and automatically
refills a missing stored object. `Allocate` uses `data_i` as the class reference and returns a newly
allocated reference. Both commands select their result without changing VRs or indices. The
allocation size and scan fields are ignored by other operations.

The following combinations return `BadCommand`:

- A pager operation with a memory operation, unless the access uses `prepared`.
- A pager operation with `Index::Next`.
- A memory operation with a read-result selector.
- Fetch, Allocate, Exchange, NextObject, or FindObject with any index, secondary-register, memory,
  read-result, VR-load, or class-guard operation, or either preparation control.
- `prepared` without a memory operation.

Except for `prepare`, every field reads the state that existed before the command. `ProbeVr` reads
the old VR even when the same command loads that VR. A memory operation uses the old index even when
the command increments it. `Register::FromIndex` saves the old index, and `Index::FromReg` restores
the old secondary index. `prepare` forwards the new index into address generation.

`ProbeBus` selects `data_i`. `ProbeType` and `ProbeRepresentation` probe the old selected object's
class or representation. `Step` adds the signed bus operand to the primary index. Both index
registers reject arithmetic overflow instead of wrapping. For a nonempty object, `Next` accepts
indices from zero through size and wraps size to one. An empty object or an index outside that range
returns `BoundsError`.

Successful probes return the selected value. A read selector overrides that result and reads the old
state. Field reads return the stored word; field writes return the written word. Fetch and Allocate
return their selected reference. Other commands return nil. Every error returns nil as its data
result. Flags encode new in bit 0, modified in bit 1, and cond in bit 2.

## Checked field access and selection

Fields are numbered from one through size. The physical address is `base + index - 1`, computed
before narrowing to 24 bits. Zero, negative, excessive, overflowing, or physically unavailable
addresses return `BoundsError` without a memory request. This check also applies to cached
first-field reads.

Selection retains a metadata snapshot. Operations that use metadata resolve the selected reference
again before use. An invalidated or replaced mapping therefore returns `NotResident` instead of
accessing stale memory. `Read::Reference` can still inspect the retained reference after
invalidation. A failed probe preserves the previous selection.

Field one is cached in the pager's representation field. A first-field read completes without an
external memory transaction. Every field write updates body memory and sets modified. A first-field
write also updates the pager cache and selected snapshot. The new and cond flags remain unchanged.

The class guard is optional exact reference equality. It does not interpret class bodies,
inheritance, or access-type policies.

`FreeWords` and `FreeIdentities` return raw unsigned capacity counts without requiring an object
selection. They generate no memory or backing-store transaction and do not change pager entries or
allocation cursors. `FreeWords` is the allocation limit minus the body cursor, clamped at zero. With
collection enabled, this excludes the inactive semispace and ranges awaiting reclamation.
`FreeIdentities` is `2^37 - next_identity`; exhaustion returns zero without wrapping. Both include
reservations retained after failed transfers. Reads report the allocator state at command
acceptance.

## Prepared-address pipeline

`prepare` captures the old selection and the new index ALU result in a separate register. The
register contains the reference, index, physical address, and deferred status. `prepared` directs a
memory operation to that register, independently of the current selection and index. An access does
not consume or clear the register.

A control word can access the previous prepared address and prepare its next address together. It
can also probe another object without changing the current memory target. A simultaneous `prepare`
still uses the old selection, before that probe. The class guard also retains its existing meaning:
it checks the old selected class. Fetch, Allocate, Exchange, and directory operations remain
standalone commands.

Preparation records selection and bounds errors without failing the command. A later prepared memory
operation reports that stored error before any memory request or architectural change. This permits
a loop to prepare past its last element while completing its final valid access. Index arithmetic
overflow still fails immediately. A memory error discards all accompanying index, selection, VR, and
preparation changes.

Prepared first-component reads use the current pager cache without a RAM request. Writes update the
prepared object's metadata, even when another object becomes selected. A write updates the selected
snapshot only if that snapshot names the written object. Later first-component reads therefore see
the new value.

Pager replacement, service installation, invalidation, and identity exchange invalidate affected
prepared addresses with `NotResident`. Refill alone does not restore an invalid address: microcode
must prepare it again. Machine GC retains a valid prepared reference as an architectural root and
relocates its address at Commit. Failed GC preserves the original address. The stopped-host
maintenance utility uses service installation, which invalidates prepared addresses.

Reset initializes the prepared status to `NoSelection`. `idx=Clear, prepare` releases a valid
prepared root. The new index is invalid for every object. These controls use project encodings for
the address pipeline described in the book. They do not assert historical binary compatibility.

## Clock and handshake contract

All handshakes occur on the rising clock edge when valid and ready are both high. `rst_i` is
synchronous and active high; interface ready/valid outputs are masked while reset is asserted.

`run_i=1` enables command acceptance through `cmd_valid_i` and `cmd_ready_o`, unless the maintenance
lock is set. `run_i=0` enables service acceptance through `svc_valid_i` and `svc_ready_o`. The core
accepts at most one command or service operation at a time. After acceptance, it captures everything
needed to finish the operation. The sender may then change its inputs. Changing `run_i` affects
later acceptance; it does not cancel an accepted operation.

Resident operations without memory commit on their acceptance edge. Resident memory operations
commit on the successful memory-completion edge. Fetch misses and allocations commit their pager
entry and selection after the complete transfer succeeds. Allocator reservations and transfer side
effects have the separate failure rules specified below. That commit is architectural retirement:
registers, metadata, and flags change together. OBJEKT state remains unchanged while a resident
memory command waits. The result becomes valid after retirement and remains stable until
`rsp_ready_i` accepts it. Response consumption does not execute or commit the operation again.
Neither input channel accepts another operation while a response is pending.

The memory request channel uses `mem_valid_o`, `mem_ready_i`, `mem_write_o`, `mem_addr_o`, and
`mem_data_o`. The request remains stable until accepted. The core then waits for `mem_rsp_valid_i`
with `mem_rsp_ready_o`. The adapter must hold completion data and error status until that handshake.
Resident commands use at most one memory transaction. Allocation and refill use sequential word
transactions, with at most one outstanding request on either external channel. Request and
completion require separate clock edges, even with zero configured delay.

A successful read supplies `mem_rsp_data_i`. A successful write must become visible exactly once on
its completion handshake. An error completion must leave memory unchanged. The core then returns
`MemoryError` and discards pending register and pager publication. Previously completed transfer
writes and reservations follow the failure rules below. These write semantics are obligations of the
external memory adapter.

RTL reset clears pager validity, compact-class validity, selection, registers, allocator state, the
maintenance lock, and pending interface state. It cannot erase external memory or revoke a write
already completed by an external adapter. A system reset must also reset or drain that adapter
before issuing new commands. `Harness::reset()` resets the core and both adapters, clears physical
memory and object records, and starts a deterministic fresh simulation. Warm restart with a
preserved backing store is not implemented.

## Halted service interface

All service operations return through the same response channel as commands. Service inputs have the
widths in `rtl/objekt.sv`. `svc_op_i` is four bits. Only the inputs listed below affect each service
operation.

| `svc_op_i` | Operation                                 | Inputs                                                                              |
| ---------: | ----------------------------------------- | ----------------------------------------------------------------------------------- |
|          0 | Install or replace a complete entry       | `svc_ref_i`, `svc_class_i`, `svc_size_i`, `svc_base_i`, `svc_repr_i`, `svc_flags_i` |
|          1 | Invalidate an exact resident reference    | `svc_ref_i`                                                                         |
|          2 | Configure a supported compact class       | `svc_code_i`, `svc_class_i`                                                         |
|          3 | Write an unpublished physical memory word | `svc_base_i`, `svc_repr_i`                                                          |
|          4 | Read a physical memory word               | `svc_base_i`                                                                        |
|          5 | Begin recovery and lock the mutator       | None                                                                                |
|          6 | Set the body cursor during recovery       | `svc_repr_i`, interpreted as an unsigned cursor                                     |
|          7 | End recovery and release the lock         | None                                                                                |
|          8 | Reserve existing identities               | `svc_repr_i`, interpreted as the next-identity floor                                |

Install requires canonical object and class references. Empty objects must have nil representation.
Installation publishes one complete entry and refreshes a matching selected snapshot. It also
advances the identity and body high-water marks to cover the installed object. Replacing a different
reference leaves the selection unchanged; later uses must revalidate it. Invalidation requires a
full reference match, including its scan bit. Updating a compact-class mapping also updates a
matching selected compact value.

ReserveIdentities raises the allocator floor without publishing an object or changing the body
cursor. It accepts values from 1 through `2^37`, including the exhausted sentinel. Smaller requests
never lower the current floor. Zero or larger values return `BadValue` without changing allocator
state. The transfer engine applies this control only through an accepted halted service request.

The initialization service is privileged. The host must install body words before metadata, supply
the actual first word as representation, and avoid overlapping live object bodies. Class references
need not themselves be resident. Boot code must account for all existing identities before
allocation, including class objects and nonresident records. A halted loader can reserve their
identity range with operation 8, then install only the initial resident objects before starting the
mutator. Installation permits metadata with unavailable physical spans; field accesses reject such
addresses when used. This supports explicit bounds testing without truncating metadata.

Raw writes are rejected if the address lies inside any currently published object body. To replace a
body's contents, invalidate its entry, initialize the words, then install coherent metadata.
`Harness::install()` performs body initialization followed by metadata publication and seeds the
matching backing record. Its installed objects are therefore clean. A direct service installation
with new and modified both clear promises that an identical backing record already exists. Direct
replacement and invalidation are privileged maintenance operations and do not trigger writeback. The
multi-operation initialization sequence is not a transaction; the mutator must remain stopped during
setup. ReadMemory returns the physical word. Other successful service operations return nil.

BeginRecovery rejects an already held lock. SetBodyCursor and EndRecovery require the lock.
SetBodyCursor rejects values above memory capacity or below the end of any valid resident body. It
never decreases the identity high-water mark. The [recovery contract](recovery.md) defines the
required copy and publication order.

## Errors and precedence

| Value | Status            | Meaning                                                                          |
| ----: | ----------------- | -------------------------------------------------------------------------------- |
|     0 | Ok                | Successful completion                                                            |
|     1 | BadCommand        | Reserved, unsupported, conflicting, or prohibited operation                      |
|     2 | BadValue          | Malformed compact value, missing compact class, or invalid type operand          |
|     3 | InvalidReference  | Raw value or malformed reference used where an object reference is required      |
|     4 | NotResident       | Empty slot, collision, tag mismatch, or stale selected mapping                   |
|     5 | NoSelection       | An operation requires a selected object                                          |
|     6 | BoundsError       | Invalid field index, empty scan, or unavailable physical address                 |
|     7 | IndexOverflow     | Signed primary or secondary index arithmetic overflow                            |
|     8 | TypeError         | Selected class does not match the requested class                                |
|     9 | MemoryError       | External memory rejected the current word transaction without changing that word |
|    10 | ServiceError      | Backing-store transfer failed                                                    |
|    11 | OutOfSpace        | The fresh body reservation exceeds physical capacity                             |
|    12 | IdentityExhausted | The next identity exceeds the 37-bit identity range                              |

Command validation runs in this order:

1. Reserved fields and conflicting combinations.
2. Canonical expected-class operand, or canonical Allocate class operand.
3. Primary index calculation, including selection and bounds for Next.
4. Secondary index calculation.
5. Pager source resolution and destination lookup.
6. Old selected metadata resolution and optional class equality.
7. Read-result selection, followed by field bounds and physical-address checks.
8. External memory completion, if needed.

The first failure determines the status. Resident-command errors preserve VRs, indices, selection,
pager entries, and memory. Transfer-command errors preserve VRs, indices, selection, pager entries,
and all published object values. Transfer reservations and unpublished body writes can remain after
an error. An attempted memory transaction with an error completion remains visible in the
simulator's transaction log.

## Simulator timing and observation

`--request-delay N` waits N cycles after a request becomes visible before accepting it.
`--memory-latency N` adds N wait cycles before presenting completion. `--response-stall N` holds
each core response for N extra cycles before accepting it. The protocol itself also needs
acceptance, completion, and response-consumption edges. These options are delay controls, not a
total-cycle formula.

`--store-delay N` and `--store-latency N` independently control backing-store acceptance and
completion delays. Their defaults match the memory request delay and memory latency. Tests vary both
channels and inject an error at a selected transaction number within a command. The harness checks
request and response stability on stalled cycles. After each completed operation, it compares all
visible state and memory effects with the independent Rust model. The debug ports expose VRs,
indices, selected metadata, one addressed pager entry, both allocator counters, and the maintenance
lock. `dbg_class_code_i` selects one of four compact-class mappings. `dbg_class_valid_o` and
`dbg_compact_class_o` expose its validity and class reference for root enumeration. They cannot
modify state. The CLI reports command, initialization, cycle, and completed memory-transfer counts
separately. Reported memory writes include initialization writes. The CLI also reports backing-store
transactions and successful object-save commits.

## Allocation and transfer engine

`rtl/objekt_transfer.sv` owns the identity counter, body cursor, initialization loop, victim save,
and refill sequence. `objekt.sv` supplies the target slot and victim metadata, arbitrates memory
access, and publishes a completed entry. Both modules are synthesizable. The Rust backing service
supplies object records through the external streaming interface. It does not edit RTL registers or
publish pager entries.

`Command::allocate(class, size, scan)` selects Allocate with the corresponding operands. The 38-bit
identity counter starts at one and represents exhaustion without wrapping. The 25-bit body cursor
starts at zero and can represent the end of the 24-bit address space. Successful halted
installations advance both counters without moving them backward. Allocation first checks identity
exhaustion, then body capacity. It reserves the identity and complete body range before any victim
transfer. An empty object consumes an identity but no body words.

`Command::fetch(reference)` selects Fetch. Resident hits and supported compact values complete
without a backing-store request. A miss first obtains and validates backing metadata, then reserves
a fresh body range. A missing identity or a scan-tag mismatch returns `InvalidReference`. Probe
operations retain their resident-only behavior and return `NotResident` on a miss. Metadata and
field operations also retain resident revalidation. A caller uses Fetch before accessing a displaced
selection.

For a new or modified victim, the engine saves its metadata and every body word before filling the
incoming body. The backing store publishes that save only after a successful CommitSave operation. A
clean victim already has an identical backing record and requires no save. The victim stays valid
until the incoming entry is ready for publication.

The engine initializes every scanned allocation word to nil and every opaque allocation word to raw
zero. Refill copies exact 40-bit words from the backing record. Each operation uses a fresh physical
range beyond the body cursor. The engine derives the representation cache from the first
successfully written word. An empty object's representation is nil.

After the complete body succeeds, the core replaces the target slot and selects the incoming object
on one clock edge. A new allocation has new set, modified clear, and cond clear. A refilled object
has new and modified clear and restores cond from the backing record. The response becomes valid
after that publication edge.

## Object-binding exchange

Exchange takes its first reference from `data_i` and its second from the selected value register.
Both operands must be canonical stored references with the same scan flag. Equal references succeed
without writes after the engine validates that the object exists.

`rtl/objekt_exchange.sv` reads each source from its resident entry or committed backing record. It
writes that source's class, size, cond flag, and body under the other identity in a private storage
batch. The engine does not allocate temporary RAM or require both objects to occupy different pager
slots. Bodies can exceed resident RAM capacity when their source is on the backing device.

CommitBatch publishes both records together. The core then invalidates resident entries for both
identities. It also clears the selected snapshot if that snapshot names either object. The response
returns the first reference. Subsequent Fetch commands load the exchanged bindings. Existing
references and aliases retain their bit patterns. Edges from the published records to resident
objects become persistent collector roots at successful completion.

On error, the engine aborts private writes. Committed records, pager entries, selection, RAM, and
allocator counters remain unchanged. No other command can run during an exchange. External adapters
implement batch storage, not an object-language operation.

## Object directory

NextObject returns the object with the smallest identity greater than `data_i`. FindObject returns
the object with exactly that identity. Both accept either a raw 37-bit identity or a canonical
stored reference. They ignore the input reference's scan flag. The response contains the canonical
reference, and `vr_i` selects a value register to receive its class. An absent identity or exhausted
enumeration succeeds with machine nil in both places.

`rtl/objekt_directory.sv` combines resident metadata with committed backing-store metadata. Resident
metadata wins when both sources contain the same identity. The engine never reads bodies, refills
RAM, or changes the selected object, pager entries, or allocation counters. It can enumerate objects
larger than RAM. Class filtering and language identity-number conversion belong in microcode.

The request is exclusive until its response completes. A store error leaves the destination value
register unchanged. Malformed returned references, classes, or identity ordering also produce
ServiceError without architectural changes. Enumeration is ordered by identity, but does not create
a snapshot across separate commands.

## Streaming backing-store interface

The store channel is independent of the halted initialization channel. It remains active while a
mutator command waits for transfer. Command and halted-service acceptance remain disabled during
that wait.

A request transfers when `store_valid_o` and `store_ready_i` are high on a rising edge. Its fields
remain stable until acceptance. A later completion transfers when `store_rsp_valid_i` and
`store_rsp_ready_o` are high. The service must hold its completion fields until that handshake.
There is one outstanding store transaction at most.

| Request field    | Width | Meaning                                                          |
| ---------------- | ----: | ---------------------------------------------------------------- |
| `store_op_o`     |     4 | Operation from the table below                                   |
| `store_ref_o`    |    40 | Canonical reference, or raw identity for directory operations    |
| `store_class_o`  |    40 | Class for BeginSave, otherwise zero                              |
| `store_size_o`   |    24 | Body size for BeginSave, otherwise zero                          |
| `store_cond_o`   |     1 | Cond flag for BeginSave, otherwise zero                          |
| `store_offset_o` |    24 | Zero-based body offset for ReadWord or WriteWord, otherwise zero |
| `store_data_o`   |    40 | Body word for WriteWord, otherwise zero                          |

| Value | Operation   | Successful effect                                                                       |
| ----: | ----------- | --------------------------------------------------------------------------------------- |
|     0 | Metadata    | Return the complete reference, class, size, and cond flag                               |
|     1 | ReadWord    | Return one committed body word                                                          |
|     2 | BeginSave   | Start an unpublished record with the supplied metadata                                  |
|     3 | WriteWord   | Append the next body word to that unpublished record                                    |
|     4 | CommitSave  | Atomically replace the committed record after all words arrive                          |
|     5 | BeginBatch  | Start an empty private batch and discard any abandoned staging                          |
|     6 | CommitBatch | Atomically publish all completed records in the batch                                   |
|     7 | AbortBatch  | Discard the private batch and any incomplete record                                     |
|     8 | NextRecord  | Return metadata for the smallest committed identity greater than the raw input identity |
|     9 | FindRecord  | Return metadata for the exact raw input identity                                        |

The completion carries `store_rsp_status_i` (4 bits). Metadata also returns `store_rsp_ref_i` and
`store_rsp_class_i` (40 bits each), `store_rsp_size_i` (24), and `store_rsp_cond_i` (1). ReadWord
returns `store_rsp_data_i` (40 bits). Unused reply fields are ignored by the RTL.

NextRecord and FindRecord use the metadata reply fields. Absence returns successful status with
machine nil reference and class; it is not a store error. Private batch records are invisible. These
operations inspect the storage directory without interpreting classes or object bodies.

The service keys records by identity and verifies the full reference, including the scan flag.
Metadata returns `InvalidReference` for a missing identity or mismatched reference. Other store
errors become `ServiceError` at the command interface. The RTL also rejects a mismatched metadata
reference or malformed class reference.

BeginSave creates private staging storage and can discard an abandoned unfinished save. WriteWord
accepts sequential offsets starting at zero and rejects duplicates or gaps. CommitSave requires
exactly the declared number of words. An error completion must not publish a replacement record. A
successful commit publishes once, at its completion handshake. These are requirements for any
external backing-store adapter. The Rust adapter verifies them independently of transfer sequencing.

Inside a batch, CommitSave adds a complete record to private batch storage without publishing it.
Metadata and ReadWord continue to read committed records. CommitBatch requires no incomplete record.
A failed CommitBatch must publish none of its records. BeginBatch, CommitBatch, and AbortBatch
ignore operand fields; RTL sends zeros. Reset discards all private staging and retains only
previously committed records.

## Transfer failure and space accounting

If metadata lookup or capacity checking fails, the engine reserves no body space. If identity or
capacity checking fails during allocation, it reserves no identity. After reservation, any later
failure consumes the reserved identity and body range. The allocator never reuses those reservations
in this stage.

A failed transfer leaves the old pager entry and selected snapshot unchanged. Successful earlier
writes can remain in the unpublished incoming body range. A successfully committed victim save also
remains in the backing store. An unfinished save remains private and a subsequent BeginSave replaces
it. Thus a failed operation can change allocator counters and service storage without changing any
published object value.

The bump allocator does not reuse eviction holes or failed reservations. Repeated allocation and
refill can return OutOfSpace even when unused physical ranges exist. The stage 4 recovery service
compacts resident bodies and lowers the cursor to reclaim those ranges. Its collector additionally
removes unreachable resident objects and backing records. Durable disk images, crash recovery, and
warm restart remain separate work.

## Processor checkpoint: LOGIK and NUMERIK

The first processor checkpoint adds autonomous execution to the existing OBJEKT command interface.
It uses project encodings and timing rules. It does not implement a complete historical instruction
set or a Smalltalk runtime.

| Module                 | Responsibility                                                                |
| ---------------------- | ----------------------------------------------------------------------------- |
| `logik.sv`             | Integration, instruction validation, command handshake, shared retirement     |
| `logik_store.sv`       | Writable control store, NAM, CSMAP, abstract program counter, fetch pipeline  |
| `logik_sequencer.sv`   | Microaddresses, branch targets, saved return address, mark, condition history |
| `logik_stacks.sv`      | Resident stacks, pointers, argument pointer, cached stack values              |
| `logik_io.sv`          | Captured device request, completion handshake, device-result register         |
| `numerik.sv`           | Sixteen registers, Q, product, flags, operand selection                       |
| `numerik_alu.sv`       | Combinational integer operations, destination shifts, flag calculation        |
| `numerik_fp32.sv`      | Binary32 arithmetic backend, response holding, exception results              |
| `rekursiv_control.svh` | Shared control enums, arithmetic flags, packed microinstruction type          |
| `objekt_entry.svh`     | Packed pager metadata shared with the transfer engine                         |

The headers define module-local types to support the Yosys 0.33 frontend. The modules use `logic`,
`always_comb`, `always_ff`, packed structures, and enumerated states and controls. Source comments
distinguish book evidence from project choices and explain timing at each state owner.

### Configuration and loading

The simulation wrapper supplies 8192 microinstructions, 256 NAM words, 1024 CSMAP entries, and 32
words per stack. LOGIK parameters permit other capacities; control-store capacity ranges from 2
through 65535 words. NAM capacity ranges from 2 through 65536 words. Stack capacity ranges from 2
through 65536 words for the supplied debug interface. Addresses remain 16 bits for microcode and 24
bits for abstract code and stacks. Capacity checks occur before address narrowing; invalid addresses
do not wrap into valid memory. These resident arrays are not yet optimized for an FPGA's block RAM.

The programming port accepts writes only while halted, with `boot_valid_i && boot_ready_o`.
`boot_space_i` selects control store (0), NAM (1), CSMAP (2), or the 32-word explicit root table
(3). Root words use two lanes, like NAM; they contribute roots only after both lanes are written.
`boot_addr_i` selects the entry; `boot_lane_i` selects a 32-bit lane. Lane zero contains the least
significant bits. Microinstructions require eight lanes; NAM requires two, with only eight
significant bits in lane one. CSMAP uses lane zero's low 16 bits. Every required lane must be
written before an entry becomes valid.

Writes patch an existing image; omitted entries retain their previous validity and contents. Reset
invalidates all program memories and the explicit root table, and clears processor state. `start_i`
loads `entry_i` while halted and starts execution without clearing registers or stacks. Programming
and starting on the same edge are not permitted.

The Rust `Instruction::encode()` method produces eight 32-bit lanes. `Image` holds symbolic
microinstructions, NAM words, and map entries. The loader uses those lanes directly. The
[text microassembler](assembler.md) supports standalone source files, labels, expressions, image
directives, and diagnostics.

### Project control-word encoding

The packed word is 256 bits. The exact symbolic values are defined in `rekursiv_control.svh` and
`rekursiv-asm::processor`. The original OBJEKT command-field values remain unchanged.

| Bits                      | Field                                                                |
| ------------------------- | -------------------------------------------------------------------- |
| 39:0                      | Immediate D-bus value                                                |
| 43:40                     | D-bus source                                                         |
| 47:44                     | Sequence operation                                                   |
| 51:48                     | Condition selector                                                   |
| 52                        | Invert condition                                                     |
| 53                        | Halt after retirement                                                |
| 54                        | Load mark with current microaddress                                  |
| 55                        | Load the 40-bit symbol comparator register                           |
| 71:56                     | Branch target or signed ALU immediate                                |
| 75:72, 79:76              | Register A and B selectors                                           |
| 83:80                     | ALU operation                                                        |
| 86:84, 89:87              | R and S sources                                                      |
| 91:90                     | Carry input selection                                                |
| 93:92                     | Destination shift                                                    |
| 95, 96, 97                | Register-B write, Q load, flag write                                 |
| 100:99, 102:101, 105:103  | ESP operation, SP operation, evaluation-stack data operation         |
| 107:106, 111:108          | CSP operation, control-stack data operation                          |
| 112                       | Load argument pointer                                                |
| 115:114                   | Abstract program-counter operation                                   |
| 116, 117                  | Read NAM, read CSMAP                                                 |
| 118                       | Issue an OBJEKT command                                              |
| 122:119, 126:123, 129:127 | OBJEKT pager, index, secondary-register operations                   |
| 131:130, 135:132          | OBJEKT memory operation and read selector                            |
| 136, 139:137              | OBJEKT value-register load and selector                              |
| 140, 180:141              | Class guard enable and expected class                                |
| 204:181, 205              | Allocation size and scan flag                                        |
| 211:206                   | Project compact code                                                 |
| 215:212                   | Recovery operation, defined below                                    |
| 216                       | Allocation size comes from NUMERIK register A instead of the literal |
| 220:217                   | Floating-point operation (with ALU Float)                            |
| 223:221                   | Floating-point rounding mode                                         |
| 225:224                   | Device operation: None=0, Read=1, Write=2                            |
| 226                       | Write full D-bus word to explicit root indexed by register A         |
| 227                       | Prepare address from old selection and new index                     |
| 228                       | Memory operation uses the previous prepared address                  |
| 229                       | Launch prepared memory command without waiting for its reply         |
| 94, 98, 113, 255:230      | Reserved; must be zero                                               |

Invalid selectors and reserved bits halt execution before an OBJEKT command can issue. The literal
field supplies D unless another source is selected. Other sources include cached stack values, the
previous OBJEKT result, register A, Q, pointers, NAM operand, saved return address, and the
full-width symbol register (source 11), and SymbolHigh (source 12), which returns symbol bits 39:32.
Device (source 13) returns the last successful 32-bit device reply. Root (source 14) returns the
explicit root indexed by register A. Full-width sources preserve all 40 bits; narrower sources are
zero-extended.

### Retirement, conditions, and errors

The [device channel](devices.md#logik-transport) is independent of OBJEKT and backing storage.
`io=Read` and `io=Write` capture a 32-bit aligned address from register A and write data from D.
They cannot combine object, Float, or recovery operations. A device instruction retires only after a
successful reply. Other local destinations use the original operands; the reply becomes available
through Device on the next instruction. Device transactions also capture their branch condition
before waiting. Errors preserve the previous device result and local destinations.

`ldroot` replaces one of the 32 explicit roots with the full D-bus word at retirement. The root
index comes from register A and must be below 32. `d=Root` reads through the same index. The boot
loader initializes these slots; microcode can subsequently register, replace, or clear tagged
references. Reads and writes are prohibited during collection, which scans the frozen root slots
through a separate port. A failed accompanying object or device command does not publish the root
write.

During mutator execution, every local architectural module receives the same retirement pulse.
During collection, stack and fetch retirement are disabled; arithmetic and sequencing execute the
collector. A local instruction retires after validation. An object instruction first validates local
effects and captures its selected condition. It then holds one command until OBJEKT accepts it and
waits for one response. For a blocking command, only a successful response retires its local
effects. Command acceptance, response waiting, and response backpressure cannot repeat local writes
or command issue.

`launch` permits a prepared memory instruction to retire its local effects when OBJEKT accepts the
command. Independent local instructions can then execute while RAM completes the access. Only one
object command can be outstanding. Its response remains private until the next barrier:

- Any OBJEKT command, `d=Object`, or `cc=ObjectOk`.
- A device instruction or a recovery instruction.
- Halt, service break, or a local validation fault.

The barrier waits for completion before it evaluates its operands or commits any effects. A
successful completion updates the Object result. An error halts at the barrier with fault 4 and the
OBJEKT status. The launch and intervening local instructions remain retired. OBJEKT itself still
commits its memory command only on success. Microcode that requires all local effects to wait for
success uses a blocking command.

`launch` requires `prepared` and a memory operation. It cannot share Halt, Service, or recovery
controls. An ordinary local branch does not force a barrier, unless it reads an object result or
condition. The RTL can overlap address preparation and independent processor work with one resident
memory request. It does not queue multiple RAM requests or overlap pager misses.

The selected condition stays fixed throughout command issue and response waiting. IRQ is a
synchronous input and cannot redirect an already prepared object instruction. Conditions include
registered arithmetic flags, full-width symbol equality, control-stack zero, previous condition,
object success, and IRQ. Inversion applies to the selected condition. `LASTCC` records that
selected, inverted condition at retirement. An instruction that writes flags still branches using
the previous flags.

OBJEKT response data becomes a source for the following instruction. Other simultaneous destinations
use the old D bus. With recovery enabled, Allocate or Fetch `OUT_OF_SPACE` enters the loaded
collector before retrying the faulting instruction. Other OBJEKT errors halt without committing
local processor effects. An unsuccessful retry halts, so one command cannot trigger an endless
collection loop. OBJEKT reservations and completed transfer side effects retain their existing
failure contract.

| Fault | Meaning                                                                     |
| ----: | --------------------------------------------------------------------------- |
|     0 | No fault                                                                    |
|     1 | Invalid control encoding or compact construction                            |
|     2 | Uninitialized control word or invalid microaddress                          |
|     3 | Stack address, argument pointer, or control-word counter overflow           |
|     4 | OBJEKT returned an error                                                    |
|     5 | Invalid abstract counter, uninitialized NAM word, or missing opcode mapping |
|     6 | Device transaction returned an error                                        |

Validation checks instruction availability, encoding, next microaddress, stack effects, fetch
effects, and compact construction in that order. An invalid programming address reports fault 1
while halted. Start clears the reported fault and preserves architectural data for an explicit
retry.

`HOLD` freezes the whole instruction independently of its condition. A relative jump by zero instead
retires and returns to the same instruction. Relative targets use the current microaddress. Taken
ordinary jumps save the sequential successor in UPCOR; they do not push it. Stack returns read
cached CSTKR and require a separate pointer operation to pop. Direct saved-return preserves UPCOR on
both paths.

A taken service break retires, preserves UPCOR, and stops at the successor. The low four branch bits
identify the service. `resume_i` continues execution from that successor. Halt retains the retiring
instruction's address for inspection.

### Stacks, arithmetic, and abstract dispatch

Both stack pointers start at slot zero, which is usable. Software defines empty-stack and frame
conventions. Evaluation words are 40 bits; control words are 24 bits and cannot contain object
references. SP identifies the logical evaluation top; ESP selects an evaluation access address. The
argument pointer supports `AP + branch` addressing. The stacks are resident; segment translation and
stack paging are not implemented.

Stack accesses use the address before retirement. Pointer changes alone do not refresh ESTKR or
CSTKR. Every stack write updates its cache, including argument stores away from the logical top. ESP
selection from SP forwards the newly calculated SP for the next access. Control-stack
increment/decrement operations change the stored counter, not its pointer. All address and counter
checks occur before writes.

Evaluation-stack operation Wide (5) constructs `{D[7:0], shifted_ALU_result[31:0]}` and updates
ESTKR. It packs raw bits without choosing a tag or validating a language representation. Together
with SymbolHigh, it permits microcode to manipulate full 40-bit words through the 32-bit arithmetic
datapath.

NUMERIK implements pass, addition, both subtraction orders, AND, OR, XOR, complement, rotation, and
signed/unsigned multiplication. The [floating-point extension](numerik-floating-point.md) adds
binary32 arithmetic, conversions, comparison, and square root. Its result completion uses a
handshake; LOGIK waits before retiring architectural effects. Carry input is zero, one, or the
previous zero flag. Subtraction computes `A - B - 1 + carry`; ordinary subtraction therefore selects
carry one. Its carry-out means no borrow. Multiplication retains a 64-bit product, with separate
high-word and low-word reads. Destination shifts support left, logical right, and arithmetic right
by one bit. Q and register B independently load the shifted output.

Zero, sign, carry, overflow, and corrected sign describe the unshifted result. Corrected sign is
sign XOR overflow. Full object equality uses the separate 40-bit symbol register. Compact
construction uses the existing project representation and validates nil and Boolean payloads. BCD,
division, rounded multiplication, mixed-signedness multiplication, and priority normalization remain
unsupported.

NAM stores a 10-bit opcode and 30-bit operand. Reading NAM captures both from the old APC. Reading
CSMAP captures the entry for the old opcode, even with a simultaneous NAM read. Dispatch uses the
previously captured map result. Operand substitution into control fields is not implemented in this
checkpoint. The control store supplies the microcode; a complete language instruction set remains
separate work.

### Host access and test utilities

The wrapper selects the host or processor as OBJEKT's command producer. Ownership changes require a
halted processor or service break and a completed OBJEKT command boundary. The harness rejects host
command execution while the processor runs. The older host collection utility also requires drained
transactions at a service break.

Both collectors retain active evaluation slots through SP, ESTKR, the symbol register, and the
previous OBJEKT result. It also scans literals and enabled expected-class operands in valid
control-store words. These are additional roots alongside the existing OBJEKT and software roots.
Inactive evaluation slots above SP are excluded; cached values remain roots even when their source
slot is inactive. Narrow arithmetic registers and control-stack entries cannot contain tagged
references. Only the host test utility requires explicit service-break resumption. The machine
collector enters and returns automatically through the recovery controls below.

## Machine recovery controls

LOGIK exposes `gc_enable_i` and `gc_entry_i` for halted image configuration. `gc_active_o`
identifies exclusive machine collection, and `last_status_o` reports detailed completion status. The
wrapper prefixes these with `cpu_`. Keep the configuration fixed while the heap is in use. The
[collection contract](recovery.md) defines context preservation, root bounds, semispaces, failure
behavior, and return.

LOGIK holds `gc_explicit_o` during proactive collection, connected to OBJEKT's `gc_explicit_i`. This
suppresses stale transfer size and class inputs.

Recovery controls 1–9 use an internal request/completion port to OBJEKT. Its valid/ready rules match
the ordinary command channel. The collector datapath never accepts host service requests and never
issues backing-store transactions.

| Value | Control   | D-bus operand and result                                                              |
| ----: | --------- | ------------------------------------------------------------------------------------- |
|     0 | None      | Ordinary local microinstruction                                                       |
|     1 | Begin     | Validate source-space bounds; clear marks and relocation plan                         |
|     2 | Root      | Index from the root table; return the 40-bit value                                    |
|     3 | Slot      | Pager slot index; select it and return flags                                          |
|     4 | Info      | Selected slot field: 0 reference, 1 class, 2 size                                     |
|     5 | Mark      | Full reference; return 1 only when a resident object's mark changes from clear to set |
|     6 | ReadBody  | Zero-based source offset; return and buffer a 40-bit word                             |
|     7 | Stage     | Reserve destination range for the selected marked object                              |
|     8 | WriteBody | Zero-based destination offset; write the matching buffered word                       |
|     9 | Commit    | Publish completed relocation and exchange allocation spaces                           |
|    10 | Return    | Restore processor context and retry; requires successful Commit                       |
|    11 | Collect   | Standalone mutator request for collection, with no allocation reservation             |

Slot flags are valid (bit 0), marked (1), scanned (2), retention root (3), and NEW (4). Retention
roots include dirty persistent objects and NEW objects protected by committed saves or
backing-record edges, and the valid prepared reference. ReadBody and WriteBody use the external RAM
request/completion channel. Stage and Commit return `OUT_OF_SPACE` when their capacity conditions
fail. Info on an invalid slot returns `NOT_RESIDENT`; invalid indices return `BOUNDS`. Mark ignores
nonreferences, nonresident objects, and collisions without faulting or fetching. Controls 1–10
require an active collector invocation. `gc=Collect` requires mutator mode and an enabled collector,
and rejects all other control fields. Collector instructions cannot combine an OBJEKT mutator
command or alter frozen stacks and fetch state.
