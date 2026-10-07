# Workstation device interface

LOGIK implements the generic device transport below. Smalltalk microcode registers input/timer semaphores, reads clocks, delivers counted notifications, and wakes idle processes.
The shared `rekursiv-devices` crate supplies register, event, clock, timer, pointer, input FIFO, and bitmap upload models.
Both executors use these models. The [native emulator](emulator.md) adds interactive presentation, keyboard, and mouse input.
Storage peripherals remain unfinished.
Primitive declarations and current coverage are listed in [the image inventory](smalltalk-primitives.md).

The machine owns language execution, process queues, semaphore state, collection, and bitmap algorithms.
Devices supply clocks, input packets, storage sectors, and framebuffer presentation.
A device does not receive a Smalltalk primitive number, class, context, or object reference.
The same transport must support other language runtimes without changes to LOGIK, NUMERIK, or OBJEKT.

## LOGIK transport

Use a separate device channel with 32-bit byte addresses and 32-bit data.
Requests contain address, read/write direction, and write data. Addresses must be aligned to four bytes.
Request valid/ready and response valid/ready each complete on a rising clock edge.
The requester holds request fields until acceptance; the device holds its reply until consumption.
There is one outstanding request. A reply contains read data and an error bit.

A horizontal control word can issue one device read or write. It cannot also issue OBJEKT, floating-point, or recovery operations.
The address comes from NUMERIK register A; write data comes from the low 32 D-bus bits.
A successful reply updates a dedicated 32-bit device-result register, available on the D bus for the next instruction.
Other simultaneous destinations use their original operands. The chosen branch condition is captured before issue.

Device waits stall instruction retirement. Each request issues once, even under backpressure.
A failed reply reports a processor fault without retiring local effects.
A device must not report an error after committing a write or destructive read.
Reset cancels both channel endpoints together; reconnecting a live device requires draining the channel first.

The external IRQ input is level-sensitive. It means that the event-status register is nonzero.
IRQ does not jump directly into a language handler. Runtime microcode checks it at a bytecode boundary or while idle.

`rtl/logik_io.sv` captures the request and owns the result register.
Control bits 225:224 select None (0), Read (1), or Write (2); D-bus source Device (13) reads the last successful result.
The assembler uses `io=Read` or `io=Write`. Unaligned addresses fail before request issue.
Processor fault 6 reports a failed device reply. The previous device result survives that error and machine collection.

Transport tests cover delayed requests/replies, one-time retirement, IRQ changes while waiting, failed writes, reset cancellation, and result preservation across collection.
The simulation device supplies addressed words, counted events, clocks, a timer, pointer registers, raw input packets, and bitmap upload banks. Other workstation register regions remain planned.

## Register regions

These regions describe devices, not language objects. Event, clock, timer, pointer, input FIFO, and bitmap registers have simulation models. Undefined registers return an error.
All multiword transfers put the least significant word first unless a device explicitly specifies pixel ordering.

| Byte address | Device | Contract |
| --- | --- | --- |
| `0x0000–0x00ff` | Identification | Interface version and implemented-device capability bits |
| `0x0100–0x01ff` | Events | Pending-source mask and explicit acknowledgement bits |
| `0x0200–0x02ff` | Clocks and timer | Latched UTC seconds, monotonic milliseconds, deadline, arm/cancel |
| `0x0300–0x03ff` | Input | Mouse coordinates/buttons, input FIFO status/data, sample interval |
| `0x0400–0x04ff` | Cursor | Position, mouse-link flag, dimensions, bitmap upload offset/data, publish |
| `0x0500–0x05ff` | Display | Dimensions, stride, pixel format, upload offset/data, publish |
| `0x0600–0x06ff` | Block storage | Sector address, transfer length, direction, data, submit, completion |

### Identification

These identification registers define the boot contract. Their peripheral model and boot negotiation belong to stage 5 integration.
Current primitive tests configure their devices directly and do not exercise discovery.

| Address | Access | Value |
| --- | --- | --- |
| `0x000` | Read | Interface signature `0x524b494f` (`RKIO`) |
| `0x004` | Read | ABI version: major in bits 31:16, minor in bits 15:0; this contract is `0x00010001` |
| `0x008` | Read | Capability mask from the table below |

| Bit | Capability | Required regions |
| ---: | --- | --- |
| 0 | Counted events | `0x100`, `0x104` |
| 1 | UTC and monotonic clocks | `0x200`–`0x20c` |
| 2 | One-shot timer | `0x210`–`0x218`; also requires bits 0 and 1 |
| 3 | Pointer and cursor position | `0x300`, `0x304`, `0x30c`, `0x400`–`0x40c` |
| 4 | Raw input FIFO | `0x310`–`0x324`; also requires bit 0 |
| 5 | Cursor bitmap upload | `0x410`–`0x42c` |
| 6 | Display bitmap upload | `0x510`–`0x52c` |
| 7 | Block storage | `0x600`–`0x64c`; also requires bit 0 |

The signature, version, and capabilities remain fixed until device reset. Identification registers reject writes.
Boot code rejects an unknown signature or major version before configuring peripherals.
A higher minor version retains these register meanings. Unknown capability bits can be ignored.
An absent capability permits primitive fallback before guest state changes; it does not permit access to undefined registers.
Timed pointer sampling into the FIFO requires bits 1, 3, and 4 together.

### Events and clocks

Read `0x100` to obtain pending-source bits: raw input packets (0), timer (1), and storage (2).
Other bits are reserved and must be zero. Low-space delivery comes from direct OBJEKT counter reads, without an external event source.
Write a bit mask to `0x104` to consume one notification per selected source.
Each source has a counter. Its status bit remains set until every notification has been consumed.
Repeated notifications do not coalesce. An acknowledgement and arrival on the same cycle preserve the new notification.
The simulator schedules arrivals by device clock tick without access to guest state.
Actual input and storage devices must retain their data separately from notification counts.
A full FIFO applies backpressure where possible; input overrun is a visible status, never silent replacement of an unread packet.

Reading a clock-latch register captures one coherent 64-bit sample for subsequent low/high reads.
UTC uses seconds since 1970; monotonic time uses milliseconds since device reset.
Smalltalk microcode performs its epoch conversion and byte-object packing.
A timer deadline uses the same monotonic clock and raises one pending event when reached.
Cancel disarms the timer and clears its pending expiry; rearming starts a new deadline.

| Address | Access | Implemented clock/timer operation |
| --- | --- | --- |
| `0x200` | Read | Capture UTC seconds; return low word |
| `0x204` | Read | Return high word of captured UTC seconds |
| `0x208` | Read | Capture monotonic milliseconds; return low word |
| `0x20c` | Read | Return high word of captured monotonic milliseconds |
| `0x210` | Write | Stage low deadline word |
| `0x214` | Write | Stage high deadline word |
| `0x218` | Write | 0 cancels; 1 arms the staged absolute deadline; either command clears the previous pending expiry |

Low-word reads sample at request acceptance and retain that sample through reply delays.
Successful reply consumption publishes the high-word latch. Failed requests do not change latches or timer state.
The timer fires once when monotonic time reaches or exceeds its deadline, including an already-past deadline.
The simulation clocks advance by a configurable number of device cycles per millisecond, independently of guest execution.

Primitive 98 adds 2,177,452,800 seconds to UTC for the 1901 epoch. Primitive 99 uses monotonic milliseconds.
Both store the low 32 bits in the first four guest bytes, least significant byte first, and leave later bytes unchanged.
Primitive 100 expands a guest 32-bit deadline relative to a coherent monotonic sample.
It interprets the modular difference as signed: positive differences schedule ahead; zero or negative differences are immediately due.
The supported future interval is therefore at most 2,147,483,647 milliseconds. A positive difference across low-word rollover increments the device deadline's high word.
All buffer and semaphore checks precede timer changes. Nil cancels the timer without reading the deadline argument.

The pointer registers expose signed 32-bit coordinates. Mouse X reads latch both coordinates at request acceptance.
Successful reply consumption publishes the Y latch. Later mouse movement does not change that captured pair.
Cursor writes stage both coordinates, then publish them together. Failed writes leave the staged or visible state unchanged.

| Address | Access | Implemented pointer operation |
| --- | --- | --- |
| `0x300` | Read | Capture mouse coordinates; return X |
| `0x304` | Read | Return Y from the captured pair |
| `0x30c` | Write | Set the sample interval in milliseconds |
| `0x400` | Write | Stage cursor X |
| `0x404` | Write | Stage cursor Y |
| `0x408` | Write | 1 publishes the staged cursor position |
| `0x40c` | Write | 0 unlinks cursor and mouse; 1 links them and moves the cursor to the mouse |

When linked, physical mouse movement also moves the cursor; publishing a cursor position also moves the mouse.
Primitive 90 constructs a guest Point from the captured coordinates.
Primitive 91 validates a Point and its two SmallInteger fields before it issues cursor writes.
Both primitives require coordinates from -16384 through 16383, the current guest SmallInteger range. Other coordinates enter guest fallback.
Primitive 92 accepts only the guest true and false objects. Primitive 94 accepts a nonnegative SmallInteger interval.
Successful cursor, link, and interval primitives return their receiver.
The pointer device samples changed positions at the configured minimum interval, using the monotonic clock.
Zero permits a sample on every changed-position tick. Stationary positions do not generate repeated packets.
Movement between samples coalesces into the latest position; queued packets remain unchanged.
Sampling starts from the initial pointer position and emits its first packet after a change.

### Input FIFO and word conversion

| Address | Access | Input operation |
| --- | --- | --- |
| `0x310` | Read | Queued packet count in bits 15:0; sticky physical-overrun flag in bit 31 |
| `0x314` | Read | Head packet kind: 1 motion, 2 key down, 3 key up |
| `0x318` | Read | Head packet X or key code |
| `0x31c` | Read | Head packet Y; unused for key transitions |
| `0x320` | Read | Head packet timestamp in milliseconds |
| `0x324` | Write | 1 consumes the head packet at successful reply consumption |

The head remains stable across field reads and reply delays. Failed consume commands preserve it.
The workstation profile timestamps packets with milliseconds since UTC midnight.
The pointer adapter obtains this timestamp from the clock device, independently of instruction retirement.
Coordinates are signed 32-bit values. Key codes identify device keys; frontend mappings belong to interactive integration.
The Smalltalk runtime currently accepts key codes 0–4095 directly, including the image's modifier and mouse-button codes.
It clamps motion coordinates to 0–4095 for the image's twelve-bit coordinate fields.
Unknown kinds and unsupported key codes stop with runtime status 5.

A packet occupies one FIFO entry and raises one input notification.
Microcode produces an absolute-time marker `0x5000`, timestamp high word, and timestamp low word for every packet.
Motion adds `0x1000 | X` and `0x2000 | Y`. Key transitions add `0x3000 | code` or `0x4000 | code`.
Thus, motion produces five guest words; a key transition produces four.
Only microcode defines this encoding. The external device contains no guest words or references.

Root 29 retains a lazily allocated private Array with sixteen word slots.
Each word uses two canonical SmallInteger fields, containing its low fourteen bits and high two bits.
The Array also stores the read index, occupied count, unsignalled count, reserved storage semaphore, registered cursor/display Forms, and snapshot target metadata.
Microcode publishes all words before it consumes and acknowledges their raw packet.
It then signals the registered guest input semaphore once per word, using the ordinary scheduler.
The pending count survives collection and process switches, including preemption during delivery.
When no semaphore is registered, unsignalled words remain pending for later registration.
Polling removes those words without producing stale signals after registration.

Primitive 95 removes and returns one word. Values above 16383 become guest LargePositiveIntegers.
An empty buffer or wrong argument count enters guest fallback without consuming a word.
The runtime reserves five free slots before accepting another packet, even for four-word key packets.
A full runtime buffer leaves packets in the device FIFO while guest execution and other event sources continue.
The external FIFO has a configurable capacity, with 32 packets in the default simulation profile.
Physical overflow preserves existing packets and sets a sticky flag; microcode stops with status 5 instead of hiding data loss.


### Cursor and display bitmaps

Cursor and display uploads use raw pixel words with explicit dimensions and stride.
The format is one bit per pixel, most significant pixel first within each 32-bit word.
The cursor bank starts at `0x410`; the display bank starts at `0x510`.

| Bank offset | Access | Bitmap operation |
| --- | --- | --- |
| `+0x00` | Read | Maximum width; zero means the device is absent |
| `+0x04` | Read | Maximum height; zero means the device is absent |
| `+0x08` | Read / Write | Read visible width (zero if absent) / stage width |
| `+0x0c` | Read / Write | Read visible height (zero if absent) / stage height |
| `+0x10` | Read / Write | Read visible stride (zero if absent) / stage stride in 32-bit words |
| `+0x14` | Write | 0 begins a full replacement; 2 begins a sparse patch; 1 publishes |
| `+0x18` | Write | Set upload offset in words |
| `+0x1c` | Write | Store one pixel word and increment the upload offset |

Begin captures valid geometry and replaces any previous pending upload. Geometry changes after Begin affect the next upload.
A full replacement requires every word, including row padding, before Publish.
A sparse patch requires an existing visible frame with identical width, height, and stride.
It stages replacement words at explicit offsets. Unwritten words retain their visible values, and repeated offsets use the last write.
A patch can contain zero writes. Successful Publish applies all staged writes atomically.
Rejected requests preserve the pending upload. Failed publication preserves the visible frame and permits a later retry.
The peripheral stores only patch words during staging. It does not copy the complete visible frame for every patch.
Visible geometry reads and sparse patches are additions in device ABI version 1.1.
The device supplies storage and presentation for pixels. It does not inspect a guest Form or perform BitBlt.

Primitives 101 and 102 accept a pointer Form with bits, width, height, and offset fields.
Microcode checks positive SmallInteger dimensions, word-format bitmap storage, sufficient length, and each used word's raw 16-bit representation.
It accepts compatible Form subclasses. The offset field belongs to guest drawing policy and does not define a hardware hotspot.
Primitive 101 requires the device's exact cursor dimensions; the initial cursor profile is 16 by 16.
Primitive 102 accepts dimensions within both display limits. Missing devices enter guest fallback before allocation or writes.
All guest validation precedes allocation and device writes. Failure preserves the previous registration and visible frame.

Guest rows use `ceil(width/16)` words. Microcode packs them into `ceil(width/32)` device words and clears unused pixels.
After publication, root 29's private Array retains the Form in component 38 for the cursor or 39 for the display.
These references survive eviction and collection. A movable physical body address never becomes a display address.
Successful registration returns the receiver.

Registration publishes an initial snapshot. BitBlt checks whether the destination storage matches either registered Form's bits object.
For matching geometry, it patches only the 32-bit groups intersecting the clipped rectangle.
The packing loop preserves neighboring pixels and clears device row padding.
If the alias or visible geometry differs, microcode validates and uploads a complete replacement.
The shared upload routine retains no physical heap address.
Direct heap writes need an upload covering those words, or explicit registration, before presentation. The host does not watch the heap.
The native emulator presents published frames and supplies physical input.
Directed RTL tests cover drawing and cursor/display refresh. A complete interactive RTL session remains stage 5 work.

### Block storage contract

This table defines the storage interface. The peripheral model, machine driver, and guest transfer integration remain stage 5 work.
The device transfers contiguous byte ranges addressed by unit, 64-bit sector number, and byte offset within that sector.
The initial workstation profile uses 512-byte sectors and supports transfers of at least 4096 bytes.
Offsets and transfer lengths are multiples of four. A transfer can cross sector boundaries.
Device data words contain the first byte in bits 7:0, then successive bytes toward bits 31:24.

| Address | Access | Operation |
| --- | --- | --- |
| `0x600` | Read | Sector size in bytes, a power of two |
| `0x604` | Read | Number of units; valid unit indices start at zero |
| `0x608` | Read | Status: bit 0 busy, bit 1 completion available; other bits zero |
| `0x60c` | Write | Stage unit index; also select the unit for capacity reads |
| `0x610` | Write | Stage low 32 bits of the sector number |
| `0x614` | Write | Stage high 32 bits of the sector number |
| `0x618` | Write | Stage byte offset within the sector |
| `0x61c` | Write | Stage transfer length in bytes |
| `0x620` | Write | Stage an opaque 32-bit request identifier |
| `0x624` | Write | Command: 0 clear upload coverage, 1 submit read, 2 submit write, 3 submit flush |
| `0x628` | Read/write | Transfer-word cursor, starting at zero |
| `0x62c` | Read/write | Read a completion word or write an upload word; successful access advances the cursor |
| `0x630` | Read | Completion status code |
| `0x634` | Read | Completed request identifier |
| `0x638` | Read | Transferred byte count |
| `0x63c` | Write | 1 releases the completion and its read buffer |
| `0x640` | Read | Maximum transfer length in bytes |
| `0x644` | Read | Capture the selected unit's sector count; return low 32 bits |
| `0x648` | Read | Return high 32 bits of the captured sector count |
| `0x64c` | Read | Selected unit flags: bit 0 media present, bit 1 read-only; other bits zero |

There is one outstanding transfer or retained completion. A submit while either exists fails without changing the current transfer or staged data.
Submit captures the descriptor and, for a write, its complete payload at successful register-reply consumption.
Later staging changes cannot alter an accepted transfer. Submit returns after acceptance, before the storage operation completes.
Read/write lengths must be nonzero and no greater than the reported maximum. The offset must be less than the sector size.
Invalid commands, alignment, offsets, lengths, unit indices, or incomplete write uploads fail as transport errors before submission.
Each word of a write payload must have been uploaded since the last clear command. Clear also resets the cursor to zero.
The upload buffer is separate from read-completion data. A read, cursor reset, or completion release does not erase staged write data.
This permits machine code to read metadata before submitting an already-staged write, without a language-specific device operation.

Capacity reads latch a coherent 64-bit count, using the same acceptance/consumption rule as clock reads.
Flush ignores sector, offset, and length, reports zero transferred bytes, and completes only after earlier successful writes reach durable storage.
Read-only media permits reads and flushes. An absent medium reports zero sectors and produces a no-media completion.

| Completion code | Meaning |
| ---: | --- |
| 0 | Success; a read/write transferred exactly the requested byte count |
| 1 | No media; zero bytes transferred |
| 2 | Requested range exceeds capacity or wraps the sector address; zero bytes transferred |
| 3 | Write to read-only media; zero bytes transferred |
| 4 | Media I/O error; transferred count identifies the completed prefix |

The transferred count is a multiple of four and never exceeds the request length. For a read, only that prefix has valid completion data.
A failed write can change that prefix. It cannot report success for uncommitted bytes, and the remaining bytes stay unchanged.
This completion status is distinct from the transport error bit. A failed register access has no side effects.
Runtime error handling must not assume that a media error rolls back an accepted write.

Completion clears busy, sets completion available, and adds exactly one event to source 2.
Status, identifier, count, and read data remain stable until release. Reads beyond the transferred prefix fail without advancing the cursor.
Data reads require a read completion. Write and flush completions carry no read buffer.
Completion registers and read data reject access before completion. Release rejects values other than 1 and requires a retained completion.
The runtime reads the result and data, acknowledges event source 2, then releases the completion before submitting another request.
Release does not acknowledge the event. Reset cancels pending control state and notifications, but does not undo storage writes already committed.

The runtime owns AltoFile page/header conventions, transfer state, and the mapping between guest requests and sectors.
The device receives no Alto command number, object identity, class, buffer reference, or semaphore reference.
It cannot inspect an AltoFile object or signal its guest completion semaphore.

The selected image's AltoFilePage buffer contains a 16-byte label followed by 512 data bytes.
The [Alto hardware manual](https://www.bitsavers.org/pdf/xerox/alto/AltoHWRef.part2.pdf) describes separate header, label, and data records.
Primitive 128 must translate that layout through machine code; the generic device must not decode it.
Its page-to-sector mapping, command handling, and delayed completion path are part of stage 5 storage integration.

### Snapshot target registration

Primitive 135 registers the target for a later snapshot. It accepts exactly four serial-number bytes and an unsigned 16-bit virtual leader address.
The address can be a nonnegative SmallInteger or LargePositiveInteger with value at most 65535.
Microcode validates complete byte representations, copies the values, and returns the receiver.
Wrong arity, malformed bytes, and invalid addresses enter guest fallback before allocation or mutation.

Component 40 of root 29's private Array retains a six-field registration Array.
Its first four fields copy the serial bytes in their original order. The final fields hold the leader address's low fourteen and high two bits.
Every field is a canonical SmallInteger. Mutating the original arguments cannot change the registered target.
A successful replacement publishes the complete new record. Failed replacement retains the old one.
The registration survives paging and collection without an external device retaining guest references.

Registration issues no device request and does not resolve a host filename. It does not perform a snapshot.
Snapshot transfer uses ordinary block storage. Machine code must prepare roots, materialize contexts, and establish the saved heap state before publication.
The device only stores the resulting bytes. A snapshot is not a serialization of Rust oracle state.

## Runtime events and idle execution

The Smalltalk runtime keeps registered semaphore references in machine-visible GC roots.
LOGIK now supplies `ldroot` and `d=Root` for the 32 explicit root slots, indexed through NUMERIK register A.
These generic controls carry full tagged words and publish only at successful retirement.
RTL tests demonstrate retaining a newly allocated object through collection and releasing it after its root is cleared.
Those references must survive allocation, eviction, and collection while the devices are armed.
Primitive 93 registers, replaces, or clears the input semaphore in root slot 27; it accepts Semaphore or nil.
Primitive 100 keeps the timer semaphore in slot 28. Replacement cancels the old timer before publishing the new registration.
Timer delivery releases this one-shot registration after retaining its recipient in VR6.
Slot 29 retains the private device-state Array, including buffered input, registered cursor/display Forms, and copied snapshot target metadata.
Its first field reserves the storage semaphore; the storage primitive remains unfinished.
Slot 30 retains a low-space registration Array containing the semaphore and copied numeric thresholds.
Slot 31 holds a raw idle flag; it contains no guest reference.
No guest reference lives solely in an external device model.

At a completed bytecode boundary, microcode materializes the active context and delivers pending notifications through the ordinary Semaphore signal algorithm.
It applies the existing priority/preemption rules, then resumes or switches contexts.
Microcode roots timer and storage recipients in VR6 before acknowledgement. Input recipients remain in root 27 while pending word counts survive in the buffer.
The device never edits a Process, Semaphore, or scheduler queue.

When no process is runnable, microcode completes the wait/suspend send and materializes its continuation before entering an interrupt wait loop.
The wait loop allocates no context and changes no guest fields.
Event delivery uses the same priority rules as synchronous signal. An idle wakeup does not requeue the blocked process.
The idle state must also remain inspectable through the machine debugger.
Quit and debugger primitives use explicit halt/debug policy; they do not invoke a host language interpreter.

Low-space notifications come from generic OBJEKT capacity counters, not a host heap traversal.
Microcode converts the machine's available-word and remaining-identity counts into guest integer representations.
Primitive 116 accepts a Semaphore with nonnegative identity and word thresholds, or nil to cancel.
Identity thresholds support all 37 bits; word thresholds support 32 bits. Invalid arguments preserve the previous registration and enter guest fallback.
Threshold values are copied into positive SmallInteger chunks in an ordinary rooted Array, so later argument mutation cannot change them.
At bytecode boundaries, microcode reads each threshold before sampling the corresponding OBJEKT counter, accounting for paging or collection during those reads.
It signals when either available count is strictly less than its threshold; equality does not signal.
Delivery releases the registration before signalling, preventing repeated signals while space remains low. A new registration rearms notification.
Zero thresholds disable their respective conditions. This path issues no device requests and requires no host heap callbacks.

## Required checks

Transport tests must hold requests and responses, inject errors, change IRQ while waiting, and reset outstanding transactions.
They must establish exactly-once side effects and unchanged local state on failed replies.

Guest tests must register semaphores, force collection, enter idle, receive a delayed input or timer event, and resume the correct process.
They must also cover timer replacement, simultaneous events, FIFO pressure, and delayed storage completion.
These checks are required before the device and scheduler portions of runtime stage 4 are complete.
Input tests cover registration replacement/clearing, invalid-argument fallback, repeated notifications, preemption, and wakeup of the same or a different process.
Clock/timer tests cover unsigned packing, coherent rollover reads, immediate expiry, modular deadlines, replacement, cancellation, and failure before timer changes.
Pointer tests cover coherent reads during motion, signed coordinate limits, argument rejection, linked movement, and atomic cursor publication with failed replies.
Low-space tests cover full-width thresholds, strict crossing, replacement, cancellation, rearming, invalid operands, preemption, and copied-threshold independence.
The registration also survives repeated machine collection during a guest allocation loop.
Bitmap tests cover row packing, pixel padding, dimensions, malformed Forms, late malformed words, missing devices, and failed replacement.
Transport checks preserve the previous visible bitmap throughout upload and reject incomplete or failed publication.
Original image tests register existing Cursor and DisplayScreen bitmaps and compare every visible pixel.
Buffered-input tests cover word conversion, ring wraparound, late registration, polling, empty-buffer fallback, FIFO pressure, overrun, and timer delivery during backpressure.
Snapshot-target tests cover full unsigned leader addresses, wrong arity, malformed values, atomic replacement, argument mutation, and repeated collection.
An original-image test also executes the AltoFile registration method without changing any original image object.
They force collection and delay memory, backing-store, and device responses. Storage transfer checks remain unfinished.
The original primitive 128 fallback reports unavailable storage through its guest error field, with no buffer mutation, semaphore signal, or device request.
The saved-context startup test records device ordering through the first BitBlt call.
Interactive bitmap execution, actual snapshot save/reload, and later startup device ordering remain stage 5 integration work.
