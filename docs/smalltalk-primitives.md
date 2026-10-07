# Smalltalk image primitive inventory

This inventory covers the pinned Xerox Version 2 image, SHA-256
`cac3a2d9690e8353d9ccfd073b1199bd49b43b5989607032a06a185cd4f23a1c`.
It contains 215 methods with extended headers, across 106 primitive-number values.
Of those methods, 71 declare primitive zero: they execute bytecodes without attempting a primitive.

Regenerate the method-level inventory with:

```sh
cargo run --locked -p rekursiv-smalltalk -- audit \
    artifacts/st80/VirtualImage artifacts/st80/primitive-audit.json
```

The report includes each method's source oop, argument count, bytecode length, and class/selector bindings.
Bindings come from the image's dictionaries, including classes without stored instances and the compact SmallInteger class.
Every extended-header method in this image has a dictionary binding.
The importer tests check inventory counts and representative bindings against the pinned image.
This table inventories declarations. The bounded startup test below supplies separate execution evidence.
Neither check proves that every fallback works.

| Primitive | Methods | Operation and current execution path |
| --- | ---: | --- |
| 0 | 71 | Extended header without a primitive; execute guest bytecodes |
| 1–18 | 21 | SmallInteger arithmetic, comparison, division, bit operations, and Point construction; microcode |
| 21–37 | 17 | LargePositiveInteger arithmetic, comparison, and bit operations; guest bytecode fallback; original methods 21–37 exercised on RTL |
| 40–54 | 15 | Float conversion, arithmetic, comparison, truncation, fraction, exponent, and scaling; microcode plus generic NUMERIK binary32 hardware |
| 60–64 | 19 | Indexed access, size, and Character conversion; microcode, with the CompiledMethod access contract below |
| 65–67 | 4 | Stream next, nextPut:, and atEnd; microcode for Array/String streams |
| 68–69 | 2 | CompiledMethod header/literal access; microcode |
| 70–74 | 8 | Allocation, become:, and instance-field access; microcode with generic OBJEKT allocation/exchange |
| 75–78 | 7 | Identity numbers, inverse conversion, and instance enumeration; microcode with generic OBJEKT directory operations |
| 79 | 1 | CompiledMethod allocation; microcode validates the receiver's byte-instance specification and preserves arguments on failure |
| 80–82 | 6 | Block copy and activation; microcode |
| 83–84 | 5 | Dynamic perform:; microcode |
| 85–89 | 5 | Semaphore/process operations and cache flush; microcode includes idle waiting and asynchronous wakeup |
| 90 | 2 | Mouse position; microcode constructs a Point from coherent device coordinates |
| 91 | 2 | Set cursor position; microcode validates the Point before staged device publication |
| 92 | 1 | Link cursor to mouse; microcode validates the Boolean and writes the link register |
| 93 | 1 | Register, replace, or clear the input semaphore; microcode with an explicit GC root and asynchronous delivery |
| 94 | 1 | Input sampling interval; microcode validates configuration, and the external pointer device samples changed positions |
| 95 | 1 | Read an input event word; microcode converts raw packets, buffers words, signals per word, and constructs guest integers |
| 96 | 2 | BitBlt copyBits/copyBitsAgain; microcode with clipping, halftones, overlap handling, and device refresh |
| 97 | 1 | Snapshot; machine preparation and device transfer required in stage 5 |
| 98 | 1 | Seconds clock into a byte object; microcode converts the epoch and packs four little-endian bytes |
| 99 | 1 | Millisecond clock into a byte object; microcode packs four little-endian bytes |
| 100 | 1 | Signal a semaphore at a millisecond deadline; microcode validates, expands the deadline, and registers a rooted one-shot recipient |
| 101 | 1 | Install a cursor bitmap; microcode validates exact device dimensions, packs rows, publishes, and roots the Form |
| 102 | 1 | Install a display bitmap; microcode validates device limits, packs rows, publishes, and roots the Form |
| 103 | 1 | Character scanning/drawing accelerator; original guest fallback measures text on RTL; native image startup also draws through BitBlt |
| 104 | 1 | BitBlt line-drawing accelerator; guest fallback uses copyBits; original line-drawing path not yet qualified |
| 105 | 5 | ByteArray/String range replacement; four original replacement methods exercised through guest fallback |
| 110–111 | 4 | Identity equality and class lookup; microcode |
| 112 | 1 | Report currently allocatable 40-bit RAM words; generic OBJEKT counter plus guest integer construction |
| 113 | 1 | Quit; materialize the context and halt with runtime status 10 |
| 114 | 1 | Enter debugger; materialize the context and take resumable service break 11 |
| 115 | 1 | Report remaining identities across the 37-bit space; generic OBJEKT counter plus guest integer construction |
| 116 | 1 | Low-space registration and one-shot delivery; microcode compares copied thresholds with OBJEKT counters |
| 128 | 1 | Missing-storage fallback exercised through the original guest method; successful transfers require stage 5 storage integration |
| 135 | 1 | AltoFile snapshot target; microcode validates and copies serial bytes and the unsigned 16-bit virtual leader address into a rooted registration |

Primitive numbers outside this table do not occur in the selected image.
Methods named `tryPrimitive...` account for some duplicate declarations.
The inventory includes block activation with zero through three arguments and `perform:` variants with one through four.
`Object>>tryPrimitive3:with:with:` declares primitive 7 with three arguments; integer equality accepts one.
A directed RTL test confirms fallback preserves all three arguments, including with a valid SmallInteger receiver.
`Object>>tryPrimitive4:with:with:with:` declares optional replacement primitive 105 with four arguments.
A primitive miss retains the original receiver and arguments and activates the guest fallback body.
Device and system misses currently follow that path too; several fallbacks report failure, so this does not provide a usable device implementation.

## Execution policies and evidence

Capacity results are snapshots before allocation of any LargePositiveInteger needed to return the count.
Word capacity excludes collector reserve and unreclaimed ranges. Identity capacity includes reservations consumed by failed transfers.
Quit leaves machine state available for inspection. Debugger resume restores the caller caches and continues after the completed send.
Neither operation invokes a host language handler.

Bitmap registration publishes an initial frame. BitBlt uploads changed pixel groups after drawing.
Direct bitmap writes become visible when an upload covers those words, or when the guest registers the Form again.

CompiledMethod byte indexing addresses only bytecodes and the source trailer. The header/literal prefix has no byte view.
The [representation contract](smalltalk-image.md#physical-bodies-and-indexing) defines this port policy and its Version 2 source evidence.
Headers are immutable after allocation. Primitive 68 reads headers and full-width literals. Primitive 69 writes only literals.
Directed tests cover 37-bit references, both index ranges, immutable headers, and unchanged method contents and arguments after rejected writes.
The original `needsStack:encoder:` also executes on RTL, replacing the method through `become:` while preserving a 37-bit literal and all code/trailer bytes.
NewMethod requires a pointer receiver with an integer instance specification for indexable bytes and no fixed fields.
Compatible subclasses retain their own class identity in the allocated method. Invalid receivers and arguments enter guest fallback before allocation.

The scheduler handles synchronous signal, wait, resume, and suspend, plus asynchronous input and timer notification delivery.
It waits in microcode when no process is runnable and performs event-driven switches at completed bytecode boundaries.
Registered semaphores remain collector roots. Devices report counted notifications without access to guest queues or contexts.
Low-space registration also signals through this scheduler, with no external device event or host heap scan.
Storage registration/delivery integration remains stage 5 work.
Without storage, the original primitive 128 fallback sets the file's error field to -1 and returns the receiver.
Its RTL test checks preserved arguments, unchanged page bytes, an untouched semaphore, and no device requests or completion acknowledgements.

The original LargeInteger methods 21–37 and all four String/ByteArray replacement methods now execute through guest fallback in RTL tests.
Those tests keep the original image objects unchanged, force collection, and delay memory/store responses.
They establish working paths for selected inputs, not exhaustive arithmetic coverage.
The original CharacterScanner fallback passes measurement tests for end-of-run, right-edge crossing, character stops, and an empty interval.
These tests also check destination position, glyph width, source position, and final string index, with collection and delayed transfers.
The display branch now reaches the implemented BitBlt primitive during native startup. Primitive-failure operand preservation has separate directed tests.
Original Cursor and DisplayScreen registration methods also execute through RTL, with every displayed pixel checked against the original bitmap.
The original AltoFile snapshot-target method also executes through RTL and retains copied metadata without issuing disk requests.
The saved-context startup test reaches the first BitBlt call after 2,176 bytecode boundaries and three collections.
It loads the original context at oop `0x2b28`, all fixed roots, and the unchanged converted image.
Before BitBlt, startup calls Semaphore signal at boundary 22 and display registration at boundaries 154 and 750.
Both registrations publish successfully. The second display measures 640 by 480 pixels.
All 499 bytecodes in the pinned Xerox `trace2` match the beginning of the RTL trace, in order.
This compares bytecode execution, not the original collector's memory-access sequence or timing.
The trace reaches primitive 96 at boundary 2,176, before executing that primitive.
No storage transfer occurs in this prefix. A separate native regression continues through 32 successful BitBlts, with no BitBlt fallback.
It reaches 9,870 bytecode boundaries, 24 collections, and 33 display publications; one copy clips to an empty rectangle.

Reproduce the trace with:

```sh
REKURSIV_ST80_DIR="$PWD/artifacts/st80" \
    cargo test --release --locked -p rekursiv-smalltalk --test startup -- --ignored --nocapture
```

The test uses 131,072 RAM words, with a 65,536-word active semispace, plus external backing records.
Memory and device requests have delayed responses. The host service count cannot change after boot.
An observer stops the test at a stage 5 primitive boundary without supplying a guest result or changing guest execution.

The [device contract](devices.md) records the generic LOGIK transport, implemented peripherals, and specified storage registers.
Input, clock, timer, and bitmap registration paths are implemented. Snapshot-target registration is implemented; storage transfer primitive 128 remains unfinished.
The native emulator now supplies interactive presentation, keyboard, and mouse input.
BitBlt and drawing refresh now execute in microcode. Snapshots and complete interactive image qualification remain stage 5 work.

## BitBlt

Primitive 96 lives in `microcode/smalltalk/bitblt.uc`. It takes no arguments and returns its receiver.
The receiver's first fourteen fields follow the image's BitBlt layout:

```
destForm sourceForm halftoneForm combinationRule
destX destY width height sourceX sourceY clipX clipY clipWidth clipHeight
```

Each Form supplies bits, width, height, and offset. BitBlt uses explicit coordinates; guest methods apply Form offsets.
Words contain sixteen pixels, most significant bit first. Rows have `ceil(width/16)` words, including padding.
All non-nil Forms need pointer format, nonnegative SmallInteger dimensions, enough word storage, and raw 16-bit bitmap words.
Numeric BitBlt fields must be signed 15-bit SmallIntegers. Other numeric representations enter the original guest fallback.
Microcode checks Form metadata and storage bounds before clipping. It then validates all words the copy reads before any destination write.
Words outside that accessed region are not inspected. Malformed data inside the region causes primitive failure without partial drawing.

| Rule | Result | Rule | Result |
| ---: | --- | ---: | --- |
| 0 | zero | 8 | NOT (source OR destination) |
| 1 | source AND destination | 9 | NOT (source XOR destination) |
| 2 | source AND NOT destination | 10 | NOT destination |
| 3 | source | 11 | source OR NOT destination |
| 4 | NOT source AND destination | 12 | NOT source |
| 5 | destination | 13 | NOT source OR destination |
| 6 | source XOR destination | 14 | NOT (source AND destination) |
| 7 | source OR destination | 15 | all ones |

A nil source supplies all ones. A non-nil halftone must be 16 by 16 pixels.
It tiles at absolute destination coordinates and masks the source before applying the Boolean rule.
Rules outside 0–15 fail the primitive; the image implements its additional paint rule in Smalltalk.

Clipping intersects the requested rectangle, destination bounds, clip rectangle, and any source bounds.
It advances source and destination together. Empty rectangles succeed without allocation, writes, or publication.
Masks preserve pixels outside the final rectangle and unused padding bits.

Disjoint source and destination bitmaps copy directly after validation. They need no temporary object.
If source or halftone storage aliases the destination, microcode snapshots aligned source words before merging.
This preserves source pixels for horizontal or vertical overlap, including distinct Forms that share bitmap storage.
The ESTK frame and value registers root references across collection and paging.
No guest process switch occurs between validation and drawing.
Scratch storage costs one descriptor plus `clipped destination words per row * clipped height` object-memory words.
Only aliased storage takes this allocation path. Directional copying can reduce that remaining allocation cost.

After a nonempty copy, microcode checks rooted cursor/display registrations for matching bitmap identities.
Matching geometry uses an atomic patch of changed 32-bit pixel groups. Edge groups include their unchanged neighboring pixels.
A different alias geometry or changed visible geometry uses a fully validated replacement frame.
No host callback draws pixels or interprets Forms.
Malformed registrations discovered after drawing stop the runtime instead of invoking fallback and repeating a completed drawing operation.
The device stages only supplied replacement words until publication. It does not clone the complete frame for each patch.
Registration still validates and uploads the complete bitmap. Direct writes outside the patched groups do not become visible automatically.

Native and RTL tests compare against a pixel-level oracle, with forced collection and delayed RTL transactions.
The original-image native test also exercises guest text drawing through this primitive.
Storage transfers, snapshot saving, and complete desktop interaction remain separate work.

[Dan Ingalls's November 1975 Bit BLT memo](https://www.bitsavers.org/pdf/xerox/alto/BitBLT_Nov1975.pdf)
separates clipping from the transfer routine and chooses a transfer direction for overlap.
That approach is a reference for replacing our remaining alias snapshots with directional copying.
Our current row-base caching and separation of validation from drawing keep setup work outside the word operation where possible.
The PDF also contains Diana Merry's April 1976 `BBSCAN.SR` assembly listing.
That character scanner calls the `BITBLT` instruction; it does not contain the raster kernel.
Its reuse of font setup across characters is relevant to future primitive 103 work.

## Hardware boundary

The machine owns bytecodes, primitives, arithmetic, scheduling, bitmap algorithms, and collection.
External models may supply RAM, storage, clocks, input events, and framebuffer presentation.
They must not dispatch primitive numbers, inspect Smalltalk classes, run guest fallbacks, or signal semaphores by editing guest objects.
Language adapters and object layouts stay in microcode. Generic hardware exposes transport, numeric operations, and object-memory mechanisms.
