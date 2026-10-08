# Squeak 1.1 microcode

These files form one program, assembled by `squeak::interpreter::assemble` in `rekursiv-smalltalk`.
The loader supplies image-specific identities from the special-object array and the saved context.
The processor executes ordinary LOGIK, NUMERIK and OBJEKT operations.
Rust loads the image and observes execution; it does not implement Squeak primitives.

| File | Responsibility |
| --- | --- |
| `interpreter.uc` | Context caches, field access, stack operations, branches and arithmetic fast paths |
| `bytecodes.uc` | Complete 256-entry NAM/map dispatch table |
| `sends.uc` | Extended sends, stored-hash lookup, headers, quick returns, activation and return |
| `blocks.uc` | Home contexts, block creation and evaluation, invalid returns |
| `messages.uc` | Original two-field Message construction for `doesNotUnderstand:` |
| `perform.uc` | Dynamic sends and argument-array expansion |
| `dispatch.uc` | Squeak primitive numbers and explicit unsupported-operation stop |
| `primitives.uc` | Identity and class results |
| `integers.uc` | Division, shifts, bit operations and Point creation |
| `indexed.uc` | Indexed fields, Characters and unsigned 32-bit word conversion |
| `storage.uc` | Allocation and the original identity-hash generator |
| `scheduler.uc` | Process queues, semaphores and switches at completed bytecode boundaries |
| `floats.uc` | Binary64 register pairs, Float allocation and primitive policy |
| `bitmaps.uc` | Form validation, palette setup and atomic scanout upload |
| `bitblt.uc` | Raster operands, clipping, rooted scratch frame and alias handling |
| `pixels.uc`, `words.uc` | Pixel mapping, Boolean rules and aligned word operations |
| `refresh.uc` | Publish changed display/cursor regions through device registers |
| `devices.uc`, `events.uc` | Clocks, mouse, keyboard buffering, timer and low-space delivery |
| `files.uc` | Guest paths, handle validation and byte/word file transfers |
| `bulk.uc`, `streams.uc` | Array replacement, fill and indexed stream operations |

Guest fields start at physical component 3, after descriptor and hash/format metadata.
A MethodContext has sender/IP/SP/method/unused/receiver at components 3–8, then temporaries and stack slots.
A BlockContext replaces method/unused/receiver with argument count/initial IP/home context.
Its evaluation stack is local, while temporary access resolves through its home context.

VR0 roots the active context, VR1 its method and VR2 its receiver.
VR3–VR7 hold temporary object references during an operation.
NUMERIK R8 holds the one-based guest byte IP; R9 holds SP, including temporaries.
R10 caches literal count, R11 method byte length, R12 stack floor and R14 frame capacity.
R13 identifies the bytecode, except within allocating helpers that no longer need it.
R15 holds a primitive failure continuation or a terminal status.
References must stay in tagged VRs, object fields or live expression-stack slots across allocation and paging.

`boundary` saves IP/SP, releases scratch roots and checks deferred process switches.
`decoded` exposes a fetched byte in R0, with R8 advanced past it.
`primitive_dispatch` exposes the method's primitive code in R0, arity in R1, receiver in VR6 and method in VR7.
`send_result` replaces receiver and arguments with VR5 only after successful completion.
`primitive_failed` preserves those operands and returns through R15 to lookup or method fallback.

The saved image restores its colour desktop in the software emulator.
Mouse menus open and dismiss through the guest scheduler and drawing paths.
Unknown required primitives still stop with status 8; snapshot writing is not implemented.
The [port document](../../docs/squeak-1.1.md) lists the implemented contract, limitations and run commands.

Root slots 0–2 hold the special-object array, initial context and identity-hash state.
Slot 3 holds the cursor Form; slot 4 is a pending low-space flag.
Slots 5–7 hold keyboard ring indices/count; 10–14 hold interrupt and low-space state.
Slots 26–31 hold the active context, input semaphore, timer semaphore, keyboard ring,
low-space semaphore and idle flag. Raw counters carry no object references.
The registered display lives in the guest special-object array.

BitBlt saves IP/SP in its rooted expression-stack frame while R8/R9 traverse pixels.
Shared validators use expression-stack slot zero for compact checks.
They must not overwrite a caller's rooted Form or temporary object slot.
