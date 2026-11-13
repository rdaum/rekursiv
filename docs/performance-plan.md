# Performance plan

Status: working notes, November 2026. This records where the MEGA65 machine spends its time running
the Smalltalk-80 image, what has been done about it, and the improvements still planned. Numbers are
from the emulator's cycle estimator at the board's sizes (32,768 pager entries, 2^23 words, 640×480,
25.2 MHz) unless they say otherwise.

## The measure

Typing one character into the System Workspace runs about 30,000 bytecodes. The benchmark is the
microinstructions and estimated time for the 30,000 bytecodes after one keystroke:

| Change                                         | Microinstructions per bytecode | Time, SDRAM cost model |
| ---------------------------------------------- | -----------------------------: | ---------------------: |
| Before this work                               |                            180 |                 0.69 s |
| IP and SP kept in registers between bytecodes  |                            152 |                 0.51 s |
| Method cache                                   |                            144 |                 0.48 s |
| Frame caches set without reloading the context |                            138 |                 0.46 s |
| BitBlt per-word validation pass removed        |                            122 |                 0.42 s |
| BitBlt draw loop, overlaps without staging     |                            112 |                 0.40 s |
| One pending flag for boundary work             |                            104 |                 0.39 s |
| `fetch_byte` checks overlapped with the fetch  |                            101 |                 0.38 s |
| Object commands launched, joined through       |                            101 |                 0.29 s |

On the board itself a keystroke took over a second before the block-RAM memory cache, and noticeably
less after it. A well-tuned bytecode interpreter spends perhaps 10 to 30 microinstructions per
bytecode; 101 is the number to attack now.

Even with ideal memory the sequencer averages about 1.8 cycles per microinstruction, because every
OBJEKT command stalls it until the command completes. On the board it is about 2.2 to 2.8.

## Done

- **Low-space line and pending flag** (`263dc2c`). OBJEKT compares free words and identities against
  thresholds in hardware; LOGIK has a microcode-owned pending flag. The two per-bytecode checks no
  longer read memory. They had cost a third of a keystroke.
- **Primitive 105**, byte range replacement (`25f4d8d`). String copying had been 74% of a keystroke.
- **Context recycling** (`7256e8e`). A returning MethodContext is parked in its sender's home and
  reused by the next send needing that frame size. Boot allocations fell from 240,235 to 11,318.
- **Block-RAM memory cache** (`fd785fa`). 16,384 words, direct mapped, write-through with a write
  queue. A read hit takes 2 cycles instead of about 9.
- **IP and SP in registers** (`a2cdbec`). They are written into the context only at sends, process
  switches and stops, not at every bytecode. A method's own temporaries are read without resolving
  its home.
- **Trusted frame reloads** (`5791d01`). Activation sets the new frame's caches from its own
  registers; after a quick method or primitive, and on returns into counted frames, only the caches
  the send used are rebuilt.
- **BitBlt** (`6a545e3`, `c257b66`). The pass that validated every pixel word before drawing is gone
  (words draw as their low 16 bits). A new draw loop copies overlapping rectangles in an order that
  needs no staging, reads each source word once, and keeps its invariants in registers. BitBlt fell
  from 36.6 to 10.6 microinstructions per bytecode of a keystroke.
- **Bytecode boundary** (`7a3786a`, `78d979d`). A pending process switch and idle now raise the
  pending flag, so a quiet boundary is three condition tests; `fetch_byte`'s bounds checks overlap
  the method fetch.
- **Background object commands** (item 7, first step). Every command that pages nothing now
  launches: its instruction retires when OBJEKT accepts it, and the next barrier executes in the
  cycle the reply arrives, with the reply forwarded as Object. OBJEKT accepts the next command as it
  delivers a reply. A keystroke fell from 4.87M to 3.88M cycles with fast memory and from 8.42M to
  7.35M with the SDRAM cost model. Paging commands still block, and one command is in flight.
- **Method cache** (`a2cdbec`). 512 entries of class, selector and method in a private array. 94% of
  sends hit; dictionary searches per keystroke fell from about 11,800 to 480.

## Where the time goes now

Profile of the 30,000 bytecodes after a keystroke, with all of the above in place, under the fast
memory model (1.79 cycles per microinstruction):

| Routine                                                                        | Microinstructions per bytecode | Share of cycles |
| ------------------------------------------------------------------------------ | -----------------------------: | --------------: |
| BitBlt                                                                         |                           36.6 |           19.7% |
| Bytecode fetch (`fetch_byte`)                                                  |                           13.5 |            9.5% |
| Context reload after sends and returns (`load_context`, `context_home`, floor) |                           13.4 |           10.6% |
| Bytecode boundary and dispatch (`boundary`, `decoded`, events)                 |                           11.0 |            8.1% |
| Sends: preparation, method cache, activation                                   |                           20.1 |           17.0% |
| Evaluation stack (`push`, `peek`, `pop`)                                       |                            7.4 |            6.9% |
| Variable access                                                                |                            7.9 |            5.8% |
| Argument copying and result handling                                           |                            6.9 |            6.2% |

Typing redraws text glyph by glyph, which is why BitBlt leads. `fetch_byte` spends six
microinstructions per byte recomputing and checking the method's bytecode bounds and five more
re-selecting the method and reading the byte. Each context reload is about 90 microinstructions that
re-derive the method, receiver, header fields and stack floor, often from a header the send path
decoded moments earlier.

## Order of work

1. Context reload (item 1): microcode only, about 10%.
2. Streams and the BitBlt combiner (item 10): hardware, the largest remaining share.
3. Bytecode prefetch (item 8) and background object commands (item 7): hardware, attacking fetch,
   dispatch and the stall cycles everywhere.
4. Packing the hot paths and avoiding re-selection (items 2 and 3) as each path is touched.
5. The evaluation stack (item 6): the smallest share of the major items and the most disruptive,
   because its hardware is already used for other purposes.

## Planned: microcode

### 1. Carry decoded state across sends and returns

`load_context` runs after every send and return. It fetches the context, validates its size and its
IP and SP, resolves the home context, reads the method and receiver, fetches the method, reads its
descriptor for the byte length, decodes the header for the literal and temporary counts, and
computes the stack floor. On activation the send path has just fetched and decoded the same method
header, so the new frame's caches (R10 to R14, VR1, VR2) can be set directly from it. On return, the
sender's decoded values could be saved where the return can find them without re-deriving them, for
example in the returning frame's spare fields or a small per-frame record, or validated only once
per method. The validation that protects against malformed contexts must stay where a context can
come from the guest (process switches, `thisContext` users, the debugger).

### 2. Pack the hot paths

Most hot microcode does one thing per word, for example:

```text
read=Vr, vr=0          ; get the context reference
d=Object, page=Fetch   ; select it
d=3, idx=Load          ; set the field index
mem=Read               ; read the field
d=Object, ldsym        ; consume it
```

A control word can carry an ALU operation, a register write, a stack operation, a branch and an
OBJEKT command at once, and `idx=Load` can accompany `read=Vr`. Schedule the hot paths by hand:
`fetch_byte`, the boundary, `push`/`peek`/`pop`, `send_prepare`, `cache_lookup`, activation,
`return_sender` and `load_context`. Expected: 20 to 30% fewer words on those paths.

### 3. Stop re-selecting objects that are already selected

`fetch_byte` fetches the method for every bytecode; `push` and `peek` fetch the context for every
stack access. Each `page=Fetch` is a full OBJEKT command. Keep the method selected across bytecodes,
or read bytecodes through a prepared address that stays valid while the method runs, and keep
selection stable on the other hot paths.

### 4. Overlap memory with ALU work

`prepare` and `launch` let one memory read run while independent microinstructions retire. Only
BitBlt uses them. Candidates: fetch the next bytecode while the current one finishes, and read the
next method-cache component while comparing the previous one.

### 5. Further allocation and collection work

- Contexts are not recycled after any `thisContext` or block creation, which happens on every
  keyboard poll (`Semaphore>>critical:`). A finer rule (mark only the frames actually captured)
  would recycle about twice as many.
- A minor collection: objects loaded from the image and never modified cannot point at new objects,
  so a collection can trace only new objects and modified old ones without moving anything.
- Preload the whole image at boot in one sequential pass, which removes the slow first use of each
  menu and editor.

### 6. Evaluation stack in the hardware stack

LOGIK has a 32-entry evaluation stack with a top-of-stack register, which the original Rekursiv used
for expression evaluation. The Smalltalk microcode keeps each context's stack in the context object,
so every push and pop is an OBJEKT command and a memory access.

Keep the active frame's stack in the hardware stack instead, and spill it into the context at the
points where the context must be complete: a send that activates a new frame (the arguments move
into the new frame anyway), a process switch, `thisContext`, a primitive that can reach a context,
and a collection. Reload on return. These are the same capture points that context recycling and the
IP/SP change already identify.

Things to settle: frames deeper than the hardware stack (spill the bottom); the collector must treat
the live stack entries as roots, which it already does for entries below SP; and `peek`, `push` and
`pop` become register operations, which removes most of `stack.uc`'s memory traffic.

Its hardware is not free: the stack pointer register currently doubles as the "process switch
pending" flag that `check_process_switch` tests, BitBlt keeps seventeen rooted values in slots 15 to
31, and `perform:` uses slots 1 to 3. Those would move first. The evaluation stack is about 7% of a
keystroke, so this follows the larger items.

## Planned: hardware

### 7. Object commands in the background

Done in part: commands that page nothing launch and are joined through (see Done). Remaining: paging
commands still block, and only one command is in flight. A scoreboard would let it continue until an
instruction actually uses a result (Object, a VR, the index, or the selection), and OBJEKT would
accept two or three commands in flight with its memory channel pipelined. This converts most of the
0.8 to 1.8 waiting cycles per microinstruction into work. It is the largest single hardware gain
available, and the largest change: LOGIK's issue logic, OBJEKT's command interface, the reference
model, the emulator's cycle estimator and the differential tests all change.

### 8. Bytecode prefetch and dispatch

Every bytecode pays `boundary → cycle → fetch_byte → decoded → dispatch`, about 25
microinstructions, nearly a fifth of all of them. A next-byte register that OBJEKT fills from the
running method while the current bytecode executes, feeding the NAM/MAP dispatch directly, would
make dispatch nearly free. This is close to how the original Rekursiv fetched its own instruction
stream.

### 9. A LUT-RAM cache in front of the block-RAM cache

A few dozen of the hottest words in distributed RAM answer in one cycle instead of two. Smaller gain
than items 7 and 8, and it lengthens a path with little timing slack.

### 10. Streaming memory access and a BitBlt combiner

BitBlt is 13% of a keystroke and about 33% of drawing the desktop, plus 9% more uploading changed
words to the display. Its inner loop reads a source word, reads a destination word and writes the
merged result. Each is a separate OBJEKT command and SDRAM burst; the shift-and-merge arithmetic
between them is a few microinstructions. Memory traffic, not arithmetic, is the cost. Smalltalk
Forms hold 16 pixels per word object, one machine word each, so a 640-pixel row is 40 separate
accesses.

Classic Smalltalk machines did BitBlt in microcode with datapath help, not with a separate blitter.
The Alto ran it in microcode (Ingalls, 1976 memo: 15 cycles per word for a fill, 36 for a move). The
Dorado added a 32-in/16-out cyclic shifter/masker used by its BitBlt microcode. The PERQ's RasterOp
had microcode generate addresses and start overlapped memory cycles while hardware aligned words
with a barrel shifter and combined them through "combiner PROMs", on a 64-bit memory path. No
evidence was found of a blitter in the Tektronix 4404 (10 MHz 68010).

The plan has two levels, the first general and the second built on it:

1. **Streams.** Extend "one prepared access" to "a prepared run". Microcode names an object, a first
   component and a count once; OBJEKT fetches the words back to back into a small queue, and each
   loop iteration takes the next word from a register instead of issuing a command. A write stream
   is the mirror image. Within one SDRAM row, consecutive words can come out as one long burst at
   about one word per 4 cycles instead of a 4-beat burst each. This helps every sequential walk:
   BitBlt rows, string copies and comparisons (primitive 105 would use it at once), hashing, method
   dictionary scans, and the collector's copy, which already has a private version of this.
2. **A combiner.** A unit that takes source and destination streams, applies the shift, masks,
   halftone and combination rule, and writes the result stream. With the frame buffer as a possible
   destination it also replaces the separate upload of changed words to the display device. This is
   effectively DMA with a transform: microcode validates the Forms, clips, chooses the rule and sets
   up the rectangle; hardware moves the words.

Design points:

- A stream holds a physical address across many cycles, so it must be relocated by the collector and
  invalidated by eviction, as the prepared address is today.
- The device contract says the display device does not perform BitBlt, and the machine must. A
  combiner inside the processor respects that; moving BitBlt into the display device would not.
- Order of work: define the stream in the Rust model (control fields, end conditions, faults,
  eviction and collection), add it to the assembler, rewrite BitBlt's inner loops and primitive 105
  on the emulator, and measure with the cycle estimator, extended for streaming bursts, before any
  RTL. The estimate of a streamed word's cost is an assumption until the RTL exists and the
  calibration tests tie the two together.
- Expected gain: plausibly 10 times or more on BitBlt itself with the combiner; streams alone
  perhaps 2 to 3 times on BitBlt and string operations.

### 11. Faster clock

25.2 MHz closes with about 1.7 ns of slack. The SDRAM controller is written for at most about 33
MHz. A faster machine clock needs the controller to count its intervals in cycles, and the full
design's critical paths examined.

## A second LOGIK

The FPGA has room in logic: LOGIK is about 20,000 LUTs (its control store's literal tables included)
and NUMERIK 7,400, against 133,800 available and 35,100 used. Block RAM is the constraint: 336.5 of
365 RAMB36 are used, and the control store alone is 64. A second sequencer would have to share the
control store through its second port, which today serves only halted programming. That is feasible.

The obstacle is not size but semantics:

- Smalltalk-80 is sequential at the virtual-machine level. Processes are scheduled one at a time,
  and the image assumes that between two bytecodes nothing else changes its objects. Running two
  Smalltalk processes at once would break that assumption throughout the image.
- OBJEKT is stateful per command stream: one selected object, one index, eight VRs, one prepared
  address. A second LOGIK needs its own copy of that state, and the pager table, the collector and
  the memory channel become shared and need arbitration.

Uses that respect those constraints, roughly from most to least practical:

- **A dedicated BitBlt or stream engine** that runs a blit while the main sequencer continues, with
  the guest seeing completion at its next use of the destination. This is item 10 with its own
  sequencer rather than fixed logic.
- **A background collector** running concurrently with the mutator. It needs a write barrier on
  every pointer store, which the processor would have to provide.
- **A runtime designed for parallelism**, rather than Smalltalk-80: a language whose processes do
  not share mutable objects, or share them only through explicit synchronization, could run one
  process per sequencer.

For the Smalltalk-80 image, items 7 and 8 extract more parallelism, inside one instruction stream,
for less effort than a second sequencer.
