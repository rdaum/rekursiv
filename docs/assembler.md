# Text microassembler

The files in `microcode/` contain executable LOGIK microcode. They assemble through the same checked
256-bit encoder used by the Rust API. The RAM collector, Smalltalk interpreter, and allocation and
collection examples use these files directly. Rust-sequenced OBJEKT test routines remain in Rust; they are not
processor microcode.

The syntax uses the book's horizontal style: one line specifies controls that act in parallel. `d`
names the data-bus source and `brch` supplies the branch field. Control names and encodings describe
this implementation, not an exact reconstruction of the historical assembler. Dependent operations
still require separate instructions: branches see old flags, and object results become available to
the next instruction.

```text
.equ CLASS = 0xa000000064
.entry start
start:
    d=7, r=Bus, rb=8, ldrb
loop:
    d=CLASS, page=Allocate, size=120, scan=0
    alu=Sub, ra=8, rb=8, s=Branch, brch=1, cin=One, ldrb, flags
    seq=ConditionalJump, cc=!Zero, brch=loop
    halt
```

Run a source file through the real RTL:

```sh
cargo run --locked -p rekursiv-sim -- --microcode microcode/collection.uc \
  --request-delay 3 --memory-latency 7 --response-stall 5
```

Add `--listing` to print encoded control words, including the collector, with the highest 32-bit
lane first. Add `--trace-file artifacts/collection.vcd` to record a waveform. The simulator installs
the standard collector at address 128 unless the source declares its own collector entry.
Overlapping code produces an error.

## Reading a microcode routine

Start with [the allocation example](../microcode/allocation.uc), then read
[the Smalltalk interpreter](../microcode/smalltalk/interpreter.uc). Read its register convention before following the labels.
[Message sends](../microcode/smalltalk/sends.uc) reuse several registers, so that file has its own convention.

Each instruction is one 256-bit control word. Read it in four parts:

1. **Sources:** What drives `D`, and what supplies the ALU operands?
2. **Operation:** What calculation or OBJEKT command occurs?
3. **Writes:** Which registers, flags, or memory locations change?
4. **Sequence:** Which microinstruction executes next?

Commas separate simultaneous controls. Their order on the line does not specify execution order.
Controls generally read state from before the instruction; their writes become available to the next instruction.
An instruction can take several clock cycles while OBJEKT completes a command.

### Registers and buses

| Name | Meaning |
| --- | --- |
| `D`, selected by `d=` | Shared 40-bit bus: a literal, register value, object response, or another selected source |
| `R0`–`R15`, selected by `ra` and `rb` | NUMERIK's 32-bit registers for arithmetic, counters, offsets, and microaddresses |
| `r`, `s` | ALU operand-source selectors; these are not register numbers |
| `Q` | A 32-bit NUMERIK result register, written with `ldq` |
| `VR0`–`VR7`, selected by `vr` | OBJEKT's 40-bit value registers; these preserve tags and act as collector roots |
| `Object` | The response latch from the most recent completed OBJEKT command |
| `SYMBOL` | A rooted 40-bit value register, written from D with `ldsym` |
| `ESTKR`, read as `d=Estk` | LOGIK's cached expression-stack value; also used to construct compact values |
| `IDX` | OBJEKT's component index, controlled by `idx`; distinct from NUMERIK's register selectors |

The Smalltalk evaluation stack lives in context objects. It is separate from LOGIK's hardware expression stack.

NUMERIK registers cannot preserve a complete 40-bit object reference.
Keep live references in rooted locations such as VRs, SYMBOL, or reachable object fields across allocation and refill.

### Loading and calculating

```text
d=7, r=Bus, rb=8, ldrb
```

Read this as: drive D with 7, select D as ALU operand R, and write the result into register R8.
The omitted `alu` defaults to `Pass`, so the result is its R operand. Without `ldrb`, R8 would remain unchanged.
Omitted `ra` and `rb` select register 0; an omitted `seq` continues to the next instruction.

By default, the ALU reads R from `R[ra]` and S from `R[rb]`.
Thus `rb` can select both an input register and the destination register:

```text
alu=Sub, ra=8, rb=8, s=Branch, brch=1, cin=One, ldrb, flags
seq=ConditionalJump, cc=!Zero, brch=loop
```

The first line computes `R8 - 1`, writes R8, and saves the arithmetic flags.
`s=Branch` uses the signed 16-bit `brch` field as an immediate operand.
Subtraction computes `R + ~S + carry`; `cin=One` gives ordinary subtraction.
The second line jumps to `loop` if the saved result was nonzero.

`flags` is an explicit write enable. Without it, arithmetic leaves the saved flags unchanged.
A branch on the same line as `flags` tests the **previous** flags.
Also, `brch` is one shared field: an immediate operand and a branch destination cannot have different values on one line.

For a comparison without a register write, omit `ldrb`:

```text
ra=9, rb=12, alu=Sub, cin=One, flags
seq=ConditionalJump, cc=Sign, brch=bad_state
```

Here R9 contains the guest stack pointer, and R12 contains the evaluation-stack floor.
For a MethodContext, that floor is the temporary count. For a BlockContext, it is zero.
The code rejects a stack pointer below the temporaries. These values are bounded nonnegative counts.
For general signed comparisons with possible overflow, use `CorrectedSign` instead of `Sign`.

`shift=Left` and `shift=Right` shift the ALU result by one bit before the destination write.
Arithmetic flags describe the result **before** that shift.

### Reading an object component

This sequence reads component 2 of the method held in VR1, then preserves its complete value in SYMBOL:

```text
read=Vr, vr=1
d=Object, page=Fetch
d=2, idx=Load
mem=Read
d=Object, ldsym
```

Follow the value through each instruction:

1. Read VR1 into the OBJEKT response latch.
2. Put that response on D and fetch the object, making it the current object.
3. Set IDX to 2.
4. Read the current object's component at IDX into the response latch.
5. Copy the response from D into SYMBOL.

`page=Fetch` can require a refill from backing storage. LOGIK waits for completion before advancing.
The backing-storage model supplies data; the RTL and microcode perform the object operation and any collection.

Treat `Object` as temporary. Every OBJEKT command replaces its response latch, including a command that loads a VR.
If another command intervenes, preserve the value first or read it back from its VR.
Likewise, loading IDX and reading memory need separate instructions: memory access uses the old index.

The interpreter often calculates a component index through Q:

```text
ra=8, rb=10, alu=Sub, cin=One, ldq
d=Q, idx=Load
mem=Read
d=Object, r=Bus, rb=0, ldrb
```

This computes `guest IP - literal count`, sets IDX, reads the bytecode component, and puts its raw byte into R0.
The intermediate Q matters: D cannot select the ALU result being produced by that same instruction.

### Raw numbers and tagged values

```text
d=42, r=Bus, rb=4, ldrb
ra=4, estk=Compact, compact=2
d=Estk, ldsym
```

The first line loads the raw integer 42 into R4.
The second constructs a tagged compact signed integer in ESTKR from the ALU result.
The third preserves that 40-bit value in SYMBOL.
In this interpreter, compact code 2 represents a SmallInteger. The generic hardware does not assign it Smalltalk language semantics.

Distinguish raw zero, guest `nil`, and a tagged SmallInteger containing zero. They have different bit patterns and uses.

### Following control flow

Labels name microcode addresses. Ordinary instructions continue at the next address unless `seq` changes the sequence.
Here is the interpreter's call to its byte-fetch helper:

```text
cycle:
    d=decoded, r=Bus, rb=7, ldrb, seq=Jump, brch=fetch_byte
decoded:
    ra=0, rb=13, ldrb
```

The first instruction stores the continuation address `decoded` in R7 and jumps to `fetch_byte`.
At the helper's end, `d=Register, ra=7, seq=Bus` jumps to that saved address.
This helper uses an explicit continuation convention; reading `seq=Jump` alone does not imply a pushed return address.

At `decoded`, the interpreter copies the returned byte from R0 into R13, then dispatches:

```text
d=Register, ra=13, apc=Bus
fetch=Nam
fetch=Map
seq=Dispatch
```

The byte selects a NAM entry, which selects a CSMAP entry, which supplies the microcode destination.
These steps occupy separate instructions because each consumes state produced by the preceding step.
The `.nam` and `.map` directives initialize those tables during loading; they are not executed instructions.

For a first Smalltalk walkthrough, follow these labels:

| Path | What to watch |
| --- | --- |
| `cycle` → `fetch_byte` → `decoded` | Read one guest byte and dispatch to its microcode handler |
| `push_integer` → `push` → `boundary` | Construct a value, write the context stack, and save IP/SP |
| `send_literal` → `send_prepare` | Obtain the selector, argument count, and receiver |
| `lookup_class` → `probe_selector` → `method_found` | Search dictionaries and follow superclass links |
| `activate` → `copy_argument` → `load_context` | Allocate a context, transfer arguments, and enter the selected method |
| `return_value` → `return_sender` → `resume_result` → `push` | Resume a sender and push the returned value |

The return path shown assumes a non-nil sender. A return from the diagnostic root halts instead.
Quick methods and supported primitives can produce a result without allocating a new context.

Keep [the interpreter's state and root conventions](smalltalk-execution.md) beside the source when tracing sends.
The Smalltalk library assembles `interpreter.uc` and `sends.uc` together and installs the collector.
Use its integration tests to execute complete examples:

```sh
cargo test --locked -p rekursiv-smalltalk --test execution
cargo test --locked -p rekursiv-smalltalk --test sends
```

For the collector, read [its recovery contract](recovery.md) before tracing [its source](../microcode/ram-collector.uc).
Its `gc=` controls invoke privileged hardware recovery operations. The contract explains what each operation does beyond the microcode loop.

## Syntax

Labels have the form `name:` and can share a line with an instruction. Forward label references are
supported. Labels and constants are case-sensitive; field and control names are case-insensitive.
Separate fields with commas. Duplicate fields, unknown controls, oversized values, and overlapping
words produce errors with line and column numbers. Comments start with `;`, `#`, or `//` and
continue to the end of the line.

Numbers use decimal, `0x` hexadecimal, or `0b` binary notation; underscores separate digits.
Expressions support symbols, parentheses, unary signs, addition, subtraction, and multiplication.
Multiplication has higher precedence than addition and subtraction. Expressions use checked integer
arithmetic and accept at most 128 tokens. A negative D-bus literal becomes a 40-bit two's-complement
value. Numeric literals are raw bit patterns: `d=42` does not construct a tagged SmallInteger.
`d=0xc20000002a` supplies a project compact signed integer containing 42.

| Directive                       | Meaning                                                        |
| ------------------------------- | -------------------------------------------------------------- |
| `.equ NAME = expression`        | Define a constant using previously defined constants           |
| `.org address`                  | Select the next microinstruction address                       |
| `.entry label`                  | Set the initial execution address; defaults to zero            |
| `.collector label`              | Set the recovery entry for a collector included in this source |
| `.map opcode, label`            | Initialize a CSMAP entry                                       |
| `.nam address, opcode, operand` | Initialize a 10-bit opcode and 30-bit operand                  |
| `.root slot, word`              | Initialize one of 32 explicit machine roots                    |

The simulator supplies `CODE_WORDS`, `STACK_WORDS`, `PAGER_ENTRIES`, and `ROOT_COUNT` as predefined
constants. Source cannot redefine them. The collector source is assembled at the requested origin
with the actual pager and root counts. File inclusion, macros, string literals, and historical
control aliases are not implemented.

## Control fields

| Fields                              | Values                                                                                                          |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `d`                                 | Literal expression or `Estk`, `Cstk`, `Object`, `Register`, `Apc`, `Ap`, `Sp`, `Namarg`, `Upcor`, `Q`, `Symbol`, `SymbolHigh`, `Device`, `Root` |
| `seq`, `cc`, `brch`                 | Sequence operation, condition with optional `!`, and label or 16-bit expression                                 |
| `ra`, `rb`                          | Register numbers 0–15                                                                                           |
| `alu`, `r`, `s`, `cin`, `shift`     | Arithmetic operation, operand sources, carry source, destination shift                                          |
| `fp`, `round`                     | Floating-point operation and rounding mode with `alu=Float`; see [numeric contract](numerik-floating-point.md) |
| `io`                              | `Read` or `Write`; register A holds the aligned 32-bit device address, D supplies write data |
| `esp`, `sp`, `estk`, `csp`, `cstk`  | Stack address, pointer, and data controls                                                                       |
| `apc`, `fetch`                      | Abstract program counter and NAM/CSMAP fetch controls                                                           |
| `page`, `idx`, `reg`, `mem`, `read` | OBJEKT command controls                                                                                         |
| `vr`, `class`                       | Value-register selector and expected-class guard                                                                |
| `size`, `scan`                      | Literal allocation size or `ra` for register A; scan flag 0 or 1                                                |
| `compact`                           | Compact construction code 0–3                                                                                   |
| `gc`                                | Collector operation or standalone `Collect` request                                                                                   |

Flags without operands are `halt`, `ldsym`, `ldmark`, `ldrb`, `ldq`, `flags`, `ldap`, `ldvr`, and `ldroot`.
`ldroot` writes D to an explicit root slot indexed by register A (0–31). `d=Root` reads that slot.
Both controls are mutator operations; the collector observes the frozen slots through its root interface.

`gc=Collect` requests the loaded collector from mutator microcode. It cannot combine other control fields.
It preserves the interrupted state, collects once, and then advances to the successor. Failure restores the request address and stops execution.

`read=FreeWords` and `read=FreeIdentities` return OBJEKT allocation capacity through the Object bus.
They do not require an object selection or issue external transfers. Counts include reservations retained after a failed allocation.

`d=SymbolHigh` reads the symbol register's high byte, zero-extended.
`estk=Wide` packs the D bus's low byte above the 32-bit arithmetic result and writes the full 40-bit stack word.
It applies no compact-value encoding or language-specific tag policy.
`nop` emits an otherwise empty control word. Unspecified fields take the encoder's default values.
Symbolic values match the enums in [processor.rs](../crates/rekursiv-asm/src/processor.rs) and
[lib.rs](../crates/rekursiv-asm/src/lib.rs). For example, `r=Register` selects register A and
`s=Register` selects register B. `r=Bus` and `s=Branch` select D and the signed 16-bit branch field.
The [interface specification](interface.md#project-control-word-encoding) defines field timing and
bit positions.
