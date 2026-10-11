# Shared Smalltalk execution core

Xerox Smalltalk-80 and Squeak 1.1 assemble these same control words with their own layout constants,
bytecode maps, method headers and primitive tables. The offline Rust assemblers concatenate sources
into one label namespace; ordinary `.equ` expressions provide the parameters. No runtime macro
processing or new hardware instructions are needed.

| Constant      | Xerox | Squeak | Meaning                                                            |
| ------------- | ----: | -----: | ------------------------------------------------------------------ |
| `FIELD_BASE`  |     2 |      3 | First guest field; also method-header and context-sender component |
| `CTX_IP`      |     3 |      4 | Context IP; SP immediately follows                                 |
| `CTX_STACK`   |     7 |      8 | Receiver component; stack top is this offset plus R9               |
| `TEMP_ROTATE` |    25 |     14 | Left rotation extracting method-header temporary count             |
| `TEMP_MASK`   |    31 |     63 | Temporary-count mask after rotation                                |

The profile also supplies `NIL`, `TRUE`, `FALSE`, `ACTIVE_CONTEXT` and the labels called by these
routines. Registers retain the documented interpreter ABI: VR0–VR2 root context/method/receiver,
VR3–VR7 root temporary values, R8–R14 cache the activation, and R15 carries status or a primitive
failure continuation.

| Source                        | Responsibility                                                                      |
| ----------------------------- | ----------------------------------------------------------------------------------- |
| `boundary.uc`                 | Release scratch roots, materialize IP/SP, scheduler entry, NAM bytecode dispatch    |
| `variables.uc`                | Receiver fields, home temporaries, literals/associations, extended stores           |
| `stack.uc`                    | Singleton/integer/context pushes, duplicate, checked peek/push/pop                  |
| `branches.uc`                 | Relative branches and Boolean checks                                                |
| `arithmetic.uc`, `compare.uc` | Compact-tag checks, arithmetic selection, signed comparisons and result publication |
| `returns.uc`                  | Local/nonlocal return selection and diagnostic root return                          |
| `activation.uc`               | Argument transfer, primitive results, sender resumption and error exits             |
| `primitives.uc`               | Full-tag identity and class primitives                                              |
| `dispatch.uc`                 | Constant-time dispatch into the profile's primitive jump table                      |

Primitive dispatch requires R0 in 0–255. Each profile's `primitive_table` contains exactly 256
contiguous, single-instruction jumps. Do not insert executable setup or padding between entries. The
dispatcher preserves numeric registers and roots; Q and sequencer return state are scratch. Xerox
unknown primitives activate the method body; Squeak distinguishes reserved failures from unsupported
required operations. Squeak quick returns above 255 are handled before table entry.

Fixed-size tables are independent of the 256-word NAM used for bytecode dispatch. The table
therefore works with the existing FPGA configuration. The collector's control-store reservation is
checked by both assemblers.

Scheduling relies on the hardware's old-operand rules. A VR read can load IDX from an immediate on
the same instruction; the following Fetch preserves IDX, including across refill and collection.
Independent NUMERIK address calculations can accompany Fetch. Context initialization writes the old
index while advancing it and comparing the old loop cursor; its following branch increments the
cursor but tests the previous flags. These reduce both microinstructions and OBJEKT handshakes on
hardware. Context saves at every bytecode boundary, popped-slot clearing, bounds checks, rooted
transfers and primitive fallback remain intact.

Keep byte encoding, numeric ranges, multiply overflow, method headers, lookup hashes,
scheduler/device policy and allocation representation in the profiles. Those differences are
intentional; sharing must not silently give either guest the other's semantics.
