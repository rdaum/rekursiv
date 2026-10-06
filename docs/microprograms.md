# Stage 5 command routines

Stage 5 adds typed access, dictionary lookup, and object-backed context save/restore. The reusable
routines are in `crates/rekursiv-sim/src/microprograms.rs`. Rust supplies branching and temporary
values between commands. Every machine read, write, allocation, and register update executes through
the OBJEKT command interface. The harness compares each command against the independent
architectural model.

These routines define workloads to port to the implemented LOGIK sequencer. They still use Rust
control flow; the separate `processor` example executes its control flow in RTL. They do not
introduce a packed control store, a textual assembler, or a language instruction set. Stage 5 uses
the existing synthesizable RTL without new hardware operations.

## Typed access

`typed_read` and `typed_write` accept a target reference and an abstract index value. They execute
this sequence:

1. Fetch the target and read its class reference.
2. Fetch the class and read field one, its access-type reference.
3. Select the index and check its class against the access type in RTL.
4. Decode the index payload.
5. Fetch the target again and load the numeric index.
6. Execute the checked field read or write.

The final Fetch handles pager collisions between the target, class, and stored index. A `nil` access
type returns `TypeError` and prohibits typed indexing. Another non-reference access type returns
`BadValue`. A class without field one fails the checked class-field read. An incompatible index
class returns `TypeError` from the RTL type check.

| Index representation     | Numeric interpretation                                  |
| ------------------------ | ------------------------------------------------------- |
| Signed compact, code 2   | Sign-extend the 32-bit payload                          |
| Unsigned compact, code 3 | Zero-extend the 32-bit payload                          |
| Stored index             | One opaque body word containing a signed 40-bit integer |

The compact class mappings determine which compact indices match each access type. The example maps
signed and unsigned compacts to the same integer class. Other compact codes, scanned stored indices,
and stored indices with another size return `BadValue` after type checking. Zero, negative, and
excessive numeric indices reach the ordinary RTL bounds check. Rejected accesses do not perform the
target field write. Paging during the routine can still copy bodies and save victims.

The stored-index representation is a project convention for this workload. It does not define
general wide-number arithmetic or the remaining class fields.

## Dictionary lookup

`lookup` uses the alternating key/value layout from book section 7.9. Keys occupy odd fields, and
each value occupies the next field. The body size must be a power of two and contain at least two
words. The caller supplies an odd starting field within the body. The routine does not define a
hashing function or insertion policy.

The index register retains the starting field. `Index::Next` advances between keys and wraps through
field one. Search terminates at a matching key, a `nil` key slot, or the starting field after a
complete pass. A host iteration limit also detects an unexpected failure to wrap.

The result is `Some(value)` for a match and `None` for a miss. A matched value can itself be `nil`.
The `nil` key is reserved for empty slots and cannot be searched. Invalid sizes, starting fields,
and the reserved key return `BadValue`. The routine fetches a missing dictionary automatically and
preserves all value registers. It changes the selection, index, and index register.

## Context frame

`save_context` records the selected value, eight value registers, and both 40-bit indices. It
requires an existing selection and allocates a scanned 13-word object using the supplied frame
class. The routine returns the frame reference after writing all fields. The frame is then selected,
the index is 13, and the value registers and index register retain their previous values.

| Field | Content                                          |
| ----- | ------------------------------------------------ |
| 1     | Selected stored reference or compact value       |
| 2–9   | VR0 through VR7                                  |
| 10    | Index bits 31:0 as an unsigned compact           |
| 11    | Index bits 39:32 as an unsigned compact          |
| 12    | Index-register bits 31:0 as an unsigned compact  |
| 13    | Index-register bits 39:32 as an unsigned compact |

The upper numeric limbs must contain values from 0 through 255. The split encoding preserves every
index bit without creating false reference edges during collection. The scanned frame retains
objects referenced by the saved selection and value registers. The caller must retain the frame
itself in a register or an explicit software root.

`restore_context` checks the frame scan flag, class, size, and numeric limb encodings. It reads the
complete frame and resolves the saved selection before restoring value registers. It then restores
all eight value registers and both indices. The selection can be a compact value or a stored object
that requires refill. Pager residency and physical addresses can change without changing the
restored context.

This frame is an OBJEKT workload convention, not a historical LOGIK stack format. It excludes return
addresses, future arithmetic registers, global compact-class mappings, and allocator state. The
compact-class mappings must remain valid for a saved compact selection.

## Completion and errors

Each routine executes several commands and is not an atomic transaction. Typed access changes
selection and index, but preserves value registers and the index register. Dictionary lookup also
uses the index register. Failed routines can leave these temporary states changed. The atomicity
guarantees of individual commands still apply.

Errors retain their architectural `Status` inside the returned Rust error. Frame validation and
saved-selection errors occur before any value register is restored. A failed save can leave an
allocated, partly written frame that later collection can reclaim. The routine does not return that
incomplete frame as a successful result.

The routines do not collect or compact implicitly. Space exhaustion returns `OutOfSpace` to the
caller. Before explicit recovery, callers must register every input and intermediate reference they
need to retain. Collection occurs between routines in the examples, with the context frame
registered as a driver root.

## Examples and checks

Run the examples with command listings and delayed memory:

```sh
cargo run --locked -p rekursiv-sim -- --example typed-access --listing \
  --request-delay 3 --memory-latency 7 --response-stall 5
cargo run --locked -p rekursiv-sim -- --example paged-dictionary --listing \
  --request-delay 3 --memory-latency 7 --response-stall 5
cargo run --locked -p rekursiv-sim -- --example save-restore --listing \
  --request-delay 3 --memory-latency 7 --response-stall 5 \
  --trace-file artifacts/save-restore.vcd
```

The typed-access example uses compact and stored indices after eviction and checks rejected writes.
The dictionary example performs wrapped hits and bounded misses after new-object and modified-object
writeback. The context example evicts its frame and collects with that frame as the only explicit
software root. It restores every saved register and proves that saved object references remain
usable.

The directed tests include colliding target/class/index identities, invalid dictionary layouts, and
malformed context frames. The context property test adds 32 cases with arbitrary index bits, signed
register values, compact selections, collection, and variable delays. The
[validation record](validation.md) contains cycle counts and executed checks.
