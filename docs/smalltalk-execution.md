# Smalltalk bytecode execution

The Smalltalk interpreter executes as [LOGIK microcode](../microcode/smalltalk/interpreter.uc),
with separate sources for sends, blocks, primitives, and scheduling.
OBJEKT supplies method bytes, dictionaries, and context fields. NUMERIK performs address calculations and arithmetic.
Rust assembles microcode and loads converted objects while the processor is halted.
During execution, the host supplies external memory, backing storage, and peripheral transactions.
The [microcode reading guide](assembler.md#reading-a-microcode-routine) explains the controls with examples from these sources.

Sends, blocks, non-local returns, failed sends, and guest process scheduling execute in microcode.
Identity conversion and instance enumeration also execute in microcode, using generic OBJEKT directory operations.
Primitive coverage remains incomplete. Storage integration and complete interactive image qualification remain outstanding.
The default simulation profile has 4096 control words and 512 object-memory words, split into two 256-word semispaces.
`rekursiv_sim::runtime_with_memory(words)` compiles the same RTL with a larger external RAM capacity and a separate build cache.
The harness reads the capacity from RTL and sizes its external RAM model accordingly.
The original LargeInteger tests use 65536 words because image dictionaries can exceed the small profile's semispace.

## Execute the checks

Run the guest and RTL integration tests:

```sh
cargo test --locked -p rekursiv-smalltalk --test execution --test sends
```

Run pinned image conversion and original-method execution checks:

```sh
scripts/check-smalltalk-image.sh
```

The original-method tests execute Xerox `Object>>isNil` and `Object>>notNil` through RTL.
A send test executes `ExternalStream class>>new`, which invokes `Behavior>>basicNew` and returns a newly allocated object.
That test keeps every original image object unchanged and adds an isolated receiver class and root context.
It forces refill recovery. It does not perform image startup.
Another original-image test exercises each LargePositiveInteger arithmetic fallback, primitives 21–37.
Four String/ByteArray replacement methods also run through their original fallback bytecodes.
These tests add operands and a root context, keep all original objects unchanged, and force collection.
Original Cursor and DisplayScreen methods also register their existing bitmaps through RTL, with pixel-by-pixel checks of the published frames.
The original AltoFile snapshot-target method registers serial bytes and an unsigned leader address through RTL.
They use the larger external RAM profile and an optimized Rust test build for full-image comparisons.

## Entry and results

`rekursiv_smalltalk::interpreter::assemble(active_context)` assembles the standalone sources in `microcode/smalltalk/`.
The caller loads the [converted object representation](smalltalk-image.md) before starting the processor.
The context must be a MethodContext with valid SmallInteger IP/SP fields and a converted compiled method.
Initial arguments and temporaries must already occupy their context slots.
A nil sender terminates only the initial diagnostic root invocation. Other returns validate and resume their target context.

The halted loader reserves the imported identity range with `Service::ReserveIdentities(32768)`.
It configures compact code 2 as class SmallInteger and supplies initial resident objects and backing records.
The reservation changes no heap data and never lowers the allocator counter.
Allocation and collection during execution require no further host service calls.

The interpreter reads saved context IP/SP, method, and receiver through OBJEKT.
IP is the guest's one-based byte offset. SP counts occupied slots, including temporaries.
The interpreter writes these fields back at each bytecode boundary and before activation.
It clears popped slots to guest nil so unused stack fields cannot retain objects.

The simulator installs the machine collector alongside the interpreter:

```rust
let assembly = rekursiv_smalltalk::interpreter::assemble(active_context)?;
let program = rekursiv_model::processor::Image::from_assembly(&assembly)?
    .with_ram_collector(rekursiv_smalltalk::interpreter::COLLECTOR_ENTRY, 16)?;
h.load_processor(&program)?;
h.start_processor(assembly.entry.unwrap())?;
```

The collector occupies control addresses 3968–4035. Bootstrap bodies must fit in the initial semispace.
The assembler rejects interpreter code that overlaps the collector.
The larger control store uses an existing RTL parameter. The hardware decoder contains no Smalltalk operations.
Mapping the control store to FPGA block RAM remains board implementation work.

R15 reports the terminal result:

| R15 | Meaning | Preserved state |
| --- | --- | --- |
| 1 | Root method returned | Tagged result in OBJEKT VR5; saved root context IP/SP |
| 2 | Unsupported bytecode | Saved context and opcode in R13 |
| 5 | Invalid context value, stack position, or index | Terminal stop; malformed boot state is not written back |
| 6 | Lookup of `doesNotUnderstand:` also failed | Caller holds the failed-send receiver and Message argument |
| 7 | Method argument count or temporary capacity is inconsistent | Caller operands remain unchanged; no activation |
| 10 | Guest quit | Machine halted; advanced context IP/SP and receiver result preserved |
| 11 | Guest debugger entry | Resumable service break 11; context saved before the break |

R15 is zero at ordinary instruction boundaries. Value 16 is internal return state, not a terminal result.
Hardware object-access failures remain processor faults with an OBJEKT status.
No terminal result authorizes a host callback to finish an operation.
Failed sends construct a guest Message and argument Array, then send `doesNotUnderstand:`.
A non-Boolean branch operand receives `mustBeBoolean`. A dead return target triggers `cannotReturn:`.

## Bytecode and primitive coverage

The encoding follows the [Blue Book interpreter specification](https://docs.huihoo.com/smalltalk/esug/HistoricalDocuments/Smalltalk80/BlueBookImplementation/bluebook_chapter28.html).
The source has explicit NAM and opcode-map entries for all 256 byte values.
NAM stores a fixed identity mapping. Guest instructions always come from OBJEKT.

| Bytes | Implemented behavior |
| --- | --- |
| 0–31 | Push receiver field or temporary |
| 32–95 | Push literal or literal Association value |
| 96–111 | Store and pop receiver field or temporary |
| 112–119 | Push receiver, singleton, or small constant |
| 120–124 | Return receiver, singleton, or stack top to the home method's sender |
| 125 | Return stack top to the active block's caller |
| 128–130 | Extended push, store, and store-and-pop |
| 131–134 | Single/double extended sends and super sends |
| 135–137 | Pop, duplicate, and push active context |
| 144–175 | Short and long branches, including signed backward offsets |
| 176–207 | Special-selector sends; selected integer operations have fast paths |
| 208–255 | Literal-selector sends with zero, one, or two arguments |

Unused bytecodes stop with R15=2.
Extended stores into literal constants stop with R15=5.
Fast integer operations are `+`, `-`, `<`, `>`, `<=`, `>=`, `=`, `~=`, `*`, `bitAnd:`, and `bitOr:`.
Arithmetic checks the complete compact tag before using a payload.
Integer results must fit the guest range -16384 through 16383.
Type errors and overflow preserve operands, then send the selector from the guest's fixed special-selector array.
Conditional branches compare full stored singleton identities, not compact machine Booleans.

Method byte access translates guest IP to OBJEKT component `IP - literal_count`.
The interpreter reads literal count and guest byte length from the converted method.
It checks the byte range before each fetch, including extension bytes.
It does not interpret source-location trailers or infer instruction boundaries from their contents.
Valid method control flow must terminate before reaching a trailer.

Method-header flags 5 and 6 implement quick receiver return and quick instance-variable load without context allocation.
Extended headers supply argument counts and primitive numbers.
Primitive methods dispatch by their guest primitive number. Failed checks retain the original receiver and arguments for the Smalltalk fallback body.

| Primitives | Implemented behavior |
| --- | --- |
| 1–18 | SmallInteger arithmetic, comparison, division, remainder, bit operations, shifts, and Point construction |
| 40–54 | Float conversion, arithmetic, comparison, truncation, fraction, exponent, and power-of-two scaling |
| 60–64 | Indexed reads/writes, size, and String Character conversion |
| 65–67 | Array/String stream reads, writes, and end tests |
| 68–69 | Read immutable CompiledMethod headers; read and replace full-width literals |
| 70–74 | Fixed/indexed allocation, `become:`, and instance-field access |
| 75–78 | Identity-number conversion and ordered instance enumeration |
| 79 | CompiledMethod allocation with separate literal and byte regions |
| 80–82 | Block creation and activation, including argument Arrays |
| 83–84 | Dynamic `perform:` sends with direct arguments or an Array |
| 85–89 | Semaphore signal/wait, process resume/suspend, and cache flush |
| 90–95 | Mouse polling, cursor position/link, input registration, timed sampling configuration, and buffered input words |
| 96 | BitBlt: Boolean rules, clipping, halftones, overlapping copies, and registered bitmap refresh |
| 98–100 | Clock reads into byte objects; one-shot timer registration, replacement, and cancellation |
| 101–102 | Cursor/display Form validation, bitmap packing, atomic publication, and rooted registration |
| 110–111 | Full tagged identity comparison and class lookup |
| 112–116 | Allocation capacity queries, quit, resumable debugger entry, and low-space notification |
| 135 | Copied snapshot serial number and virtual leader-address registration |

Mouse polling and cursor positioning require signed 15-bit SmallInteger coordinates. Out-of-range coordinates enter guest fallback.
Cursor publication updates both coordinates together. The pointer device samples changed positions at the configured minimum interval.
Microcode converts raw packets into a rooted word ring and signals the guest input semaphore once per word.
Primitive 95 returns unsigned words as SmallIntegers or LargePositiveIntegers; empty reads enter guest fallback.
Bitmap registration publishes a complete initial frame through the [device interface](devices.md#cursor-and-display-bitmaps).
BitBlt updates registered cursor/display bitmaps after drawing, including Forms sharing the same bits object.
The [BitBlt contract](smalltalk-primitives.md#bitblt) defines validation, clipping, and temporary storage.
Snapshot-target registration copies four serial bytes and an unsigned 16-bit leader address. It does not write a snapshot.
`flushCache` succeeds without work because lookup currently has no method cache.
Integer division distinguishes exact division, floor division, and truncating quotient. Remainder follows floor division.
Out-of-range SmallInteger results enter the guest fallback. LargePositiveInteger byte objects support unsigned 16-bit indexed values.
General LargeInteger arithmetic still uses guest fallback.
[Float primitives](numerik-floating-point.md#smalltalk-policy) execute through the generic NUMERIK floating-point unit and language microcode.

CompiledMethod byte indexing currently supports the bytecode region. Byte access into the header/literal prefix fails the primitive.
CompiledMethod allocation validates the receiver's byte-instance specification before allocation and accepts compatible subclasses.
Malformed receivers or arguments preserve the original send operands for guest fallback.
Unsupported primitives enter their guest fallback. This behavior alone does not establish image compatibility or a usable startup path.
The [image primitive inventory](smalltalk-primitives.md) lists every declaration and remaining execution/device requirement.

## Lookup and contexts

Lookup uses the low 15 bits of the selector identity and the dictionary capacity mask.
It probes the selector table, wraps at the end, and stops at nil or after one complete traversal.
The parallel method Array supplies the matching CompiledMethod.
Lookup continues in the superclass after a dictionary miss. No host method table or method cache participates.

Super sends read the defining-class Association from the current method's last literal.
They begin lookup in that class's superclass, even when the receiver belongs to a deeper subclass.
SmallInteger receivers start lookup in the guest SmallInteger class. Stored receivers use their OBJEKT class metadata.

Activation allocates a scanned MethodContext with 12 or 32 temporary/stack slots, as the header specifies.
The context also holds sender, guest IP/SP, method, and receiver.
Microcode initializes all fields to guest nil, writes the header fields, and copies receiver and arguments in order.
Each source slot is cleared only after the destination write completes.
Arguments occupy the first temporary slots. Remaining temporaries start as guest nil.
The caller's SP decreases before execution switches to the new context.

A normal return keeps its result rooted, clears the completed context's sender and IP, and resumes the caller.
The caller receives the result on its guest stack. Captured context objects remain ordinary guest-visible objects.
A BlockContext carries its caller, initial IP, argument count, and home MethodContext.
Block activation copies arguments into its own stack and uses the home context for method, receiver, and temporary access.
Local block returns resume the caller. Method returns from a block target the home method's sender.
An escaped block can still access home temporaries. An invalid non-local return sends `cannotReturn:` with the intended result.

`perform:` checks the target method's argument count before rearranging caller operands.
`perform:withArguments:` also checks Array type and context capacity. Failed checks preserve the original primitive call.
A missing dynamic selector follows the same Message construction path as an ordinary failed send.

## Identity and process state

`asOop` preserves the imported identity encoding: stored identities below 32768 map to signed 15-bit SmallIntegers.
The image reserves positive integer codes 32768–65535 for immediate SmallInteger identities.
New stored identities therefore encode as identity plus 32768 in a LargePositiveInteger.
`asObject` accepts either representation, reverses that mapping, and fails if the stored identity does not exist.
This preserves round trips across all 37 hardware identity bits without truncation or reuse.

`someInstance` and `nextInstance` use the generic OBJEKT directory commands.
Microcode compares candidate classes; the directory reads no object bodies.
Enumeration follows increasing identity and fails at the end, including when no instance exists.
It covers resident and committed objects, including objects larger than the resident RAM window.

`become:` uses the generic OBJEKT Exchange command described in the [machine interface](interface.md#object-binding-exchange).
RTL streams both bodies and metadata into a private backing-store batch, then publishes both bindings together.
Aliases retain their identities and observe the exchanged contents and classes. The host does not implement a Smalltalk swap.

The scheduler reads the guest Processor association, active Process, priority queues, and Semaphore fields through OBJEKT.
Resume preempts only for a higher priority. Equal or lower priorities enter the corresponding FIFO queue.
Wait consumes an excess signal or queues the active process. Signal wakes a waiter or increments the excess count.
Suspend accepts the active process and selects the highest-priority runnable process.
The switch writes the old process's suspended context and clears the new process's suspended-context field.
A pending process stays rooted in hardware evaluation-stack slot one until the bytecode boundary performs the switch.

When no process is runnable, microcode completes the wait/suspend send and enters an interrupt wait loop.
At completed bytecode boundaries, it consumes counted external notifications and signals registered semaphores through the same guest queue operations.
A higher-priority waiter preempts a running process. An idle processor resumes an available waiter without requeuing the blocked process.
Input registration accepts Semaphore or nil. Timer registration roots its recipient until cancellation, replacement, or one-shot delivery.
The [device contract](devices.md) defines clock packing, modular deadlines, and one-shot low-space notification.
Low-space checks compare hardware counters with copied thresholds at bytecode boundaries. Storage registration remains unfinished.

## Machine state and roots

| Location | Ordinary bytecodes | Sends and activation |
| --- | --- | --- |
| Explicit root 26 | Initial diagnostic root context | Same initial root |
| Explicit roots 27–28 | Input and timer semaphore registrations | Same registrations |
| Explicit root 29 | Private device Array: input buffer, cursor/display Forms, snapshot target, and reserved storage semaphore | Same Array |
| Explicit root 30 | Low-space registration Array: semaphore and copied thresholds | Replacement publishes a fully initialized Array; delivery/cancellation clears the root |
| Explicit root 31 | Raw scheduler idle flag | Set after wait/suspend finds no runnable process; cleared on a switch |
| VR0 | Active context | Caller, then new active context |
| VR1; VR2 | Active method; receiver | Caller method; receiver |
| VR3 | Variable or Association target | Lookup class |
| VR4 | Pending store value | Selector |
| VR5 | Return value or scratch | Dictionary, new context, or result |
| VR6 | Scratch | Send receiver |
| VR7 | Scratch | New method or saved sender |
| SYMBOL; ESTKR; evaluation slot zero | Tagged scratch values | Tagged value during argument transfer |
| R8; R9 | Guest IP; SP | Caller IP; SP until activation |
| R10; R11; R12; R14 | Literal count; byte length; stack floor; slot capacity | Reused, then restored from the active context |
| R13; R15 | Current bytecode; status | Same, with internal return state |
| R0–R7 | Numeric scratch and microaddresses | Counts, probe indices, and transfer cursor |

R12 is the method temporary count for a MethodContext and zero for a BlockContext. Temporary access resolves the home context separately.
`perform:` temporarily roots its original method and argument Array in hardware evaluation-stack slots one and two.
Object references never depend on NUMERIK registers for liveness.
The caller stack retains arguments across context allocation. VR5 roots a new context before any further fetch.
SYMBOL protects each copied value across paging or collection. VR5 also protects returned values across caller refill.
Completed operations release scratch roots at instruction boundaries.
The generic collector scans tagged components without recognizing Smalltalk classes or method headers.

## Validation boundary

A test-only interpreter calculates basic-bytecode expectations using source oops and byte offsets.
Tests compare guest IP, temporaries, and stack contents at instruction boundaries.
Send tests check expected method transitions, argument order, sender links, quick returns, and captured contexts.
They read external RAM written by RTL and check the materialized context fields.
The generic processor oracle compares every microinstruction retirement and OBJEKT transaction.
Neither expectation model supplies data, control flow, or recovery actions to the running processor.

A converted send path makes 32 allocations, including the callee context, and returns its first allocated guest object.
The test requires allocation recovery and dirty eviction with a 16-entry pager and 256-word active semispace.
The original Xerox-method test separately requires refill recovery.
The host maintenance count remains unchanged during both runs.

Stage 4 tests also cover escaped blocks, failed sends, primitive fallback, dynamic sends, streams, identity exchange, and process switches.
They use delayed memory and backing-store devices and force collection/refill in directed cases.
The shared unsigned 16-bit argument decoder requires a matching byte descriptor and raw digits for LargePositiveInteger operands.
Malformed formats, tagged digits, and bits above the byte range enter fallback without changing the receiver or arguments.
Generic Exchange tests cover dirty residents, cold records, pager collisions, oversized bodies, and every memory/store failure position.
Directory tests cover resident/backing ordering, newer resident metadata, missing keys, full-width identities, and malformed device replies.
Guest identity tests round-trip all identity-width boundaries and enumerate oversized objects without fetching their bodies.
Asynchronous input tests cover registration replacement/clearing, repeated notifications, preemption, and idle wakeup with collection and delayed transfers.
Timer tests cover immediate/delayed expiry, rollover, replacement, cancellation, and invalid operands.
Bitmap tests cover row packing, padding, malformed Forms, missing devices, and preservation of the old frame on failure.
Capacity tests cover allocator reservations, exhausted identities, and guest integer results above 32 bits.
Low-space tests cover strict threshold crossing, one-shot rearming, operand preservation, copied values, and registration survival through repeated collection.
Quit/debugger tests check saved context fields and one-time continuation after debugger resume.
The saved-context startup test resumes the unchanged image and reaches its first BitBlt call after 2,176 bytecode boundaries and three collections.
The [primitive inventory](smalltalk-primitives.md) records the observed device-call order and the reproduction command.
Storage completion and startup beyond that boundary remain outside this coverage.
