# Microcode programs

These sources implement the machine's RAM collector, standalone demonstrations, and Smalltalk-80
runtime. LOGIK executes the control flow, NUMERIK performs arithmetic, and OBJEKT manages tagged
values, object access, allocation, and paging. The native emulator executes the same assembled
microcode as the RTL simulator. The host loads images and supplies external memory, backing storage,
and peripherals; it does not execute Smalltalk primitives or traverse guest queues during execution.

Start with [allocation.uc](allocation.uc), then [pipeline.uc](pipeline.uc), then the
[interpreter](smalltalk/interpreter.uc) and [send engine](smalltalk/sends.uc). Each source documents
its entry state, scratch registers, results, and important control-flow transitions. The
[assembler guide](../docs/assembler.md) is the complete syntax reference. The descriptions here
refer to this project's encodings and converted image, not historical binary compatibility.

## Loading and running

| Source                               | Purpose and observable result                                                                                                         |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| [allocation.uc](allocation.uc)       | Preserve an object through 20 further allocations, eviction, and refill. Halts with R0=0 and ESTKR holding compact signed 1234.       |
| [collection.uc](collection.uc)       | Allocate seven opaque 120-word objects. With the simulator's two 256-word semispaces, completes five collections and halts with R8=0. |
| [pipeline.uc](pipeline.uc)           | Sum raw 11, 22, 33, 44 with prepared, launched reads. Halts with R0=110, R1=0, R2=16, R3=4.                                           |
| [workstation.uc](workstation.uc)     | Upload a cursor and stripe display; invert stripes on key/button down. Polls forever.                                                 |
| [ram-collector.uc](ram-collector.uc) | Privileged recovery entry installed alongside a mutator program; not an ordinary boot program.                                        |

Run the finite demonstrations through RTL, optionally adding a listing and memory stalls:

```sh
cargo run --locked -p rekursiv-sim -- --microcode microcode/allocation.uc --listing
cargo run --locked -p rekursiv-sim -- --microcode microcode/collection.uc
cargo run --locked -p rekursiv-sim -- --microcode microcode/pipeline.uc \
  --request-delay 3 --memory-latency 7 --response-stall 5
```

The simulator normally installs the collector at microaddress 128 and rejects overlap. Its
`--example processor` and `--example machine-gc` paths also assert their expected final values and
recovery behavior. Collection counts and pager collisions depend on the loaded hardware profile. The
demo class reference, identity 100, is arbitrary object metadata; these examples do not use the
Smalltalk descriptor convention.

The emulator's default program is the workstation demonstration:

```sh
cargo run --release --locked -p rekursiv-emulator
cargo run --release --locked -p rekursiv-emulator -- --headless --steps 100000
```

All 21 files under `smalltalk/` assemble together through
[`rekursiv_smalltalk::interpreter::assemble`](../crates/rekursiv-smalltalk/src/interpreter.rs). They
share one label namespace and the `NIL`, `FALSE`, `TRUE` constants. The loader supplies
`ACTIVE_CONTEXT`, loads the converted graph while halted, configures compact code 2 as SmallInteger,
and reserves the imported identity range. The runtime reserves microaddress 8064 onward for the
collector. A single primitive file cannot be run with `--microcode` independently. See the
[emulator guide](../docs/emulator.md) for image startup and debugging.

## Reading a control word

Each executable line is one 256-bit horizontal microinstruction. Commas describe simultaneous
controls, not a sequence of statements. Labels and directives consume no instruction slots. Comments
begin with `;`, `#`, or `//`. The assembler has no include or macro facility; the Rust loader
concatenates Smalltalk sources.

| Idiom                                                         | Meaning and dependency                                                                      |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `d=7, r=Bus, rb=8, ldrb`                                      | Drive the 40-bit D bus with raw 7; default ALU Pass writes its low32 to R8.                 |
| `ra=8, rb=8, alu=Sub, s=Branch, brch=1, cin=One, ldrb, flags` | R8 := R8-1, update flags. Subtraction requires carry-in 1 for ordinary subtraction.         |
| `seq=ConditionalJump, cc=!Zero, brch=loop`                    | Test saved flags. A simultaneous `flags` write would not change the condition being tested. |
| `read=Vr, vr=3` then `d=Object, page=Fetch`                   | Read VR3 into the response latch, then select/refill that object.                           |
| `d=2, idx=Load` then `mem=Read`                               | Load component index, then read it. Memory access on the index-load line would use old IDX. |
| `mem=Read, idx=Increment`                                     | Read old IDX and advance for the next access.                                               |
| `ra=4, estk=Compact, compact=2`                               | Construct a tagged signed compact from the ALU result; `d=42` alone is raw data.            |
| `d=continuation, r=Bus, rb=6, ldrb, seq=Jump, brch=helper`    | Save a return microaddress explicitly. The helper returns with `d=Register, ra=6, seq=Bus`. |
| `d=29, r=Bus, rb=7, ldrb` then `ra=7, d=Root`                 | Select explicit root slot 29. `ldroot` writes the indexed slot from D.                      |

R0–R15 and Q are 32-bit numeric state. VR0–VR7, SYMBOL, ESTKR and object fields hold complete 40-bit
values. `r` and `s` select ALU operand sources; `ra` and `rb` select registers. Defaults include
`alu=Pass`, `ra=rb=0`, register operands, and sequential execution. `rb` can select both the second
input and the register written by `ldrb`. `brch` is one shared field: its signed 16-bit ALU
immediate and branch destination cannot be different on the same line.

`alu=Rotate` rotates left by the low five bits of its second operand. Code often uses rotate plus a
mask for shifts by 8, 14, 16, or 28 bits. `shift=Right` is a one-bit logical destination shift;
`ArithmeticRight` sign-extends. Arithmetic flags describe the unshifted ALU result. A multiply
updates the product registers, then a separate `ProductLow`/`ProductHigh` instruction reads them.

`Object` is the previous completed command's response, not a durable variable. Commands that read
metadata or load a VR can replace it too. A line combining `d=Object` and a new blocking object
command consumes the old reply in its local datapath, then produces the next reply. Preserve a full
reference in a VR or SYMBOL before another command needs that response latch.

`prepare` computes an address using the updated index and old selection. `prepared` uses the
previously prepared address. `launch` permits local work while a prepared access waits; the next
object/result/device/recovery/stop barrier joins it. An unused prepared bounds error is deferred,
which is why the stream demo can prepare component 5 of a four-word object after its final read. See
the [pipeline contract](../docs/interface.md#prepared-address-pipeline) for invalidation and
recovery rules.

## Smalltalk source map

Primitive numbers are guest method metadata. They are dispatched by `sends.uc`, not interpreted by
devices. Bytecode numbers are a different namespace. All ordinary primitive results use VR5 and
`send_result` unless the module documents activation, dynamic sending, or stopping instead.

| Module                                     | Entries and responsibility                                                                                                              |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| [interpreter.uc](smalltalk/interpreter.uc) | `start`, `load_context`, `cycle`; byte fetch, variables, evaluation stack, branches, integer fast paths, returns, NAM/CSMAP tables.     |
| [sends.uc](smalltalk/sends.uc)             | `send_prepare`, `lookup_class`, `activate`, `send_result`, `return_sender`; method lookup, primitive dispatch, contexts and resumption. |
| [primitives.uc](smalltalk/primitives.uc)   | 110 identity, 111 class; shared `primitive_true`/`primitive_false` exits.                                                               |
| [integers.uc](smalltalk/integers.uc)       | 1–18 arithmetic, comparisons, division variants, bit operations, shifts and Point construction.                                         |
| [floats.uc](smalltalk/floats.uc)           | 40–54 Float conversion, binary32 arithmetic/comparison, truncation, fraction, exponent and scaling.                                     |
| [indexed.uc](smalltalk/indexed.uc)         | 60–64 indexed/String access and size, 68–69 method header/literals, 73–74 instance fields; unsigned 16-bit helpers.                     |
| [storage.uc](smalltalk/storage.uc)         | 70–71 new/new:, 72 become:, 79 newMethod:header:. Heap operations, despite the storage name.                                            |
| [identities.uc](smalltalk/identities.uc)   | 75–78 identity conversion and ordered instance enumeration; unsigned integer result construction.                                       |
| [streams.uc](smalltalk/streams.uc)         | 65–67 Array/String next, nextPut:, atEnd.                                                                                               |
| [blocks.uc](smalltalk/blocks.uc)           | 80–82 block creation/value/valueWithArguments:, `context_home`, `cannot_return`.                                                        |
| [messages.uc](smalltalk/messages.uc)       | `lookup_failed`; construct Message and argument Array and send doesNotUnderstand:.                                                      |
| [perform.uc](smalltalk/perform.uc)         | 83–84 dynamic sends; validate target arity before rearranging caller operands.                                                          |
| [scheduler.uc](smalltalk/scheduler.uc)     | 85–89 signal/wait/resume/suspend/flushCache; queue helpers and deferred process switches.                                               |
| [events.uc](smalltalk/events.uc)           | 93 input semaphore registration; boundary event delivery and idle loop.                                                                 |
| [input.uc](smalltalk/input.uc)             | 90–92 mouse/cursor, 94 sampling, 95 input word; raw-packet translation and rooted word ring.                                            |
| [clocks.uc](smalltalk/clocks.uc)           | 98–100 clock reads and timer registration; shared receiver-result helper.                                                               |
| [system.uc](smalltalk/system.uc)           | 112/115 allocation capacities, 113 quit, 114 resumable debugger entry.                                                                  |
| [lowspace.uc](smalltalk/lowspace.uc)       | 116 copied thresholds and one-shot notification; boundary collection attempt and unsigned 40-bit decoder.                               |
| [disk.uc](smalltalk/disk.uc)               | 135 snapshot target registration only; no storage transfer or snapshot writer.                                                          |
| [bitmaps.uc](smalltalk/bitmaps.uc)         | 101–102 cursor/display registration; full and partial device uploads.                                                                   |
| [bitblt.uc](smalltalk/bitblt.uc)           | 96 clipping, 16 Boolean rules, halftones, alias snapshots and display refresh.                                                          |

Unsupported primitive numbers execute their original guest method body. This includes storage
transfer primitive 128; snapshot target registration does not implement saving an image. The
[primitive inventory](../docs/smalltalk-primitives.md) distinguishes implemented operations from
guest fallback and records the coverage of original-image tests.

## Values and physical layouts

Stored guest objects use a scanned reference, `0xa000000000 | identity`; imported even oop `p` has
identity `p/2`. New identities start at 32768. Signed SmallInteger values use
`0xc200000000 | uint32(payload)` and the guest range -16384..16383. A raw zero, compact integer
zero, machine nil `0xc000000000`, and guest nil `0xa000000001` are four distinct values.

Every ordinary guest body begins at physical component 1 with the raw descriptor
`(guest_byte_length << 2) | kind`. Kinds are pointer=0, word=1, byte=2, compiled method=3.
Pointer/word fields contribute two guest bytes each, even though one physical component is 40 bits.
Byte fields each occupy a whole physical component. Class is OBJEKT metadata, outside the body.
Guest pointer field `i` (zero-based) is physical component `i+2`; primitive indices such as `at:`
are one-based and additionally skip fixed fields where appropriate.

| Object                  | Physical components after descriptor 1                                                            |
| ----------------------- | ------------------------------------------------------------------------------------------------- |
| MethodContext           | 2 sender, 3 IP, 4 SP, 5 method, 6 unused, 7 receiver, 8 onward temporaries/stack.                 |
| BlockContext            | 2 caller, 3 IP, 4 SP, 5 argument count, 6 initial IP, 7 home, 8 onward argument/evaluation slots. |
| Class                   | 2 superclass, 3 method dictionary, 4 instance specification.                                      |
| MethodDictionary        | 2 tally, 3 parallel method Array, 4 onward selector slots.                                        |
| Association             | 2 key, 3 value.                                                                                   |
| ProcessScheduler        | 2 priority-list Array, 3 active Process.                                                          |
| Process                 | 2 next link, 3 suspended context, 4 priority, 5 owning list.                                      |
| LinkedList or Semaphore | 2 first, 3 last; Semaphore adds 4 excessSignals.                                                  |
| Stream                  | 2 collection, 3 position, 4 readLimit, 5 writeLimit.                                              |
| Form                    | 2 bitmap, 3 width, 4 height, 5 offset.                                                            |
| Point                   | 2 x, 3 y.                                                                                         |
| Message                 | 2 selector, 3 argument Array.                                                                     |
| Float                   | 2 high16, 3 low16 of IEEE binary32; descriptor 17.                                                |
| LargePositiveInteger    | 2 onward raw digits, little-endian; descriptor `4*digit_count+2`.                                 |

A CompiledMethod has header in component 2, literals in 3 through `literal_count+2`, then raw
bytecode/trailer components. Context IP retains the original one-based guest byte offset: initial IP
is `2*literal_count+3`, and byte component is `IP-literal_count`. Headers and literals reserve guest
byte positions but are not exposed as serialized bytes. Header writes fail; `objectAt:put:` can
replace full-width literals. See the
[image representation contract](../docs/smalltalk-image.md#physical-bodies-and-indexing).

The converted header's payload uses bits 0–5 for literal count, bit 6 for large context, bits 7–11
for temporary count, and bits 12–14 for method flag. Flags 0–4 give arity; 5 returns self, 6 loads
an instance field, and 7 uses an extension in the next-to-last literal. Its payload gives primitive
number in bits 0–7 and arity in bits 8–12. The last literal provides the defining-class Association
for super sends.

## Runtime state and helper calls

Between bytecodes, VR0 is active context, VR1 method, VR2 receiver. R8 is next guest IP, R9 SP, R10
literal count, R11 guest method byte length, R12 evaluation floor, R13 opcode, R14 slot capacity.
For methods, the floor is the temporary count; for blocks it is zero. Top of stack is component
`R9+7`; pushing increments R9 before writing. R0–R7 and VR3–VR7 serve individual operations. Sends
reuse the method caches, and `load_context` rebuilds them afterwards.

A primitive normally receives R0 number, R1 arity, VR6 receiver, VR7 method, caller VR0 and R8/R9,
and R15 failure continuation. Keep original operands in the caller until all fallible operand checks
pass. On success, root the result in VR5 and jump to `send_result`, which consumes receiver and
arguments, reloads caches with internal R15=16, and pushes the result. On rejection,
`primitive_failed` jumps through R15. For special bytecodes this resumes lookup; for an already
found method it activates the method's fallback body. Hardware access faults are separate from this
guest-level rejection path.

There is no universal call/return stack convention. Helpers save their continuation in a named
numeric register; do not overwrite it or assume another helper uses the same register.

| Helper                           | Inputs                                               | Output and continuation                                                      |
| -------------------------------- | ---------------------------------------------------- | ---------------------------------------------------------------------------- |
| `fetch_byte`                     | VR1 method, R8 IP, R10/R11 bounds                    | R0 byte; increments R8; returns via R7.                                      |
| `peek`                           | VR0, R9 SP, R12 floor                                | SYMBOL top value, selected caller/top IDX; returns via R6 without popping.   |
| `push`                           | SYMBOL, caller caches                                | Writes stack and goes to `boundary`; no helper return.                       |
| `context_home`                   | VR0 context                                          | VR3 MethodContext, via R6; preserves numeric registers.                      |
| `positive_value`                 | SYMBOL integer                                       | R4 unsigned 16-bit value, via R6; R7 scratch; rejection uses R15.            |
| `unsigned40_value`               | SYMBOL integer                                       | R2 low32, R3 high8, via R6; R4/R5/R7 scratch; rejection uses R15.            |
| `positive_result`                | R4 unsigned 16-bit                                   | VR5 SmallInteger or two-byte LargePositiveInteger; tail-calls `send_result`. |
| `identity_count`                 | R2 low32, R3 high8                                   | VR5 LargePositiveInteger with 1–5 digits; tail-calls `send_result`.          |
| `float_load`                     | SYMBOL Float                                         | R4 binary32 bits, via R6; R5 scratch; rejection uses R15.                    |
| `clock_byte_target`              | VR4 target                                           | Selected byte object with room for four bytes, via R6; R7 scratch.           |
| `clock_result`                   | VR6 receiver                                         | Copies receiver to VR5 and tail-calls `send_result`.                         |
| `input_argument`                 | One-argument primitive state                         | VR3 and Object hold argument, via R6; Q/IDX/selection change.                |
| `input_buffer_load`              | R7 continuation                                      | Selected private state rooted in VR3, via R7; may allocate; R0 scratch.      |
| `queue_append` / `queue_remove`  | VR7 queue, VR5 process for append                    | Updates links; remove returns head in VR5; both return via R6.               |
| `bb_form`                        | VR3 Form, R7 continuation, BitBlt frame              | Object/VR3 bitmap, R2/R3 dimensions, R4 stride; returns via saved R7.        |
| `bitmap_upload` / `bitmap_patch` | VR4 bits, R0 bank, R2/R3 dimensions, R10/R11 strides | Publishes device pixels, returns via R15; patch also consumes BitBlt frame.  |

A typical send traverses `send_literal` → `send_prepare` → `lookup_receiver_class` → `lookup_class`
→ `probe_selector` → `method_found`. A normal method then allocates through `activate`, copies
receiver/arguments at `copy_argument`, and enters `load_context`. A return roots its value before
fetching the sender: `return_value` → `return_sender` → `load_context` → `resume_result` → `push`.
Quick methods and successful primitives skip context allocation.

## Roots and boundary processing

Allocation **and refill** can collect. A reference held only in a 32-bit NUMERIK register is neither
complete nor a root. Use VRs, SYMBOL, live expression-stack slots, explicit roots, or reachable
tagged object fields. Rooting does not pin a body or prevent pager collisions: refetch the reference
when changing objects. New guest pointer fields must be initialized to stored guest nil; the
allocator's machine nil is not a guest value.

The Smalltalk evaluation stack is in context objects. LOGIK's hardware expression stack is separate
scratch/root storage. Its address pointer ESP chooses a slot; SP determines which slots the
collector scans. Changing ESP does not load ESTKR: a separate `estk=Read` does that. Numeric
control-stack storage and NUMERIK registers are not scanned for references.

| Explicit root slots | Runtime ownership                                                       |
| ------------------- | ----------------------------------------------------------------------- |
| 0–25                | Fixed imported objects from oops 2 through 52, installed by the loader. |
| 26                  | Initial diagnostic root context, not updated at every context switch.   |
| 27, 28              | Input and one-shot timer semaphores.                                    |
| 29                  | Lazily allocated private device Array.                                  |
| 30                  | Low-space registration Array.                                           |
| 31                  | Raw scheduler idle flag, not a guest Boolean.                           |

Root29's private Array has descriptor 336 (42 fields) and 43 physical components:

| Components | Contents                                                                             |
| ---------- | ------------------------------------------------------------------------------------ |
| 2          | Reserved storage semaphore.                                                          |
| 3–5        | Input read index, occupied-word count, unsignalled-word count.                       |
| 6–37       | Sixteen unsigned input words stored as `(low14, high2)` SmallInteger pairs.          |
| 38–39      | Registered cursor and display Forms.                                                 |
| 40         | Snapshot registration Array: four serial-byte SmallIntegers, leader low14/high2.     |
| 41–43      | Previous input timestamp low14/mid14/high4; component 41 is nil before first packet. |

Root30's seven-field Array holds semaphore, three low-first 14-bit identity-threshold chunks, then
three word-threshold chunks. Counts are copied, so mutating an original integer argument cannot
change registration. Delivery clears the registration before signalling.

At `boundary`, scratch VRs are released and `save_context` writes IP/SP. Next,
`check_process_switch` consumes any pending process rooted in hardware ESTK slot 1, then
`check_low_space` can collect once and signal, then `check_device_events` delivers input/timer
notifications. Scheduler/event paths revisit switch checks before returning to `cycle`. If no
process is runnable, the machine retires an interrupt-wait loop. Guest queues and context changes
remain microcode work.

Three uses of hardware ESTK must not overlap: pending process in slot 1 with SP=1; `perform:` saved
method/Array in slots 1–2 with SP=2; BitBlt's private frame with SP=31. Those primitives release
their private frame before entering ordinary boundary processing. Slot 0 remains compact
construction scratch when the higher slots hold roots.

## BitBlt frame and traversal

BitBlt takes no arguments and returns its receiver. Its receiver fields are destForm, sourceForm,
halftoneForm, rule, destX, destY, width, height, sourceX, sourceY, clipX, clipY, clipWidth,
clipHeight. Nil source supplies ones. A non-nil halftone must be 16×16 and tiles at absolute
destination coordinates. Form offsets are handled by guest drawing methods.

| Hardware ESTK slots | Contents                                                                                |
| ------------------- | --------------------------------------------------------------------------------------- |
| 0                   | Validation/compact scratch.                                                             |
| 1–3                 | Destination, source, halftone Forms.                                                    |
| 4                   | Boolean rule 0–15.                                                                      |
| 5–8                 | Destination x/y and extent width/height, rewritten by clipping.                         |
| 9–10                | Source x/y, advanced with the destination during clipping.                              |
| 11–14               | Clip x/y/width/height.                                                                  |
| 15–16               | Destination/source stride in 16-bit words.                                              |
| 17–20               | Destination width/height, source width/height.                                          |
| 21                  | Snapshot object during alias copying; destination bitmap identity during refresh.       |
| 22–23               | Destination words per clipped row, first destination word; upload later reuses slot 22. |
| 24                  | Pass: 0 validation, 1 direct drawing, 2 source snapshot.                                |
| 25–26               | First/last destination word masks.                                                      |
| 27–29               | Saved caller IP, SP, and primitive failure continuation.                                |
| 30–31               | Form helper continuation, total snapshot words.                                         |

For each axis, clipping intersects parameter `t` in `[0, extent)` with destination bounds, clip
bounds, and source bounds when present. Add the lower limit to both source and destination, then
replace extent by upper minus lower. Empty copies succeed without allocation or publication.

Bitmap rows contain `ceil(width/16)` words. Physical destination component is
`2 + (destY+row)*stride + floor(destX/16) + word`. For an unaligned source, combine adjacent 16-bit
words into a 32-bit window, rotate to align, and take the high half. Out-of-row source words
contribute zero; clipping and edge masks exclude their extra bits.

Pass 0 validates every word the operation will read before any destination write. If source or
halftone bitmap identity equals destination identity, pass 2 snapshots aligned, halftone-masked
source words into an opaque scratch object; a later merge consumes it. Otherwise pass 1 draws
directly without allocation. Comparing bitmap identities catches distinct Forms sharing storage. No
guest process switch occurs between validation and drawing.

Boolean dispatch jumps to `bb_rule0 + 3*rule`. Each rule occupies exactly three control words,
including deliberate no-ops. Preserve that spacing when editing the implementation. The final merge
is `old XOR ((rule_result XOR old) AND mask)`, preserving pixels outside the rectangle. The prepared
write address survives arithmetic; launched writes are joined by the next barrier.

After drawing, matching registered bitmap identities trigger presentation. Matching geometry uses a
patch of changed 32-pixel groups, including unchanged edge neighbors; different geometry uses a
fully validated replacement. A malformed registration after drawing halts, since falling back could
repeat the operation. Both uploads publish atomically at the device, but guest drawing is not rolled
back on a subsequent device fault. The full
[BitBlt contract](../docs/smalltalk-primitives.md#bitblt) defines every Boolean rule and test
oracle.

## Collector phases

`ram-collector.uc` is a stopped-mutator, resident-only copying collector. LOGIK freezes roots and
saves the interrupted arithmetic/sequencer state. The loader supplies `PAGER_ENTRIES` and
`ROOT_COUNT`; the latter includes code literals and frozen machine state, not merely explicit root
slots. Its scratch registers are R0 slot/root index, R1 body offset, R2 size, R3 changed flag, R4
slot flags.

1. `collector` / `roots`: Begin a plan and mark each frozen root.
2. `dirty`: Seed retention-required resident slots, including persistence dependencies and the
   prepared-address root.
3. `pass` / `trace_slot` / `trace_body`: Trace marked objects' classes and scanned body fields;
   repeat while Mark reports a newly marked resident. Cycles require no recursive work stack.
4. `copy_slot` / `copy_body`: Stage each retained body in the inactive semispace and copy in
   ascending zero-based offsets. WriteBody uses the preceding maintenance read's data latch.
5. Commit all bases and allocator state together, then Return to retry the interrupted operation or
   advance beyond explicit Collect.

Mark compares complete references and never fetches missing bodies. Backing records are retained. No
published source mapping changes before Commit; failed planning/copying leaves the old heap
authoritative. Live bodies plus a pending allocation/refill must fit the destination half. LOGIK
permits one collection/retry per exhausted command, not an infinite retry loop. The
[recovery contract](../docs/recovery.md) details root indices, persistence retention, and faults.

## Validation and maintenance

Existing tests exercise these sources directly, including delayed memory and collection:

```sh
cargo test --locked -p rekursiv-asm
cargo test --locked -p rekursiv-smalltalk --test execution --test sends
cargo test --locked -p rekursiv-sim --test machine_gc --test pipeline
```

The Smalltalk send suite includes primitive success/fallback, operand preservation, process/events,
and BitBlt pixel-oracle checks. Original-image tests require the separately fetched pinned Xerox
fixture; follow [the image guide](../docs/smalltalk-image.md). A diagnostic root return halts with
R15=1 and result VR5; other runtime stop statuses are documented in `interpreter.uc` and
`system.uc`. Unsupported bytecodes halt, while unsupported primitives execute guest fallback.

When changing code, check reference liveness across every fetch/allocation, old-state dependencies
on combined controls, the chosen helper continuation register, and the point after which guest
mutation makes fallback invalid. Check assembled addresses too: sources share a namespace and
control store, and BitBlt's Boolean table relies on fixed instruction spacing. Comments consume no
control words and can be expanded without moving labels.
