# NUMERIK floating-point extension

NUMERIK executes binary32 arithmetic in synthesizable RTL. The floating-point unit receives raw
numeric bits and returns a result plus exception flags. Smalltalk classes, primitive numbers,
allocation, and failure policy stay in [microcode](../microcode/smalltalk/floats.uc). The
implementation uses
[Berkeley HardFloat Release 1](https://www.jhauser.us/arithmetic/HardFloat.html), distributed under
its BSD license. [Vendor provenance](../vendor/hardfloat/README.md) records the source archive,
checksum, specialization, and compatibility patch.

## Architectural operations

`alu=Float` selects the floating-point unit. The `fp` field selects the operation; R and S use the
existing NUMERIK operand sources. The result uses the existing register-B, Q, and evaluation-stack
ALU destinations. Operation numbers belong to NUMERIK, independently of any guest language.

| `fp`           | Encoding | R                       | S        | Result                                          |
| -------------- | -------: | ----------------------- | -------- | ----------------------------------------------- |
| `Add`          |        0 | binary32                | binary32 | R + S                                           |
| `Subtract`     |        1 | binary32                | binary32 | R - S                                           |
| `Multiply`     |        2 | binary32                | binary32 | R × S                                           |
| `Divide`       |        3 | binary32                | binary32 | R / S                                           |
| `Sqrt`         |        4 | binary32                | Ignored  | Square root of R                                |
| `Compare`      |        5 | binary32                | binary32 | One of: less=1, equal=2, greater=4, unordered=8 |
| `FromSigned`   |        6 | Signed 32-bit integer   | Ignored  | binary32                                        |
| `ToSigned`     |        7 | binary32                | Ignored  | Signed 32-bit integer                           |
| `FromUnsigned` |        8 | Unsigned 32-bit integer | Ignored  | binary32                                        |
| `ToUnsigned`   |        9 | binary32                | Ignored  | Unsigned 32-bit integer                         |

`round` selects `NearestEven=0`, `TowardZero=1`, `Down=2`, `Up=3`, or `NearestAway=4`. The default
is nearest, ties to even. Binary32 supports gradual underflow, signed zeros, infinities, and NaNs.
Tininess is detected after rounding. Arithmetic NaNs use the canonical quiet encoding `0x7fc00000`.
Compare is quiet: any NaN produces unordered, but only a signaling NaN raises invalid.

Invalid integer conversion returns a saturated result. Negative overflow returns signed minimum or
unsigned zero. Positive overflow and NaN return the maximum integer for the selected signedness.
These result bits are deterministic; callers must still inspect the invalid flag.

`alu=FloatStatus` reads the exception register as an unsigned integer. Bits are:

| Bit | Meaning                                                         |
| --: | --------------------------------------------------------------- |
|   4 | Invalid operation, including an out-of-range integer conversion |
|   3 | Division by zero                                                |
|   2 | Floating-point overflow                                         |
|   1 | Underflow                                                       |
|   0 | Inexact                                                         |

Each retired Float operation replaces the exception register. The flags are not sticky. Integer
operations and FloatStatus reads leave it unchanged. Reset clears it; collection saves and restores
it with other NUMERIK state. The ordinary `flags` control still records integer zero/sign conditions
from the result bit pattern. It does not select floating-point exceptions.

For example, this sequence converts a Float bit pattern in R2 to a signed integer and reads its
exceptions:

```text
ra=2, alu=Float, fp=ToSigned, round=TowardZero, ldq
alu=FloatStatus, rb=4, ldrb
ra=4, s=Branch, brch=16, alu=And, flags
seq=ConditionalJump, cc=!Zero, brch=conversion_failed
```

Floating-point operation and rounding fields must be zero on other ALU operations. A Float
instruction cannot combine an OBJEKT command, device transaction, recovery operation, carry input,
destination shift, or compact-value construction. These combinations fail encoding checks before
execution. A separate instruction can construct a compact value after conversion and range checks.

## Completion and retirement

[numerik_fp32.sv](../rtl/numerik_fp32.sv) defines the backend interface:

| Channel  | Signals                                         | Contract                                                      |
| -------- | ----------------------------------------------- | ------------------------------------------------------------- |
| Request  | `valid_i`, `ready_o`, operation, rounding, A, B | Accept once on a rising edge with both handshake signals high |
| Response | `valid_o`, `ready_i`, result, exceptions        | Hold all response fields until accepted                       |
| Reset    | `rst_i`                                         | Cancel pending work and discard unconsumed responses          |

The backend accepts one operation at a time and latches all request fields. The portable
implementation registers operands and results. Division and square root share an iterative unit. No
architectural latency or throughput is promised. LOGIK enters `NUMERIC_WAIT` and retires the
instruction only when the result is valid. Register writes, stack effects, exception updates, and
sequencing occur once at retirement. A branch condition is captured before the wait, so an interrupt
transition cannot redirect a pending instruction.

The interface permits a device-specific backend. Any replacement must pass the same numerical,
handshake, reset, and retirement tests. FP64 can use a separate future operation family and operand
convention. The current extension does not implement it.

## FPGA mapping

The Mellanox MNV303212A-ADLT card uses an XCKU15P and 8 GB DDR4. See the
[manufacturer product brief](https://www.mellanox.com/related-docs/prod_adapter_cards/PB_Innova-2_Flex.pdf).
Its DSP48E2 blocks provide integer multiplication and arithmetic used to construct floating-point
operators. HardFloat's significand multiplier uses a Verilog multiplication operator that synthesis
can map onto those blocks.

Run the isolated backend mapping with:

```sh
scripts/synth-float.sh
```

Yosys 0.33, targeting `xcup`, maps this backend to two DSP48E2 blocks, 3654 LUT primitives, and 212
flip-flops. The netlist also contains carry and wide-multiplexer primitives. These counts exclude
NUMERIK integration and the rest of the processor. The report is `artifacts/numerik-fp32-xcup.log`;
the mapped netlist is `artifacts/numerik-fp32-xcup.json`. This is synthesis evidence, not a placed
design, a timing result, or proof of board fit. Vivado implementation must establish resource
packing and achievable clock frequency on the actual part.

AMD's Floating-Point Operator IP is a possible alternative, but is not integrated here. Its
documented treatment of subnormals differs from this contract for most operations. It cannot replace
this backend unchanged merely because both interfaces use binary32. See the
[AMD IP guide](https://docs.amd.com/api/khub/documents/ym1A7qsltTGP_saZFTrikQ/content).

## Smalltalk policy

Primitives 40–54 execute in the machine. Float objects retain their two 16-bit IEEE words behind the
language descriptor. Microcode checks Float class and representation, loads the words, executes
numeric operations, and allocates the result through OBJEKT. The collector sees tagged object
fields; it never needs to recognize floating-point numbers.

Guest Float primitive inputs must be finite. Invalid operation, division by zero, or floating-point
overflow causes primitive fallback with caller operands unchanged. Inexact and gradual-underflow
results are accepted. Truncation additionally checks the signed 15-bit SmallInteger range. Exponent
extraction normalizes subnormals in microcode and returns -1 for zero, matching the image's fallback
definition. Power-of-two scaling handles normal results through exponent adjustment and uses one FPU
multiplication for rounding into the subnormal range. Fractional part uses conversions and
subtraction when fractional bits exist; integral values return positive zero. No host arithmetic
result is used to complete these operations.

## Validation

```sh
cargo test --locked -p rekursiv-sim --test float --test processor
cargo test --locked -p rekursiv-smalltalk --test sends
scripts/lint.sh
scripts/synth-float.sh
```

The standalone backend test compares 34,200 result/exception pairs against the independent Berkeley
SoftFloat reference. It covers all ten operations and five rounding modes, boundary values, random
bits, and response stalls. A separate test resets the unit during division and while a completed
response is held. Processor tests check single retirement, interrupt-condition capture, and
exception preservation across collection. Smalltalk tests cover arithmetic, comparisons, signed
conversion, truncation, fraction, exponent, scaling, and primitive fallback under delayed memory and
collection.

The C reference supplies numeric semantics to the architectural oracle and native microcode
emulator. It is compiled portably without host floating-point operations or architecture-specific
assembly. Its expected results are compared with RTL outputs and never supplied to RTL execution.
The separately selected native emulator uses these semantics to execute NUMERIK instructions in
software.
