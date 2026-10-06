# Smalltalk-80 image conversion contract

`rekursiv-smalltalk` converts a Xerox Smalltalk-80 image into ordinary 40-bit OBJEKT records.
It is an offline tool. It contains no bytecode interpreter, primitive implementations, scheduler,
or collector. RTL and microcode must supply those operations in subsequent stages.

## Selected distribution

The target is the Xerox Smalltalk-80 Virtual Image Version 2 distribution of 1983.
[Mario Wolczko's archive](http://www.wolczko.com/st80/) preserves the image, sources, diagnostic
listings, traces, and its accompanying manual. The fetch tool uses the matching
[Internet Archive copy](https://archive.org/details/smalltalk-80).
Both archive copies were checked and have the same SHA-256.

The [fixture manifest](../crates/rekursiv-smalltalk/fixtures/xerox-v2.json) pins the archive and
each downloaded file by checksum. The `VirtualImage` member is 596,128 bytes.
Its SHA-256 is:

```text
cac3a2d9690e8353d9ccfd073b1199bd49b43b5989607032a06a185cd4f23a1c
```

The distribution retains Xerox copyright notices. The repository contains the manifest and
invented test fixtures. The fetch command obtains the original files separately.

Fetch, convert, and check the complete image:

```sh
scripts/check-smalltalk-image.sh
```

This command requires Python 3 and network access on the first run.
It verifies cached files on subsequent runs. It writes the converted bundle to
`artifacts/st80/rekursiv-image.json`.

The individual commands are:

```sh
python3 scripts/fetch-smalltalk-image.py
cargo run --locked -p rekursiv-smalltalk -- inspect artifacts/st80/VirtualImage
cargo run --locked -p rekursiv-smalltalk -- import \
    artifacts/st80/VirtualImage artifacts/st80/rekursiv-image.json
cargo test --locked -p rekursiv-smalltalk
REKURSIV_ST80_DIR="$PWD/artifacts/st80" \
    cargo test --locked -p rekursiv-smalltalk --test import -- --ignored
```

`inspect` validates the supported interchange format without requiring the distribution checksum.
`import` requires the exact pinned image. An internal-format, little-endian, Stretch, or LOOM
image is not an alternative input profile. Unsupported headers, layouts, and corrupt graphs produce errors.

## Source format and specification

The source format comes from Part 1, printed pages 2–3, of the
[Version 2 manual](http://www.wolczko.com/st80/manual.pdf.gz).
The [Blue Book, chapter 27](https://docs.huihoo.com/smalltalk/esug/HistoricalDocuments/Smalltalk80/BlueBookImplementation/bluebook_chapter27.html)
defines the guest layouts and method headers.

- The file uses big-endian 16-bit words and 32-bit section lengths measured in words.
- A 512-byte header precedes object space. Object space ends with zero padding to a 512-byte boundary.
- The object table follows, with two words per entry. An even oop selects its entry by word index.
- The low four flag bits and the location word form a contiguous 20-bit word address.
- Flags `0x20`, `0x40`, and `0x80` mean free entry, pointer contents, and odd byte length.
- Object size includes the size and class words. The unused byte of an odd-length body is not guest content.
- Odd oops encode signed 15-bit SmallIntegers. Oop zero is invalid.

The parser checks section lengths, flags, padding, extents, class formats, method literals, and all reference targets.
Allocated objects must cover object space exactly, without overlap or gaps.
Historical reference counts do not determine which objects the importer keeps.
All allocated records survive conversion, including objects not listed in the diagnostic files.

The manual supplements and corrects the book. These distinctions affect this port:

| Evidence | Consequence |
| --- | --- |
| Part 1, pp. 2–3: interchange storage is contiguous across segment boundaries | Treat the segment bits as address bits, without gaps or per-segment allocator metadata |
| Part 1, p. 4: compiled methods end with source-file information | Preserve every byte after the literal frame, including the source trailer |
| Part 1, p. 7: fixed interpreter objects | Preserve the low-oop objects, including the SystemDictionary association at oop 18 |
| Part 4, PDF page 37: trace implementation differs from chapter 30 | Compare guest execution semantics, not the old reference-count collector's memory-access sequence |
| Part 4 correction index, PDF page 38: book p. 578 names the wrong class | Method headers and literals belong to CompiledMethod, not MethodContext |
| Part 4, PDF page 53: book p. 650 mouse-button correction | Device work must use the corrected left/right event codes, 130 and 128 |

The remaining interpreter and primitive corrections in Part 4 apply as those routines are implemented.
The importer does not reproduce chapter 30's allocator or reference-count collector.
The existing machine owns allocation and RAM collection.

## Guest values and identity

The definitions live in [layout.rs](../crates/rekursiv-smalltalk/src/layout.rs).

| Guest value | Machine representation |
| --- | --- |
| Stored object with even oop `p` | Scanned reference with identity `p >> 1` |
| SmallInteger | Compact code 2 with the sign-extended value in its 32-bit payload |
| nil, false, true | Stored objects from oops 2, 4, and 6, with their original distinct classes |
| Guest byte or unsigned 16-bit word | Raw machine word, with all unused bits zero |

Only compact code 2 has a guest class mapping: SmallInteger, originally oop 12.
Machine compact nil, Boolean, and unsigned values are not guest Smalltalk values.
Guest SmallInteger limits remain -16384 through 16383. Wider hardware arithmetic does not change
overflow behavior or primitive fallback.

The original dictionary hash bits are `oop >> 1`, exactly the imported machine identity.
Conversion preserves dictionary slots and selector identities without rehashing.
Physical relocation cannot change this value. For a new identity, runtime hash lookup must use its
low 15 bits. This permits collisions without limiting the machine identity space.
The legacy `asOop` representation interprets those 15 bits as a signed SmallInteger payload.
Reversible `asOop`/`asObject` behavior for identities beyond the original oop range remains a stage 4
runtime decision. It must not truncate a machine identity and silently alias another object.

The output reserves the original 15-bit identity namespace and requires `next_identity = 32768`.
The later halted loader must establish that allocator floor before guest allocation.
This metadata is not permission for a host callback to allocate identities during execution.

New scanned machine fields initially contain machine nil. Runtime allocation routines must replace
guest pointer fields with the stored guest nil before publishing an object to Smalltalk code.
Partially initialized objects and temporary references must remain in machine-visible roots.

## Physical bodies and indexing

Every imported object uses a scanned reference, including guest byte and word objects.
The collector follows tagged references and ignores raw scalar words in those bodies.
It needs no Smalltalk class test or method-header decoder.
Using one scan tag also keeps object format out of identity when later implementing `become:`.

Every body begins with one raw descriptor at OBJEKT component 1:

```text
descriptor = (guest_byte_length << 2) | kind
kind: 0 = pointers, 1 = words, 2 = bytes, 3 = compiled method
```

The length excludes the source object's two-word size/class header.
OBJEKT class metadata holds the converted class reference.
The physical body length includes the descriptor and is independent of the guest length.

| Kind | Components after the descriptor |
| --- | --- |
| Pointers | One converted guest oop per component |
| Words | One raw 16-bit value per component, preserving bit patterns such as Float words |
| Bytes | One raw byte per component, including an odd final byte |
| Compiled method | Converted header SmallInteger, converted literals, then one raw byte per remaining byte |

Guest pointer and word indices are zero-based: index `i` maps to physical component `i + 2`.
For byte objects, byte index `b` maps to component `b + 2`.
Word objects retain high-byte-first guest ordering for byte access.
No floating-point conversion occurs during import.

Let `P` be one plus a compiled method's literal count.
Its first `P` guest words occupy physical components 2 through `P + 1`.
A byte offset `b >= 2*P` maps to component `2 + P + (b - 2*P)`.
The header and literal prefix retain their guest word interpretation.
Guest context instruction pointers are one-based byte offsets from the start of the method contents.
Their numeric values remain unchanged by conversion.
The initial method IP formula is `2*P + 1`, independent of the target body size.

Class, dictionary, and context field indices remain the Blue Book indices in `layout.rs`.
The extra physical descriptor never becomes an extra guest instance variable.
Source oops in the JSON records are diagnostic metadata, not a required host lookup table.

## Roots and boot metadata

Root slots 0–25 hold the fixed even oops 2 through 52 from the manual's interpreter-object table.
This includes the Symbol table and SystemDictionary association, as well as constants and classes.
Slot 26 holds the context reached through SchedulerAssociation, activeProcess, and suspendedContext.
Slots 27–31 are initially raw zero and are available for runtime state.
The converter emits all 32 slots explicitly.

Future interpreter microcode must keep its active context root current.
Receivers, methods, and temporary object references must also remain in tagged registers, scanned
context fields, or the machine's other documented root locations across allocating operations.
Rust-local variables are never machine roots.

The original saved context is oop `0x2b28`, its method is `0x6b64`, and its guest IP is 144.
Reading that position yields bytecode 131, matching the first instruction in Xerox's `trace2`.
This check reads the saved state. It does not execute the image or prove interpreter correctness.

## Primitive conventions

Method headers retain the Blue Book flag field and six-bit literal count.
Flags 5 and 6 encode quick-return methods. Flag 7 selects the extended header in the second-last literal.
The primitive number is bits 1–8 of that literal's original tagged 16-bit value.
An extended header with primitive number zero does not request a primitive.
The converter emits a complete number/count inventory, including these zero entries, for later implementation.

The image uses the standard arithmetic, collection, storage, control, I/O, and system primitive groups.
It also contains Alto-specific primitives 128 and 135.
The distribution's `Smalltalk-80.sources` identifies them as disk-page transfer and snapshot serial/leader setup.
Their argument decoding, guest state changes, success, and fallback belong in microcode.
Only the underlying disk or snapshot device transaction can execute on the host.
No primitive is implemented or emulated by this converter.

## Output and validation

The output is JSON format version 1. Machine words are unsigned JSON integers with at most 40 bits.
It contains the source checksum, allocator floor, compact-class mappings, roots, records, and primitive inventory.
Each record supplies a reference, class, COND value, and physical body words for the existing backing-store interface.
COND starts false. The output does not prescribe resident pager slots or physical RAM addresses.

The converter independently decodes each physical record back into its guest representation.
Every class link, field, method literal, and byte must match the input.
A separate comparison checks exactly which edges the generic tagged-word collector will see.
Raw bytecode data cannot create extra edges, even when adjacent bytes resemble a valid oop.

| Pinned image result | Count |
| --- | ---: |
| Allocated objects | 18,391 |
| Pointer objects | 7,607 |
| Word objects | 359 |
| Byte objects | 5,920 |
| Compiled methods | 4,505 |
| Converted physical body words | 341,467 |

The original diagnostic listings are slightly stale: two listed classes and six listed methods
have no allocated record in the image. The integration test pins these exact discrepancies.
It checks all 446 surviving class names and all 4,493 surviving listed method identities.
The importer preserves the other methods in the actual image as well.

Invented fixtures cover shared cycles, distinct singleton classes, odd byte lengths, raw word bits,
method literals, primitive extensions, and all 32,768 SmallInteger values.
Negative tests cover invalid flags, truncated files, bad extents, dangling references, unsupported
profiles, malformed methods, and nonzero padding.

Stage 1 ends at verified conversion. Loading the bundle into the simulator and executing guest
bytecodes belong to stage 2. A full image run also needs larger memory than the current test wrapper.
