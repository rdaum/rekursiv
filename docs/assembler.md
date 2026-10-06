# Text microassembler

The files in `microcode/` contain executable LOGIK microcode. They assemble through the same checked
256-bit encoder used by the Rust API. The RAM collector, allocation example, and collection example
now use these files directly. Rust-sequenced OBJEKT test routines remain in Rust; they are not
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
| `d`                                 | Literal expression or `Estk`, `Cstk`, `Object`, `Register`, `Apc`, `Ap`, `Sp`, `Namarg`, `Upcor`, `Q`, `Symbol` |
| `seq`, `cc`, `brch`                 | Sequence operation, condition with optional `!`, and label or 16-bit expression                                 |
| `ra`, `rb`                          | Register numbers 0–15                                                                                           |
| `alu`, `r`, `s`, `cin`, `shift`     | Arithmetic operation, operand sources, carry source, destination shift                                          |
| `esp`, `sp`, `estk`, `csp`, `cstk`  | Stack address, pointer, and data controls                                                                       |
| `apc`, `fetch`                      | Abstract program counter and NAM/CSMAP fetch controls                                                           |
| `page`, `idx`, `reg`, `mem`, `read` | OBJEKT command controls                                                                                         |
| `vr`, `class`                       | Value-register selector and expected-class guard                                                                |
| `size`, `scan`                      | Literal allocation size or `ra` for register A; scan flag 0 or 1                                                |
| `compact`                           | Compact construction code 0–3                                                                                   |
| `gc`                                | Privileged recovery operation                                                                                   |

Flags without operands are `halt`, `ldsym`, `ldmark`, `ldrb`, `ldq`, `flags`, `ldap`, and `ldvr`.
`nop` emits an otherwise empty control word. Unspecified fields take the encoder's default values.
Symbolic values match the enums in [processor.rs](../crates/rekursiv-asm/src/processor.rs) and
[lib.rs](../crates/rekursiv-asm/src/lib.rs). For example, `r=Register` selects register A and
`s=Register` selects register B. `r=Bus` and `s=Branch` select D and the signed 16-bit branch field.
The [interface specification](interface.md#project-control-word-encoding) defines field timing and
bit positions.
