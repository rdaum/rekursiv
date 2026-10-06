# Smalltalk bytecode execution

The Smalltalk interpreter executes as [LOGIK microcode](../microcode/smalltalk/interpreter.uc),
with [send and context routines](../microcode/smalltalk/sends.uc).
OBJEKT supplies method bytes, dictionaries, and context fields. NUMERIK performs address calculations and arithmetic.
Rust assembles microcode and loads converted objects while the processor is halted.
During execution, the host supplies external memory and backing storage transactions only.
The [microcode reading guide](assembler.md#reading-a-microcode-routine) explains the controls with examples from these sources.

Sends, method lookup, context activation, and normal returns are implemented.
Blocks, non-local returns, scheduling, full primitive coverage, and image startup remain later work.
The simulation wrapper has 1024 control words and 512 object-memory words.
Each collector semispace has 256 words. Large image objects can exceed this test configuration.

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

## Entry and results

`rekursiv_smalltalk::interpreter::assemble(active_context)` assembles both standalone sources.
The caller loads the [converted object representation](smalltalk-image.md) before starting the processor.
The context must be a MethodContext with valid SmallInteger IP/SP fields and a converted compiled method.
Initial arguments and temporaries must already occupy their context slots.
A nil sender terminates a diagnostic root invocation. Other normal returns resume the sender.

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

The collector occupies control addresses 896–963. Bootstrap bodies must fit in the initial semispace.
The interpreter has 663 microinstructions. Its assembler rejects overlap with the collector.
The larger control store uses an existing RTL parameter. The hardware decoder contains no Smalltalk operations.
Mapping the control store to FPGA block RAM remains board implementation work.

R15 reports the terminal result:

| R15 | Meaning | Preserved state |
| --- | --- | --- |
| 1 | Root method returned | Tagged result in OBJEKT VR5; saved root context IP/SP |
| 2 | Unsupported bytecode | Saved context and opcode in R13 |
| 4 | Conditional branch received a non-Boolean | Operand remains on the stack; saved IP follows the instruction |
| 5 | Invalid context value, stack position, or index | Terminal stop; malformed boot state is not written back |
| 6 | Method lookup failed | Receiver and arguments remain on the caller stack |
| 7 | Method argument count or temporary capacity is inconsistent | Caller operands remain unchanged; no activation |

R15 is zero at ordinary instruction boundaries. Value 16 is internal return state, not a terminal result.
Hardware object-access failures remain processor faults with an OBJEKT status.
No terminal result authorizes a host callback to finish an operation.
Missing-method handling through `doesNotUnderstand:` belongs to the remaining VM work.

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
| 120–124 | Return receiver, singleton, or stack top from a method |
| 128–130 | Extended push, store, and store-and-pop |
| 131–134 | Single/double extended sends and super sends |
| 135–137 | Pop, duplicate, and push active context |
| 144–175 | Short and long branches, including signed backward offsets |
| 176–207 | Special-selector sends; selected integer operations have fast paths |
| 208–255 | Literal-selector sends with zero, one, or two arguments |

Bytecode 125 needs block-return support. Unused bytecodes also stop with R15=2.
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
Primitive 70 implements `new` for stored, non-indexable pointer classes with fixed fields.
It allocates the descriptor and fields, then initializes every guest field to guest nil.
Unsupported primitive numbers, formats, or argument shapes execute the method's Smalltalk fallback body.
Indexed allocation and the remaining primitive implementations belong to stage 4.

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
Blocks and non-local returns require additional rules and are not implemented here.

## Machine state and roots

| Location | Ordinary bytecodes | Sends and activation |
| --- | --- | --- |
| Explicit root 26 | Initial diagnostic root context | Same initial root |
| VR0 | Active context | Caller, then new active context |
| VR1; VR2 | Active method; receiver | Caller method; receiver |
| VR3 | Variable or Association target | Lookup class |
| VR4 | Pending store value | Selector |
| VR5 | Return value or scratch | Dictionary, new context, or result |
| VR6 | Scratch | Send receiver |
| VR7 | Scratch | New method or saved sender |
| SYMBOL; ESTKR; evaluation slot zero | Tagged scratch values | Tagged value during argument transfer |
| R8; R9 | Guest IP; SP | Caller IP; SP until activation |
| R10; R11; R12; R14 | Header and context numeric state | Reused, then restored from the active context |
| R13; R15 | Current bytecode; status | Same, with internal return state |
| R0–R7 | Numeric scratch and microaddresses | Counts, probe indices, and transfer cursor |

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
