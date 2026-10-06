# Smalltalk bytecode execution

The Smalltalk interpreter executes as [LOGIK microcode](../microcode/smalltalk/interpreter.uc).
OBJEKT supplies method bytes and context fields. NUMERIK performs address calculations and arithmetic.
Rust assembles the microcode and loads converted objects while the processor is halted.
During execution, the host supplies external memory and backing storage transactions only.

This is the initial interpreter for root method activations. Method lookup, calls, blocks, scheduling,
full primitive coverage, and image startup remain later work.
The simulation wrapper has 512 control words and 512 object-memory words.
It cannot hold the complete Xerox image in RAM.

## Execute the checks

Run the guest and RTL integration tests:

```sh
cargo test --locked -p rekursiv-smalltalk --test execution
```

Run the pinned image conversion and original-method execution checks:

```sh
scripts/check-smalltalk-image.sh
```

The original-method test executes Xerox `Object>>isNil` and `Object>>notNil` through RTL.
It preserves their identities, headers, bytecodes, and source trailers.
The test supplies a root context and receiver. It does not perform method lookup or image startup.

## Entry and results

`rekursiv_smalltalk::interpreter::assemble(active_context)` assembles the standalone source.
The caller loads the [converted object representation](smalltalk-image.md) before starting the processor.
The context must be a MethodContext with valid SmallInteger IP/SP fields and a converted compiled method.
Its sender must be guest nil for a supported return.
Initial arguments and temporaries must already occupy their context slots.

The interpreter reads the saved context IP/SP, method, and receiver through OBJEKT.
IP is the guest's one-based byte offset. SP counts occupied slots, including temporaries.
The interpreter writes these fields back at each bytecode boundary.
It clears popped slots to guest nil so unused stack fields cannot retain objects.

The simulator can install the existing machine collector alongside the interpreter:

```rust
let assembly = rekursiv_smalltalk::interpreter::assemble(active_context)?;
let program = rekursiv_model::processor::Image::from_assembly(&assembly)?
    .with_ram_collector(rekursiv_smalltalk::interpreter::COLLECTOR_ENTRY, 16)?;
h.load_processor(&program)?;
h.start_processor(assembly.entry.unwrap())?;
```

The collector occupies control addresses 384–451. Bootstrap bodies must fit in the initial semispace.
The current interpreter has 328 microinstructions. Its assembler rejects overlap with the collector.
The larger control store uses an existing RTL parameter; the hardware decoder contains no Smalltalk operations.
Mapping the control store to FPGA block RAM remains board implementation work.

R15 reports the terminal result:

| R15 | Meaning | Preserved state |
| --- | --- | --- |
| 1 | Root method returned | Tagged result in OBJEKT VR5; saved context IP/SP |
| 2 | Unsupported bytecode or send | Saved context and opcode in R13 |
| 3 | Initial arithmetic primitive failed | Receiver and argument remain on the stack; saved IP follows the opcode |
| 4 | Conditional branch received a non-Boolean | Operand remains on the stack; saved IP follows the instruction |
| 5 | Invalid context value, stack position, or index | Terminal stop; malformed boot state is not written back |

R15 is zero during normal execution. Hardware object-access failures remain processor faults with an OBJEKT status.
No terminal result authorizes a host callback to finish an operation.
Primitive failure will enter ordinary message lookup in the later sends stage.
This implementation stops at that boundary without discarding arguments or substituting a result.

## Bytecode coverage

The encoding follows the [Blue Book interpreter specification](https://docs.huihoo.com/smalltalk/esug/HistoricalDocuments/Smalltalk80/BlueBookImplementation/bluebook_chapter28.html).
The source has explicit NAM and opcode-map entries for all 256 byte values.
NAM stores a fixed identity mapping; guest instructions always come from OBJEKT.

| Bytes | Implemented behavior |
| --- | --- |
| 0–31 | Push receiver field or temporary |
| 32–95 | Push literal or literal Association value |
| 96–111 | Store and pop receiver field or temporary |
| 112–119 | Push receiver, singleton, or small constant |
| 120–124 | Return receiver, singleton, or stack top from a root method |
| 128–130 | Extended push, store, and store-and-pop |
| 135–137 | Pop, duplicate, and push active context |
| 144–175 | Short and long branches, including signed backward offsets |
| 176–184 | Integer `+`, `-`, `<`, `>`, `<=`, `>=`, `=`, `~=`, and `*` |
| 190–191 | Integer `bitAnd:` and `bitOr:` |

Other bytecodes stop with R15=2. Extended stores into literal constants stop with R15=5.
Arithmetic checks the complete compact tag before using a payload.
Integer results must fit the guest range -16384 through 16383.
Type errors and overflow preserve both arguments and stop with R15=3.
Conditional branches compare full stored singleton identities, not compact machine Booleans.

Method byte access translates guest IP to OBJEKT component `IP - literal_count`.
The interpreter reads the literal count and guest byte length from the converted method.
It checks the byte range before each fetch, including extension bytes.
It does not interpret source-location trailers or infer instruction boundaries from their contents.
Valid method control flow must terminate before reaching a trailer.

## Machine state and roots

| Location | Interpreter state |
| --- | --- |
| Explicit root 26; VR0 | Active context |
| VR1 | Compiled method |
| VR2 | Receiver |
| VR3 | Variable or Association target |
| VR4 | Pending store value |
| VR5 | Returned value |
| SYMBOL; ESTKR; evaluation slot zero | Tagged scratch values |
| R8; R9 | Guest IP; guest SP |
| R10; R11; R12; R14 | Literal count; method byte length; temporary count; context slot capacity |
| R13; R15 | Current bytecode; terminal status |
| R0–R7 | Numeric scratch values and microcode continuations |

Object references never depend on NUMERIK registers for liveness.
Context fields hold the guest stack, including all live temporaries and operands.
The existing machine root interface exposes value registers, tagged scratch values, and the explicit context root.
The collector scans ordinary tagged components. It needs no Smalltalk class or method-header knowledge.

The forced-refill test exhausts the active semispace and evicts the receiver before execution.
A field access triggers the machine collector, relocates live objects, and retries the fetch.
Execution then completes with the expected temporary and result. Host maintenance counts remain unchanged.

## Validation boundary

A test-only interpreter calculates expectations using original source oops and byte offsets.
It does not use physical descriptors or microcode control flow.
Tests compare guest IP, temporaries, and stack contents at each instruction boundary.
They read external RAM written by RTL and check the materialized context IP/SP.
The generic processor oracle also compares every microinstruction retirement and OBJEKT transaction.
Neither expectation model supplies data, control flow, or recovery actions to the running processor.

Tests cover extended field addressing, negative branches, integer boundaries, type failures,
non-Boolean branches, stack underflow/overflow, invalid boot fields, and unsupported operations.
They also check that failed primitives preserve operands and popped slots contain guest nil.
