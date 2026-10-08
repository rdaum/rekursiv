# Squeak 1.1 port

The offline tools read and convert the pinned Squeak 1.1 image into OBJEKT records.
They preserve the complete allocated graph, identity hashes, method bytes and binary64 Float bits.
A separate microcode profile restores the colour desktop and handles mouse menus in the native emulator.
This profile targets the archived 1.1 VM, not modern Squeak.
Binary64 arithmetic, allocation, collection, scheduling and drawing execute through machine instructions.
Synthetic programs compare CPU and object state against RTL, including paging, collection and packed-pixel drawing.
The full desktop has only been tested in the software emulator.

The target is the Windows archive at [files.squeak.org/1.1](https://files.squeak.org/1.1/).
The [fixture manifest](../crates/rekursiv-smalltalk/fixtures/squeak-1.1.json) pins the archive and its three data files.
The original executable is not needed or executed.

## Reproduce the conversion

Run the fetch, validation and conversion checks:

```sh
scripts/check-squeak-image.sh
```

The script writes the primitive inventory and converted records under `artifacts/squeak-1.1`.
The following commands inspect the cached image separately:

```sh
cargo run --locked -p rekursiv-smalltalk -- inspect-squeak artifacts/squeak-1.1/Squeak1.1.image
cargo run --locked -p rekursiv-smalltalk -- audit-squeak \
    artifacts/squeak-1.1/Squeak1.1.image artifacts/squeak-1.1/inventory.json
cargo run --locked -p rekursiv-smalltalk -- import-squeak \
    artifacts/squeak-1.1/Squeak1.1.image artifacts/squeak-1.1/rekursiv-image.json
```

`inspect-squeak` and `audit-squeak` accept format 6502 images without requiring the distribution checksum.
`import-squeak` requires the pinned image. These commands do not execute guest code.
The output profile is `squeak-1.1`. The Xerox microcode cannot execute it.

## Execute the saved context

```sh
cargo run --release --locked -p rekursiv-emulator -- \
    --squeak artifacts/squeak-1.1/Squeak1.1.image
```

The window defaults to 1024 × 768. Middle-click the background to open the system menu.
The emulator mounts the image, sources and changes files from the same directory.
These external files use DOS paths because the image only has Mac and DOS directory subclasses.
Guest writes modify the mounted copies in memory. They do not persist to host files.
Snapshot save/reload is not implemented.

For a deterministic headless run, add `--headless --steps 1000000000 --frame desktop.ppm`.
Add `--guest-trace startup.trace` to record bytecodes, guest IP/SP, method/context identities and primitive numbers.
It observes microcode boundaries and supplies no execution results.
Use `--engine interpreter` to disable the microinstruction JIT.
Use `--stop-at primitive_dispatch --when R0=101` to stop before the cursor primitive.
`--trace FILE` still records individual microinstructions.

Unknown required primitives stop with R15=8, R0=primitive number and VR7=method.
Primitives that the archived VM explicitly marks as failures execute their Smalltalk fallback.
A failed implemented primitive also preserves operands and executes its guest fallback.
An empty runnable queue enters an interrupt wait loop. Input and timer delivery can wake a waiting process.
Root returns use status 1; malformed state uses 5; argument mismatch uses 7.

Run execution and RTL tests with:

```sh
REKURSIV_SQUEAK_DIR="$PWD/artifacts/squeak-1.1" \
    cargo test --release --locked -p rekursiv-emulator --test squeak -- --include-ignored
```

## Evidence from the archive

The image uses little-endian format 6502, with a 64-byte header and a 1,674,684-byte heap.
It stores direct 32-bit object addresses with one-, two-, or three-word headers.
The Xerox image instead uses an object table and 16-bit oops.

| Pinned image property | Value |
| --- | ---: |
| Allocated objects | 31,737 |
| Compiled methods | 7,489 |
| Float objects | 1,255 |
| Declared nonzero primitive/quick-return codes | 182 |
| Converted physical body words | 779,941 |
| Saved context | `0x00911b18` |
| Saved method | `0x00843504`, `SystemDictionary>>snapshot:andQuit:` |
| Saved instruction pointer | 150 |
| Saved stack pointer | 5 |
| Next bytecode | 129, extended store |
| Saved display Form | 240 × 120, depth 8 |
| Requested window extent | 640 × 480 |

Primitive declarations do not prove runtime reachability or successful Smalltalk fallback.
The saved Form is smaller than the requested window. Startup must follow the guest restoration code.

`SqueakV1.sources` contains the class library.
`Squeak1.1.changes` also contains the Smalltalk VM source, its primitive table and platform support code.
Both files use carriage-return line endings. A text editor must handle those endings when searching the source.

The relevant source methods are:

| Contract | Source methods |
| --- | --- |
| Image/object headers | `ObjectMemory class>>initializeObjectHeaderConstants`, `ObjectMemory>>sizeBitsOf:` |
| Compact classes and roots | `ObjectMemory>>fetchClassOf:`, `ObjectMemory class>>initializeSpecialObjectIndices` |
| Method headers | `Interpreter>>literalCountOfHeader:`, `argumentCountOf:`, `primitiveIndexOf:` |
| Bytecodes | `Interpreter class>>initializeBytecodeTable`, `Interpreter>>doubleExtendedDoAnythingBytecode`, `secondExtendedSendBytecode` |
| Primitive dispatch | `Interpreter class>>initializePrimitiveTable` |
| Float representation | `Interpreter>>floatObjectOf:`, `floatValueOf:`, and their `InterpreterSimulator` overrides |
| Hashes and dictionaries | `ObjectMemory>>hashBitsOf:`, `Interpreter>>lookupMethodInDictionary:` |
| Snapshot continuation | `Interpreter>>primitiveSnapshot`, `SystemDictionary>>snapshot:andQuit:` |
| Guest method reflection | `CompiledMethod>>initialPC`, `numLiterals`, `numArgs`, `numTemps`, `header` |

## VM differences

| Area | Xerox Version 2 runtime | Squeak 1.1 requirement |
| --- | --- | --- |
| SmallInteger | Signed 15-bit payload | Signed 31-bit payload |
| Indexed words | 16 bits | 32 bits |
| Float | Binary32 | Binary64, preserved without narrowing |
| Method literals | Six-bit count | Eight-bit count |
| Method metadata | Flags and optional extension literal | One 31-bit header payload |
| Primitive number | Eight bits | Nine bits, including quick returns |
| Initial method IP | `2 * (literalCount + 1) + 1` | `4 * (literalCount + 1) + 1` |
| Bytecode 132 | Double-extended send | Eight operation modes, including sends and field accesses |
| Bytecode 134 | Double-extended super send | Second extended send with a six-bit selector index |
| Identity hash | Derived from imported oop | Independent 12-bit hash in the object header |
| Roots | Fixed low oops | `specialObjectsArray` referenced by the image header |
| Bitmap | Monochrome, 16-bit words | Packed 32-bit words and Form depth |
| Primitive 75 | Identity-number conversion | Stored identity hash |
| Primitive 128 | Alto disk transfer | Array identity exchange |
| Primitive 135 | Snapshot target registration | Millisecond clock |

Most basic bytecodes and context field indices remain familiar.
The changed headers and primitive contracts still prevent direct use of the Xerox interpreter.
Matching primitive numbers are not sufficient evidence of matching semantics.

## Converted object ABI

The Squeak converter is separate from the Xerox converter.
It maps a stored source oop to a scanned OBJEKT identity of `oop >> 2`.
It maps each SmallInteger to compact code 2 with a sign-extended 32-bit payload.
The allocator floor is one greater than the largest imported identity.

Each stored body has two metadata components:

1. `(guest_byte_length << 2) | kind`, with kinds pointer=0, word=1, byte=2, method=3.
2. The saved 12-bit identity hash, plus the original object format in bits 12–15.

Guest fields start at component 3. Each guest pointer, 32-bit word or byte uses one machine word.
Compiled methods contain their tagged header, tagged literals, and separate raw bytecode/trailer bytes.
The original guest IP remains a byte offset through the 32-bit header/literal prefix.

Both binary64 words remain raw and unchanged. No Rust floating-point operation participates in conversion.
Object class links remain OBJEKT class metadata. OBJEKT and its collector need no Squeak-specific fields or tests.

Boot metadata reserves root 0 for `specialObjectsArray`, root 1 for the saved context, and root 2 for the raw hash-generator state.
Compact code 2 maps to the SmallInteger class from `specialObjectsArray`.
The guest nil, false and true objects remain distinct stored objects.

Conversion independently decodes every emitted record and compares it with the source object.
It also compares every reference visible to the generic collector.
Opaque words, Float payloads and bytecode bytes cannot introduce collector edges.

## Implementation stages

### 1. Image reader and conversion — implemented

The reader handles both image byte orders, three allocated header types, free chunks, compact classes and cyclic references.
Tests cover invented graphs, malformed files and the pinned image.
The audit resolves class/selector bindings and the saved process without executing it.

### 2. Squeak interpreter and core primitives — execution milestone implemented

The [Squeak microcode](../microcode/squeak/) implements method-header decoding, bytecodes 132/134,
quick returns, sends, returns, blocks, dynamic sends and `doesNotUnderstand:`.
Method dictionaries use stored hashes. New objects receive hashes from the archived VM's generator.
The image's special-object array supplies boot constants and mutable device registrations.

Implemented primitive families are:

| Primitive codes | Operations |
| --- | --- |
| 1–18 | Signed 31-bit arithmetic, comparison, division, shifts and Point creation |
| 40–54 | Binary64 arithmetic, comparison, fraction, exponent, scale and integer conversion |
| 60–69, 73–74 | Indexed access, size, Characters, method literals and instance fields |
| 70–71, 79 | Instance and compiled-method allocation |
| 75 | Stored identity hash |
| 81–84 | Block evaluation and dynamic sends |
| 85–89 | Semaphores, process queues, deferred switches and cache flush |
| 90, 101–102, 106–109 | Mouse, cursor, display, screen extent, buttons and keyboard |
| 93, 124–125, 133–137 | Input, low-space and timer registration; clocks and interrupt key |
| 96, 105, 145 | Packed-colour BitBlt, bulk replacement and constant fill |
| 103 | Character scanning, font metrics, stop conditions and glyph drawing |
| 110–112, 129–131 | Identity, class, free capacity, special-object array and explicit collection |
| 121–122, 142 | Image name, display configuration and VM path |
| 150–155, 157–158, 161 | File handles, positions, read/write, size and directory separator |
| 256–511 | Quick-return method codes |

Special bytecode 200 implements block creation. Primitive 80 itself fails, as the archived table specifies.
Unsigned 32-bit word results allocate LargePositiveIntegers when necessary.
Methods retain separate tagged headers/literals and opaque bytes.
Changing a method header through `objectAt:put:` is not supported; this operation fails before mutation.
The header/literal prefix has no raw byte view in this representation.

Native tests exercise both microinstruction engines. RTL tests compare every mutator retirement and object state through forced collection.
The pinned-image tests preserve the initial bytecode prefix, restore the desktop, open a menu and dismiss it.
The dismissal check compares the complete packed framebuffer with its previous contents.
These are project regression checkpoints, not traces captured from the original VM.

### 3. Binary64 NUMERIK support — implemented

`precision=Binary64` uses even register pairs for operands and PRODUCT for the full result.
The assembler, Rust processor model and synthesizable HardFloat backend share this generic contract.
See [NUMERIK floating point](numerik-floating-point.md) for encodings and retirement rules.
Squeak microcode owns Float allocation, word order and primitive success or fallback.

RTL differential tests cover all operations and rounding modes, edge cases and randomized operands against SoftFloat.
Processor tests cover response stalls, reset cancellation and collection after a binary64 operation.
Float primitives also run through both native microinstruction engines.

### 4. Colour graphics and input — desktop milestone implemented

Microcode handles packed 32-bit bitmap words at depths 1, 2, 4, 8, 16 and 32.
BitBlt implements Boolean rules 0–15 and transparent paint/mask rules 25–26.
It clips coordinates, applies indexed colour maps and halftones, and preserves overlapping source data.
Aligned Boolean copies and fills process whole words.
Mapped 1-bit glyph copies to 8-bit destinations cache both colours and merge pixels before each destination word write.
Other mapped pixels use the general pixel loop.
Only changed words are written. Registered display updates publish through the external scanout device.
The host converts published pixels for presentation; it does not read Forms or perform BitBlt.

Primitive 103 scans character runs in microcode and calls the shared BitBlt path for each glyph.
It supports measurement without drawing, character stops, right-margin stops and end-of-run results.
The scanner validates run metrics and raster operands before it draws the first glyph.
Unsupported inputs retain the complete Smalltalk fallback.
The accelerator requires a String, Array tables, SmallInteger coordinates, and identical argument and receiver stop tables.
This last condition preserves the archived fallback's distinction between the table it tests and the table it returns from.

Mouse, buttons, keyboard modifiers, clocks and semaphore delivery execute through the same microcode boundary path.
Timer expiry clears its registration and signals once. A full keyboard ring does not block timer delivery.
Allocation primitives preserve the registered low-space reserve and fail into guest code after collection cannot satisfy it.

Graphics support is not complete. Extended raster rules 18–24, WarpBlt and byte-backed bitmaps remain unavailable.
RGB depth conversion and RGB colour maps fail before drawing; same-depth RGB copying is supported.
The archived rule 15 leaves the destination unchanged.

### 5. Workstation services — partial

Microcode opens opaque external handles, validates guest buffers, and transfers file bytes through registers.
Byte and word buffers retain the archived little-endian transfer convention, including partial final words.
Source files can be read from the mounted volume. Image-name changes update an external property only.
The [file device](devices.md#external-file-volume) neither walks the heap nor constructs guest values.

Remaining work includes snapshot writing/reloading, persistent storage, directory enumeration and mutation,
array identity exchange, object enumeration, sound, and the remaining graphics accelerators.
Unimplemented required primitives stop explicitly; these facilities are not silently emulated by host VM code.
A full Squeak desktop run on RTL or an FPGA board remains an integration gate.

The host may provide disk, memory and I/O.
Allocation, GC, dispatch, scheduling and language primitives must execute through machine hardware and microcode.
The standalone emulator models those generic machine operations, including binary64 arithmetic.
