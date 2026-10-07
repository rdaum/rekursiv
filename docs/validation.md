# OBJEKT and processor validation

## Machine collector and text assembler

The current collector executes ordinary LOGIK microinstructions from `microcode/ram-collector.uc`.
`machine_gc.rs` checks automatic allocation/refill recovery, each root path, repeated space
exchange, dynamic sizes, and context restoration. It also checks cycles, opaque bodies, dirty
descendants, pager collisions, saved NEW objects, evicted parents, and each copy failure boundary. A
property test adds 32 generated resident graphs and compares them with a separate work-list
traversal. Tests clock the RTL and emulate devices; expected graph and placement results never
control the executor.

The text assembler tests labels, image directives, expressions, source diagnostics, rejected input,
control encodings, and collector relocation. The `processor` and `machine-gc` examples now assemble
standalone source files. The delayed-memory `machine-gc` example performs seven allocations and five
collections, with zero host service commands or backing-store transactions.

The complete `scripts/check.sh` run passed with `PROPTEST_RNG_SEED=374655`. It includes 72 named
tests, twelve examples, formatting, Clippy, Verilator lint, and Yosys synthesis of both cores. The
full check log is `artifacts/machine-gc-check.log`. The prior LUT estimate below predates collector
state, saved processor context, root ports, and persistent-edge tracking. It must not be read as the
size of the current design.

## First processor checkpoint

The stage 6 processor checkpoint passed `scripts/check.sh` on 2026-10-06 with
`PROPTEST_RNG_SEED=374655`. The complete output is `artifacts/stage6-core-check.log`. The pre-commit
cleanup repeated the same complete check successfully; its output is
`artifacts/initial-commit-check.log`. This run includes 55 named tests, all eleven examples, Rust
formatting, Clippy, Verilator lint, and generic Yosys synthesis. Both OBJEKT and LOGIK/NUMERIK
passed Yosys `check -assert`. Netlists and reports are `artifacts/objekt.json`,
`artifacts/logik.json`, `artifacts/yosys-synth.log`, and `artifacts/yosys-logik.log`. No FPGA
placement, timing, or frequency result is claimed.

The eleven processor tests execute actual RTL and compare architectural state with the independent
Rust processor model. The arithmetic property test adds 32 generated cases, each with up to 23
operand/operation combinations. The comparison covers registers, flags, Q, product, pointers, cached
values, microaddresses, and every stack slot. Object instructions also use the existing OBJEKT
oracle and transaction checks.

| Processor behavior            | Evidence                                                                                                         |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| Arithmetic and representation | Signed overflow, carry-one subtraction, retained signed product, compact construction, and full 40-bit equality  |
| Generated arithmetic          | Arbitrary 32-bit operands across all implemented ALU operations, carry modes, and destination shifts             |
| Stack and call timing         | Nested calls, explicit pops, cached return values, and restoration of the evaluation top                         |
| Sequencing                    | Relative-current-address origin, unconditional freeze, retiring relative-zero loop, and service-break resumption |
| Condition timing              | Previous-retirement flags, inverted conditions, LASTCC, and IRQ changes during command backpressure              |
| Fetch pipeline                | Simultaneous NAM/CSMAP reads consume the old opcode and retain the new operand                                   |
| Atomic local state            | Invalid local effects cannot issue an OBJEKT command; object errors preserve processor destinations              |
| Reset                         | Reset during an object wait clears state and invalidates the loaded image                                        |
| Autonomous object operations  | An RTL loop allocates through pager collisions, saves victims, and refills its first object                      |
| Collection                    | Stack, symbol, result, and code roots survive collection at a service break; an unrelated object is reclaimed    |

The `processor` example retired 70 microinstructions and issued 26 OBJEKT commands. It allocated 21
objects, completed six object saves, and read back the original field value of 1234 after refill.
The complete example run used request delay 3, memory latency 7, response stall 5, and waveform
tracing.

The RTL refactor separates control storage, stack state, sequencing, arithmetic state, and
combinational arithmetic. Packed structures define control words, flags, and transfer metadata;
enums define controls and state machines. The complete suite passed after that refactor, including
existing resident, transfer, recovery, and command-routine tests.

This checkpoint supplies an executable processor foundation. It does not complete the entire stage 6
acceptance target: earlier command routines still use Rust sequencing. A full language ISA, operand
substitution, additional arithmetic, stack paging, and automatic processor recovery remain follow-up
work.

## Smalltalk bytecode execution

`cargo test --locked -p rekursiv-smalltalk --test execution` compares guest instruction boundaries
against an independent test interpreter. The target executes standalone microcode on actual RTL.
The checks cover stack and variable operations, branches, integer arithmetic, explicit failure,
and context-root retention during machine collection.
`cargo test --locked -p rekursiv-smalltalk --test sends` covers lookup, activation, normal returns,
argument transfer, quick methods, primitive fallback, and guest allocation under memory pressure.
See the [execution contract](smalltalk-execution.md).
`scripts/check-smalltalk-image.sh` also executes original methods from the pinned Xerox image, including a send to `Behavior>>basicNew`.

### Smalltalk stage 4 acceptance

The stage 4 requirements cover interpreter/runtime execution and device interface definitions.
The runtime suite and full pinned-image check pass for this checkpoint.
The latter includes all eight original-method send tests, saved-image startup, complete conversion verification, and primitive inventory generation.
The [primitive inventory](smalltalk-primitives.md) separates implemented primitives, tested guest fallbacks, and operations assigned to stage 5 image integration.
The [method representation contract](smalltalk-image.md#physical-bodies-and-indexing) records the typed literal access policy and its image-port consequences.

| Requirement | Evidence |
| --- | --- |
| Bytecodes, blocks, returns, and failed sends | `tests/execution.rs` compares guest instruction boundaries. `tests/sends.rs` checks block reuse, escaped homes, non-local return, `cannotReturn:`, `doesNotUnderstand:`, and `mustBeBoolean`. |
| Primitive execution and failure | `tests/sends.rs` covers integer/Float arithmetic, indexing, streams, allocation, dynamic sends, identity conversion/enumeration, and `become:`. Rejected calls preserve the receiver and arguments. Original LargeInteger and replacement fallbacks execute on RTL. |
| Method construction and access | Directed tests check immutable headers and 37-bit literals. The original `needsStack:encoder:` allocates, copies, and exchanges a method across paging and collection, preserving code and source bytes. |
| Scheduling, priorities, and semaphores | Guest tests check priority preemption, FIFO ordering, excess signals, waits, suspension, idle wakeup, repeated input, simultaneous timer/input delivery, and low-space notification. |
| Image primitive accounting and startup | Offline inventory tests check every declared primitive and its class/selector binding. The saved-image RTL test matches all 499 bytecodes in Xerox's `trace2`, then reaches the first BitBlt call at boundary 2,176. |
| Device interfaces | [The register contract](devices.md) specifies discovery, events, clocks, input, cursor/display upload, and block request/completion fields. Generic LOGIK tests cover delayed replies, errors, reset, captured branch conditions, and one-time retirement. |
| Collection, paging, and device delays | Guest fixtures force allocation/refill recovery and apply request, memory, and reply delays. The saved-image startup performs three collections and publishes both display registrations. |
| Execution stays on the machine | `execute_machine` checks that the host service count does not change after boot. The startup harness checks the same invariant. The Rust oracle observes microinstructions and object transactions; it supplies no guest results or scheduling decisions. |

The guest test files are in `crates/rekursiv-smalltalk/tests/`. Generic transport, directory, exchange, and floating-point tests are in `crates/rekursiv-sim/tests/`.
NUMERIK's independent numerical checks and synthesis evidence are recorded in [the floating-point contract](numerik-floating-point.md#validation).

Reproduce the runtime and pinned-image checks with:

```sh
cargo test --locked -p rekursiv-smalltalk
bash scripts/check-smalltalk-image.sh
```

The startup observer stops before executing primitive 96. It does not supply a BitBlt result or bypass the guest call.
Successful storage transfers, actual snapshots, drawing, interactive presentation, and startup beyond that boundary require stage 5 integration.
The missing-storage test executes the original Alto fallback and checks its error field, unchanged buffer, and untouched semaphore.
It establishes failure behavior, not a functioning disk implementation.

## FPGA resource estimate

The following estimate uses the original 256-word control store. The current wrapper has 8192 words.
These figures are not a current resource estimate. `scripts/synth.sh` now checks the 8192-word LOGIK configuration.

On 2026-10-06, Yosys 0.33 mapped the combined `objekt_tb` wrapper to Xilinx 7-series primitives. The
configuration had 16 pager entries, 32 words per stack, 256 microinstructions, 256 NAM words, and
1,024 opcode-map entries. The external object-memory capacity parameter is 512 words for this test
configuration. Object memory itself is external: neither its storage nor a board-specific SRAM or
DRAM controller is included.

| Block                                            | Logic LUT primitives | LUTs used as distributed RAM |
| ------------------------------------------------ | -------------------: | ---------------------------: |
| OBJEKT, including transfer engine                |               10,643 |                            0 |
| Control store, NAM, opcode map, and access logic |                7,078 |                        5,408 |
| Evaluation and control stacks                    |                2,095 |                            0 |
| NUMERIK, including ALU                           |                1,390 |                            0 |
| Sequencer and integration                        |                  643 |                            0 |
| Total                                            |               21,849 |                        5,408 |

The raw total is 27,257 LUT equivalents, plus 10,941 flip-flops and eight DSP48E1 blocks. There is
also one RAMB36E1 and one RAMB18E1: 54 Kib of block RAM. The distributed-memory count is 1,352
RAM64M primitives, each containing four 64-bit LUT memories. Logic LUT counts sum LUT1 through LUT6
cells; placement can pack compatible functions together. Yosys estimates 18,215 logic cells under
its packing heuristic, before adding distributed RAM. Neither count establishes device fit: the
design also uses dedicated carry and wide-multiplexer resources.

These figures retain the wrapper's test, programming, and debug interfaces. Its hundreds of input
and output buffers are not a proposed board pinout. This is synthesis evidence, without placement,
routing, or timing closure. The control store uses asynchronous reads, including a separate
root-inspection port. Changing its access schedule to use block RAM is a potential area
optimization, not an assumed saving in this estimate.

The complete report is `artifacts/rekursiv-xc7-estimate.log`. To reproduce it from the repository
root:

```sh
yosys -Q -T -p 'read_verilog -sv -Irtl objekt_tb.sv rtl/objekt.sv rtl/objekt_transfer.sv rtl/objekt_gc.sv rtl/logik.sv rtl/logik_store.sv rtl/logik_stacks.sv rtl/logik_sequencer.sv rtl/numerik.sv rtl/numerik_alu.sv; synth_xilinx -family xc7 -top objekt_tb; check -assert; stat' > artifacts/rekursiv-xc7-estimate.log 2>&1
```

## Earlier stage 5 checkpoint

Stages 1–5 passed the local checks on 2026-10-06. The tested configuration has 16 pager entries, 512
external memory words, and 40-bit data paths.

Tools: Cargo 1.98.1, Verilator 5.020, and Yosys 0.33. Dependencies come from `Cargo.lock`, including
Marlin 0.7.2 and `sv-parser` 0.13.3.

### Executed checks

`scripts/check.sh` passed:

- Rust formatting and Clippy with warnings denied.
- Seven assembler/model unit tests without an RTL dependency.
- Seven differential tests using actual Verilator execution.
- Three resident cycle-level protocol tests using actual Verilator execution.
- Ten allocation/transfer tests, including fault injection, reset, and randomized histories.
- Eleven recovery tests, including every root category, dead cycles, failure resumption, pressure
  retries, and generated graphs.
- Five command-routine tests, including pager collisions, malformed inputs, dictionary termination,
  and context preservation through collection.
- Verilator lint for the simulation wrapper and a two-entry core configuration.
- Full generic Yosys synthesis, followed by `check -assert` with zero reported problems.
- All ten examples with request delay 3, memory latency 7, response stall 5, and VCD tracing.

The complete run contains 43 named tests.

The seven differential tests include two property tests with 64 generated cases each. One generates
command sequences of up to 119 operations. The other varies body contents, object sizes, bases,
metadata flags, scan tags, collisions, and delays. The transfer suite adds 48 randomized
allocation/fetch histories of 20–79 steps. The recovery suite adds 64 generated graphs with scanning
flags, cycles, roots, nonresident nodes, and variable delays. The command-routine suite adds 32
generated context round trips with arbitrary index bits, compact selections, collection, and
variable delays. The complete check passed with `PROPTEST_RNG_SEED=374655`. Its output is recorded
in `artifacts/stage5-check.log`.

The paged-list and space-recovery examples also ran from `/tmp` through the built executable, with
command listings and waveforms. The stage 5 save-restore example passed the same outside-repository
check. This checks that RTL source and build paths do not depend on the caller's working directory.

### Acceptance evidence

| Requirement                                 | Evidence                                                                                                                                               |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Exact word formats and provisional encoding | `rekursiv-asm` unit tests check fixed hexadecimal values, field numbers, round trips, and rejected values                                              |
| Independent architectural model             | `rekursiv-model` defines transitions without RTL or Marlin dependencies                                                                                |
| Resident selection and collisions           | Directed tests cover missing entries, full-tag and scan-tag mismatches, replacement, invalidation, and metadata revalidation                           |
| Compact values                              | Tests cover nil, both Booleans, signed limits, unsigned limits, reserved codes, missing class mappings, and no memory traffic                          |
| Checked access                              | Tests cover first/last fields, zero, negatives, excessive indices, empty objects, physical capacity, and address overflow                              |
| Cache and flag coherence                    | Tests compare the selected snapshot, all pager entries, and body memory after first-field and later-field writes                                       |
| Horizontal command semantics                | Combined-field tests verify old-state reads and rollback of simultaneous register, index, and memory effects                                           |
| Stalls and exactly-once writes              | Protocol tests change inputs after acceptance, stall requests and responses, and count transfers and completions                                       |
| Memory errors                               | Failed resident writes leave body memory unchanged; failed transfers preserve published object values and pager state                                  |
| Halted initialization                       | Tests exercise body installation, complete metadata publication, class mappings, prohibited raw writes, and channel exclusion                          |
| Reset                                       | Tests reset during request, completion wait, and response phases, then initialize and use a fresh object                                               |
| Allocation                                  | Tests cover scanned nil initialization, opaque zero initialization, empty objects, full memory, identity exhaustion, and consumed failed reservations  |
| Backing-store publication                   | Unit tests reject incomplete saves, repeated words, and failed commits without changing the previous committed record                                  |
| Eviction and refill                         | Tests cover new victims, modified clean victims, clean eviction, relocation, first-word cache reconstruction, cond preservation, and scan-tag mismatch |
| Transfer failures                           | Directed cases inject faults at every transaction in dirty-victim allocation and refill, including failures after a completed save                     |
| Transfer protocol                           | Tests change command inputs after acceptance, hold responses, inspect publication ordering, and reset at seven transfer phases                         |
| Compaction                                  | Sparse bodies and failed reservations compact into a reusable prefix without changing references, register values, caches, or flags                    |
| Root interface                              | Tests cover VRs, nonresident REF, class edges, compact mappings, driver/service/language roots, and pending data/type operands                         |
| Tracing collection                          | Dirty residents override stale backing records; opaque fields do not retain objects; unreachable nonresident cycles disappear                          |
| Recovery failures                           | Read failures abort without mutation; relocation failures retain source copies and hardware exclusion until a successful resume                        |
| Exhaustion                                  | Pressure retries preserve pending Fetch targets and allocation classes; a full live set still returns OutOfSpace                                       |
| Identity safety                             | Collection preserves the identity high-water mark, including its exhausted state; dead records cannot resurrect objects                                |
| Typed access                                | Compact and stored indices use RTL class checks and bounds checks, including target/class/index pager collisions and rejected writes                   |
| Dictionary routines                         | Every valid starting field finds each key, full-table misses terminate, nil values remain distinguishable from misses, and malformed layouts fail      |
| Context save/restore                        | A scanned frame preserves all eight VRs, the selected value, and both 40-bit indices through eviction and collection                                   |
| Frame validation                            | Wrong classes, sizes, scan flags, limb encodings, and missing saved references fail before value registers change                                      |
| Runnable demonstrations                     | All ten examples agree with the model, including paged lists, dictionary writeback, and context restoration after collection                           |

The harness compares responses, VRs, indices, selected metadata, every pager slot, allocator
counters, physical memory, object records, and both transaction sequences. It also compares
unfinished backing-save state. It also checks that requests and responses remain stable under
backpressure.

### Example results

These counts use request delay 3, memory latency 7, and response stall 5. Backing-store delays use
the matching defaults. Memory writes include initialization and relocation. Service operations
include bootstrap and recovery commands.

| Example          | Commands | Service operations | Cycles | Memory reads | Memory writes |
| ---------------- | -------: | -----------------: | -----: | -----------: | ------------: |
| field-access     |       10 |                  4 |    158 |            1 |             4 |
| compact          |       12 |                  4 |    112 |            0 |             0 |
| cons             |       15 |                  9 |    276 |            3 |             6 |
| dictionary       |       51 |                 18 |    759 |            7 |            16 |
| rejected         |       12 |                  2 |    110 |            0 |             1 |
| paged-list       |      359 |                  1 |   8064 |           97 |           145 |
| space-recovery   |       93 |               4989 | 121583 |         3263 |          3547 |
| typed-access     |      122 |                 21 |   1582 |            3 |            18 |
| paged-dictionary |      105 |                  2 |   2435 |           24 |            34 |
| save-restore     |      143 |                 30 |   2618 |           26 |            47 |

The paged-list run performs 200 backing-store transactions and commits 40 object saves. It allocates
24 cells, changes the head to 999, rejects an invalid write, evicts every cell, and refetches the
list. With store request delay 2 and completion latency 11, the same program takes 8664 cycles. That
separate run produces `artifacts/stage3.vcd` and `artifacts/stage3-listing.log`.

The space-recovery example compacts 262 words to 120 and reclaims 34 unreachable objects, including
a cycle. It then continues allocation and preserves a driver-rooted object containing 123. The full
example performs four compactions, four collections, 188 backing transactions, and 20 committed
object saves. Reported cycles exclude host graph-analysis time.

The resident dictionary example now uses the shared lookup routine, including one size-validation
command per search. The typed-access run performs 22 backing transactions and commits five object
saves. The paged dictionary performs 72 backing transactions and commits 19 object saves. The
context example performs 37 backing transactions, commits two saves, and completes one collection
before restoration. Its separate listing and waveform are `artifacts/stage5-save-restore.log` and
`artifacts/save-restore.vcd`. Command-routine cycle counts exclude host branching and
payload-decoding time.

The combined waveform is `artifacts/examples.vcd`. It contains clock, reset, command,
initialization, response, memory, backing-store, debug, and internal-state signals. The synthesis
report and generic netlist are `artifacts/yosys-synth.log` and `artifacts/objekt.json`. These
generated artifacts are ignored by version control and can be regenerated with `scripts/check.sh`.

### Limits at stage 5

The stage 5 Yosys run reported 41,411 generic cells across OBJEKT and its transfer engine, including
debug ports. It maps the pager to logic and registers; this result is not an FPGA LUT count or a
timing result. There has been no board build or place-and-route run.

Initialization trusts the host to publish coherent first-field metadata and nonoverlapping live
bodies. The memory adapter must honor successful-completion write visibility and error-without-write
behavior. Those integration obligations are specified in [interface.md](interface.md).

The tests cover resident commands, allocation, writeback, refill, compaction, tracing collection,
command routines, and sampled command histories. The Rust service performs graph analysis and keeps
recovery copies in host memory. The RTL enforces maintenance exclusion and performs the physical
memory transfers and metadata changes. Service-process crash recovery and warm restart are not
implemented. Stage 5 routines use Rust for sequencing, branching, and temporary values between
commands. Their context frame is a project convention, separate from the LOGIK stacks. LOGIK and
NUMERIK were outside the stage 5 checkpoint; the processor checkpoint above now implements a subset
of both. Concurrent collection, historical instruction compatibility, and FPGA deployment remain
outside the implemented scope.

## Native microcode emulator

The [native emulator](emulator.md) executes the same assembled programs without Verilator.
Its differential regression compares CPU and object state with RTL after each mutator retirement across paging and five collections.
Collector execution uses individual privileged controls. The graph-walking Rust collector remains a separate oracle.
The shared peripheral models preserve the existing RTL handshake tests.

`cargo test --locked -p rekursiv-emulator` also covers failed device writes, recovery failure, framebuffer clipping, keyboard mapping, and microcode-driven input/display changes.
The first ignored native startup test matches all 499 Xerox trace bytecodes and the RTL checkpoint before BitBlt.
A second runs through 32 successful BitBlts, reaching 9,870 bytecode boundaries, 24 collections, and 33 display publications.
Directed BitBlt tests compare native and RTL results with a pixel-level oracle for all 16 rules.
They cover clipping, word alignment, overlapping/shared bitmaps, nil sources, halftones, and invalid operands.
Both executors also test registered cursor/display refresh and unchanged destinations on failure.
Small semispaces force collection; RTL memory and device responses include delays.
`scripts/check-smalltalk-image.sh` includes that test.

A native X11 smoke run also exercised keyboard input, mouse-button input, and cursor movement through the window.
Every screenshot pixel matched the published scanout plus the cursor at the expected coordinates.
The emulator does not validate FPGA timing, and shared architectural semantics limit independence from the Rust RTL oracle.


### BitBlt rendering measurements

The original-image native test stops after the same 32 completed copies, at bytecode boundary 9,870.
Both versions produce 33 display publications and this SHA-256 of the little-endian pixel words:

```
5c88b1fc4cd078c333f200091c7dab230cbbb9a0ba8a3b35e222a26b43fa9115
```

| Measurement | Before | After |
| --- | ---: | ---: |
| Mutator microinstructions | 22,163,025 | 6,883,624 |
| Collector microinstructions | 2,329,356 | 2,148,003 |
| OBJEKT commands | 5,033,755 | 1,320,860 |
| Device requests | 307,689 | 34,562 |
| Collections | 29 | 24 |
| Observed native execution time | 4.38 s | 1.47 s |

These release-build timings include startup and collection, with 131,072 RAM words. They exclude image conversion and compilation.
The elapsed times are individual local measurements, not FPGA timing estimates.
The new profile has 8,192 control words, compared with 4,096 before. Its collector therefore scans more control-store roots per collection.
The regression checks the framebuffer checksum and bounds instruction and device-request counts. It does not assert wall time.

Directed native and RTL tests also cover sparse uploads, pixel-group edges, changed geometry, alias geometry, and atomic publication failure.
A two-row, one-pixel-wide copy uploads two pixel words after registration, regardless of the full framebuffer size.
Disjoint bitmap copies allocate no scratch object. Aliased source or halftone storage still uses a snapshot.
The larger control-store profile passes RTL lint and the RTL regressions.
OBJEKT generic synthesis passes. The 8,192-word LOGIK synthesis run was stopped after nine minutes during process lowering; its result remains unverified.
These checks do not establish board timing or a mapped LUT count.

Native profiling also found redundant control-word encoding on every instruction fetch.
Execution now checks the same constraints without constructing the wire representation.
Three paired release runs of 20 million steps took 2.405–2.484 seconds before and 2.051–2.138 seconds after this change.
These runs used the original image, 1,048,576 RAM words, and headless presentation.
They produced identical framebuffers, final micro-PCs, instruction counts, collection counts, and device-request counts.
The window loop also removes the sleep after its former 8 ms execution budget.
It now executes for approximately 16.7 ms before presentation. Interactive performance for this change remains unmeasured.

### Proactive collection and longer execution

Native and RTL regressions exercise the standalone `gc=Collect` request through the existing collector microcode.
They cover empty heaps, repeated requests, saved CPU state, and live objects that leave no room for a stale allocation reservation.
Requests without a collector, nested requests, and malformed control words fault without issuing mutator commands.
Smalltalk tests cover reclaimable pressure, genuine low-space notification, strict thresholds, rearming, and scheduler preemption.
The reclaimable-pressure test verifies that the notification remains armed after collection restores sufficient space.
Workspace tests, original-image tests, RTL lint, and Clippy pass.
Generic synthesis passes for OBJEKT and a reduced LOGIK profile with 32 control words, four stack words, and 16 NAM words.
The full 8,192-word profile remains covered by RTL simulation and lint, without a completed synthesis result.

The original-image regression executes 60 million mutator instructions with 1,048,576 RAM words.
It completes 769 BitBlts and 27 proactive collections without reaching `low_space_signal`.
A separate 200-million-step run executes 177,560,709 mutator instructions and 22,438,887 collector instructions.
It completes 202 collections, including 152 proactive requests, with no low-space notification.
Its desktop framebuffer matches the earlier frame at the premature warning boundary.

Profiling that longer run attributes 31% of sampled native CPU cycles to instruction preparation.
Memory copying accounts for another 10%, with 3% attributed separately to CPU-state cloning.
Within microcode, rebuilding threshold chunks consumes 18,135,870 instructions, about 10% of mutator execution, before other low-space checks.
Context handling and method lookup are also prominent. Guest bytecode counts include point/rectangle operations and controller searches.
These observations identify optimization candidates, not measured gains from changes to those paths.

The emulator now reports retired microinstructions per second against active execution time and total elapsed time.
A local 20-million-step headless run reported approximately 9.52 million microinstructions/s, including collector execution.
Zero-step and Hold-only CLI checks report zero retired instructions/s. Window-title throughput remains unverified on a live display in this session.

### Native pending writes

The native executor now prepares pending writes instead of copying the whole processor per instruction.
Directed tests check simultaneous register, root, stack, pointer, and scalar destinations after success and after local, object, or device failure.
All 25 processor RTL tests and four original-image emulator tests pass, including the Xerox bytecode trace and low-space recovery.

A deterministic headless benchmark uses the original image, 16,777,216 memory words, and release builds on a Cortex-X925 pinned to CPU 5.
It measures 50 million startup steps, then 50 million steps after an equal warmup, excluding image loading.
Across three runs, median throughput increases from 8.46 to 10.27 million microinstructions/s at startup and from 8.68 to 10.70 million afterward.
Both intervals preserve their final PCs, retired counts, collections, object commands, device requests, and framebuffer hash.
These rates describe native execution without external input; they do not predict FPGA or interactive display performance.
