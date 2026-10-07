# Native microcode emulator

`rekursiv-emulator` executes Rekursiv microcode in a native Rust process, without Verilator. It uses
the same assembler, instruction definitions, and microcode files as the RTL machine. The window
presents device pixels and supplies keyboard and mouse input.

The emulator operates at instruction boundaries. It does not predict FPGA cycle counts or
throughput. Verilator remains necessary for handshake, pipeline, stall, reset, and synthesis
validation.

The CLI uses a [Cranelift](https://docs.wasmtime.dev/api/cranelift_jit/index.html) JIT by default.
It translates arithmetic, flag calculations, branches, stack controls, fetch controls, and validation checks into native code.
Native blocks execute up to 32 local instructions per call.
Each instruction retains its device tick, retirement boundary, and fault checks.

Object instructions use native preparation followed by the existing OBJEKT command path.
Blocking commands publish local writes only after success.
A `launch` instruction publishes local writes at command acceptance and defers its reply until an
[object barrier](interface.md#retirement-conditions-and-errors).
A fault inside a native block retains earlier retirements and drains any older object reply first.

Expression-stack and control-stack accesses use their old addresses, even when the same instruction changes a pointer.
`ESP=SP` forwards the new stack pointer.
Compact-value construction and stack bounds remain checked before retirement.
`Fetch::Both` fetches a new NAM word while mapping the old opcode.
The sequencer also uses the old dispatch target.

Fetch reads live NAM and opcode-map entries through small Rust ABI adapters.
These adapters handle optional table slots without assumptions about Rust enum layouts.
Native code extracts the fields and updates fetch state.
Replacing or resizing either table takes effect on the next access without JIT recompilation.

Floating-point, device, root-access, and collector instructions use the checked interpreter.
Interrupt-sensitive instructions execute individually so each condition uses the correct device state.
Microcode still implements Smalltalk operations and collection.
The JIT changes host execution speed without changing the hardware control words or replacing guest operations.

The control store remains writable through `Machine::image_mut()`.
`Machine::image()` supplies read access without invalidation.
A mutable image borrow marks the instruction caches dirty before it exposes the image.
Before the next `step` or `run_steps`, the executor refreshes scalar decoding and invalidates changed native translations.
This check includes every block that overlaps a changed or removed word.
Unchanged translations remain available, including after NAM/map-only edits.
Changed words use the interpreter until a library caller recompiles the image.
Execution performs no repeated source comparisons between edits.
JIT code belongs to the machine and is freed when the machine drops or disables the JIT.

## Run the workstation demo

Install Rust and a C compiler. The default window build also needs the platform window libraries. On
Debian or Ubuntu, install them with:

```sh
sudo apt-get install build-essential libx11-dev libxkbcommon-dev libwayland-dev
```

Run the included peripheral demo:

```sh
cargo run --release --locked -p rekursiv-emulator
```

The [demo microcode](../microcode/workstation.uc) uploads a stripe pattern and a cursor. Mouse
movement moves the cursor. A key or mouse-button press inverts the pattern. Microcode consumes the
input packets and uploads the replacement pixels. The window adapter does not draw that pattern or
choose its response to input.

Close the window to exit. Escape goes to the guest. A stopped processor leaves its last frame
visible, with its micro-PC in the window title. The default window session has no instruction limit.

## Run existing microcode

Run the allocation and collection examples without a window:

```sh
cargo run --release --locked -p rekursiv-emulator --no-default-features -- \
  --headless --microcode microcode/allocation.uc
cargo run --release --locked -p rekursiv-emulator --no-default-features -- \
  --headless --microcode microcode/collection.uc --memory-words 512 --pager-entries 16
```

The second example executes seven allocations and five collections. The emulator executes the
existing `ram-collector.uc` instructions for each collection. Smalltalk defaults to 16,777,216 RAM
words; standalone microcode defaults to 131,072. An explicit `--memory-words` value overrides either
default and includes both semispaces. Each word carries 40 bits in a Rust `u64`. The native pager
defaults to 65,536 entries. `--pager-entries N` selects a power of two from 2 through 65,536. The
loader assembles the collector for that capacity. RTL comparison tests explicitly use the 16-entry
hardware test configuration.

A microcode file can supply its own `.collector` directive. Otherwise, the loader installs the
standard collector at microaddress 8064 and rejects overlap. The loader supports the assembler's
code, NAM, opcode-map, root, and entry directives.

## Run the Smalltalk image

Fetch the pinned Xerox V2 distribution:

```sh
python3 scripts/fetch-smalltalk-image.py
```

The Smalltalk desktop and initial window use 1024×768 pixels. The loader adjusts the saved display
configuration before execution. Smalltalk creates the full bitmap and redraws the desktop during
startup. Resizing the host window scales the guest display.

Run its saved process and watch it draw the desktop:

```sh
cargo run --release --locked -p rekursiv-emulator -- \
  --smalltalk artifacts/st80/VirtualImage --memory-words 16777216
```

For a reproducible run without a window, use:

```sh
cargo run --release --locked -p rekursiv-emulator --no-default-features -- \
  --headless --smalltalk artifacts/st80/VirtualImage --memory-words 16777216 \
  --steps 300000000 --frame artifacts/startup.ppm
```

The loader verifies and converts the image before execution. It then seeds backing storage and the
boot context. Runtime bytecode dispatch, method lookup, primitives, process scheduling, and
collection execute through microcode. The emulator does not supply missing Smalltalk operations
through Rust callbacks.

Primitive 96 now draws and refreshes the display through microcode. Native startup draws the
browser, transcript, and workspace. BitBlt validates accessed words and uploads changed pixel
groups. Disjoint copies need no temporary object. Disk transfers and snapshot saving remain
unfinished. Low-space checks now collect before notification and recheck available space afterward.
The original image passes its former premature warning point with 1,048,576 RAM words. A
200-million-step headless run completes 769 BitBlts and continues guest controller activity without
a low-space signal. This remains bounded execution evidence, not proof of complete desktop
interaction. The [rendering measurements](validation.md#bitblt-rendering-measurements) compare the
same image checkpoint before and after the optimization. The bounded startup regression establishes
drawing progress, not complete interaction. To stop at the earlier checkpoint before any drawing,
add `--stop-at primitive_dispatch --when R0=96`. The window presents complete published frames. It
does not read a live Form from the heap.

## Controls and diagnostics

| Option              | Behavior                                                                          |
| ------------------- | --------------------------------------------------------------------------------- |
| `--microcode FILE`  | Assemble and execute a standalone program                                         |
| `--smalltalk FILE`  | Convert and start the pinned Xerox V2 `VirtualImage`                              |
| `--headless`        | Disable the window and use deterministic device time                              |
| `--engine MODE`     | Select `jit` (default) or `interpreter`                                             |
| `--steps N`         | Stop after N instruction steps, including collection and Hold steps               |
| `--pager-entries N` | Set pager capacity; default 65536, power of two from 2 through 65536              |
| `--memory-words N`  | Set external RAM capacity; default 16777216 for Smalltalk, otherwise 131072 words |
| `--stop-at LABEL`   | Stop before an instruction at the named label                                     |
| `--when Rn=VALUE`   | Add a register condition to `--stop-at`; decimal or `0x` hexadecimal              |
| `--objekt-metrics`  | Collect and report detailed OBJEKT counters                                       |
| `--trace FILE`      | Record retired micro-PCs, collector mode, object result, and numeric registers    |
| `--frame FILE`      | Save the last published display as a PPM, without cursor composition              |
| `--frames N`        | Close after N presentation iterations, including those that skip uploads          |

Headless execution stops after ten million steps unless `--steps` supplies another limit. Processor
faults report the micro-PC, fault code, and object status where applicable. A library caller can
resume an explicit service break with `Machine::resume()`. The CLI leaves service breaks stopped.

The exit summary reports active execution seconds, total elapsed seconds, and retired
microinstructions per second for both intervals. The rates include mutator and collector
instructions. Hold steps and collector entry/return transitions do not count as retired
instructions. Active time includes instruction execution, emulated devices, recovery setup, and
optional trace output. It excludes worker input/clock updates and snapshot publication. Elapsed time
also includes window startup, frontend work, and time spent displaying a stopped processor. Window
work runs concurrently with CPU execution, so their measured durations overlap. Both intervals
exclude image loading/conversion, JIT compilation, and final trace flush/frame export. The window title updates
approximately once per second with the recent active execution rate. Headless runs with fixed memory
and step counts provide repeatable workloads. Trace output affects throughput.

JIT startup reports its function count and compilation time separately.
The exit report counts instructions retired in native blocks and individual native preparations.
Preparations include attempts that later fault or enter collection, so they are not retirement counts.

### Compare execution engines

Run the same image and step budget under each engine:

```sh
python3 scripts/bench-emulator.py
```

The benchmark builds the release binary and alternates engine order across five pairs of runs.
Each run executes 50 million steps with deterministic device time.
The script compares final processor counters and framebuffer hashes, then reports median execution rates.
Logs, frames, and a JSON summary go to `artifacts/jit-benchmark`.
Compilation time is separate from execution time.
A separate before/after comparison used three alternating pairs on an ARM Cortex-X925 core, pinned to CPU 7.
Both versions used the JIT, deterministic device time, and 16,777,216 RAM words.
The baseline was commit `28c324a`, before explicit cache invalidation and the OBJEKT response-only path.

| Step budget | Baseline | Current | Throughput gain |
| --- | --- | --- | --- |
| 50 million | 25.5 million/s | 33.6 million/s | 31.6% |
| 500 million | 20.4 million/s | 25.7 million/s | 25.6% |

Rates are medians and exclude image loading and compilation.
Each pair produced the same final counters and framebuffer hash.
These runs had no collections; collection-heavy workloads need separate measurements.
On Linux, `taskset -c 7 python3 scripts/bench-emulator.py` pins an engine comparison to that core.
Choose a suitable CPU number on your host; mixed CPU types can affect comparisons.
These measurements describe native emulation, not FPGA throughput.

A library caller starts with the interpreter and enables translation through `Machine::enable_jit()`.
`Machine::run_steps(budget)` uses native blocks when possible and never exceeds its step budget.
`Machine::step()` preserves single-instruction observation, including recovery transitions and held instructions.
CLI traces and breakpoints use that single-step path, which can be slower than the interpreter.
`Machine::disable_jit()` releases generated code and restores interpreter execution.

### OBJEKT metrics

With `--objekt-metrics`, the exit report includes OBJEKT counters. `--pager-entries` changes pager
capacity; `--memory-words` changes RAM capacity only.

- Fetch and probe counts describe explicit selections, excluding internal metadata revalidation and
  collector lookups. A collision miss finds an occupied slot with a different full reference. An
  empty miss finds no mapping. Hit rates exclude compact values and invalid operands. Retries count
  as additional attempts.
- Completed evictions count successful allocation/refill replacements. Victims are classified as
  new, modified persistent, or clean persistent objects. A failed miss is not a completed eviction.
  Collector discards are counted separately.
- Allocations and refills report completed objects and body words. Refill words consume RAM space
  even when the guest creates no new objects.
- Field counters report successful accesses, including reads served by the cached first word.
- RAM and backing counters report issued requests, including requests from failed commands. Backing
  traffic includes identity exchange and directory operations. Save commits count requests, not
  durable disk commits. Payload sizes use five bytes per 40-bit word, excluding metadata, protocol
  overhead, and physical bus padding.
- Collector reads and writes are separate from mutator RAM requests. Collection entries distinguish
  explicit requests, allocation pressure, and refill pressure. Reclaimed words and discarded
  mappings count successful collector commits. Reclaimed space includes abandoned copies from
  earlier evictions.

These counters exclude image loading and bootstrap services. They measure logical machine activity,
not FPGA cycles or disk latency. Detailed counters are disabled by default and require no per-object
history. Collection adds execution overhead; use the same setting when comparing implementations.
Library callers enable `machine.objekt_metrics_enabled` before execution and read
`machine.stats.objekt`.

For a reproducible run with the original image:

```sh
cargo run --release --locked -p rekursiv-emulator -- \
  --headless --smalltalk artifacts/st80/VirtualImage \
  --memory-words 16777216 --steps 100000000 --objekt-metrics
```

Add `--pager-entries 16` to reproduce the former capacity. Equal step budgets can include different
amounts of collection and guest work. The drawing regression compares identical pixels after 32
completed BitBlt operations at both capacities.

### Profile OBJEKT commands

The [standalone OBJEKT benchmarks](../crates/rekursiv-model/benches/objekt/main.rs)
use the published `micromeasure` crate. Run all cases, or filter by name:

```sh
cargo bench --locked -p rekursiv-model --bench objekt
cargo bench --locked -p rekursiv-model --bench objekt -- resident/read_field
```

On Linux, pin execution to one core and save the measurements as JSON:

```sh
mkdir -p artifacts
MICROMEASURE_OUTPUT=artifacts/objekt-bench.json \
  taskset -c 7 cargo bench --locked -p rekursiv-model --bench objekt
```

Choose an available core on your host. Hardware counters require access to Linux perf events.
The report includes counter scheduling coverage; check it before comparing instruction or cycle counts.

The 22 cases separate direct lookup, validation, resident commands, observed commands, allocation, and refill.
The ordinary command cases call `Model::execute_response`, as the emulator does with metrics disabled.
The observed field-read case includes access-log allocation and destruction, but excludes emulator counter updates.
Fixture setup is outside measurement. Every case checks for command or lookup failures.

All fixtures use 65,536 pager entries. Direct lookups permute either 256 or 65,536 resident identities.
Resident commands repeatedly access one selected object and its hot RAM.
Transfer samples each use a fresh model and 32,768 operations, with RAM pages touched before measurement.
Allocations use unique identities and sufficient space to avoid eviction or collection.
Refills alternate two colliding identities, so every fetch evicts and reloads an object.
Their backing store contains only two records; this measures a hot in-memory store, not disk latency.
The dirty-refill case measures a **write-and-fetch pair**. Other command cases measure one command per operation.
Transfer measurements include the request records that the current response-only transfer path still creates.
None of these cases includes CPU dispatch, JIT execution, collector microcode, or display work.

Measurements on an ARM Cortex-X925, pinned to CPU 7, with Rust 1.98.1 and `micromeasure` 0.16.0:

| Case | Median ns/operation | Host instructions/operation |
| --- | ---: | ---: |
| Direct resident lookup, 256 objects | 2.8 | 40 |
| Direct resident lookup, 65,536 objects | 5.7 | 40 |
| Field-read command validation | 6.8 | 45 |
| No-op command | 14.1 | 204 |
| Resident fetch hit | 12.6 | 189 |
| Resident field read | 18.1 | 296 |
| Prepared field read | 18.0 | 295 |
| Resident field write | 22.0 | 333 |
| Field read with access log | 26.3 | 514 |
| Allocate 16 words | 75.3 | 1,968 |
| Clean refill, 16 words | 178.5 | 4,803 |
| Write and dirty refill, 16 words | 410.8 | 10,238 |
| Allocate 256 words | 431.9 | 9,379 |
| Clean refill, 256 words | 1,042.5 | 21,483 |

These are one run's medians against implementation `fd5d437`, with the benchmark additions.
All hardware counters had 100% scheduling coverage. Each case collected approximately one second of measured work.
Transfer timings varied more between processes than resident timings; repeat runs when assessing changes.
Microbenchmark costs are not additive, and these host timings do not predict FPGA performance.

A separate Linux perf profile ran 500 million Smalltalk steps with the JIT and 16,777,216 RAM words.
It used the same core, no window, no detailed OBJEKT counters, and a release build with debug information.
Cycle sampling began after a 1.5-second delay to exclude startup on this host.
The run retired approximately 25.6 million microinstructions per second and performed no collections.
`Model::execute_response` accounted for about **17.2% of sampled CPU time**, including its callees.
Resident `transition` accounted for 8.3% of total time, and command validation accounted for 4.4%.
These are overlapping profile shares, not separate costs to add together.
A separate metrics-enabled run had 25,287,341 stored-reference fetch hits and 3,268 misses: a 99.99% hit rate.

The strongest candidates for improvement are repeated static command validation and resident state publication.
The profile attributes substantial time to copying the next register state back into the model.
This is the small architectural `State`, not a copy of the object heap.
Any change must preserve old-operand behavior and leave state unchanged on resident command failure.
The relevant code is in [command.rs](../crates/rekursiv-model/src/command.rs),
[datapath.rs](../crates/rekursiv-model/src/datapath.rs), and
[pager.rs](../crates/rekursiv-model/src/pager.rs).

### Input and presentation

The initial keyboard profile uses unshifted US ASCII and separate modifier transitions. The frontend
maps left/right Shift to 136/137, Control to 138, and Caps Lock to 139. Left, middle, and right
mouse buttons use codes 130, 129, and 128. These values match `InputState class>>initialize` and
`InputSensor` in the pinned Xerox V2 sources. They belong to the frontend profile, not the CPU or
object-memory implementation. Other keyboard layouts, text composition, wheel input, and unmapped
function keys remain unsupported.

Keyboard and mouse-button events preserve short press/release pairs. Focus loss releases held keys
and buttons. Caps Lock toggles a virtual lock state. Both physical Control keys share one guest
state. Mouse coordinates follow the scaled display rectangle, with clipping at its edges. Published
cursor pixels invert the display pixels at the device cursor position. The workstation starts with
cursor tracking enabled; guest software can explicitly unlink the cursor from the mouse. The CPU
runs on a worker thread. The main thread owns the window and checks display snapshots at up to 60
Hz. The event loop waits between checks and wakes immediately for window or input events. Slow
presentation does not pause the CPU. The worker checks queued input, host clocks, and shutdown
requests approximately every 2 ms. Physical input still waits for the window backend to receive it.
Input batches preserve key order and capture timestamps. A full frontend queue stops execution with
an error. The exit report also counts overruns in the guest's separate, bounded input FIFO.

The worker publishes immutable peripheral snapshots through one shared slot, at up to 60 Hz. The
window takes the newest snapshot; intermediate snapshots can be replaced without building a frame
queue. Published bitmaps share storage until their contents change. Identical guest publications
reuse the previous snapshot's pixels. The window converts pixels only when the display, cursor, or
visible cursor position changes. It uploads only changed RGB pixels, except when a resize, focus
regain, or repaint requires another submission. Winit supplies repaint and resize events directly;
the frontend needs no separate X11 repaint connection.

The window report separates initialization, snapshot checks, pixel conversion, buffer
acquisition/submission, scaling, callbacks, and event-loop waiting. Each timing category reports its
call count, total duration, mean, and maximum. Initialization includes event-loop and window/surface
creation. Presentation checks and unchanged snapshots have separate counters. Window callback time
includes redraw work; it overlaps the submission and scaling categories. Event-loop waiting includes
the time blocked waiting for events or the next snapshot deadline, plus backend dispatch overhead
before callbacks. The surface-pixel byte count measures submitted buffers after scaling, before
protocol overhead or compression. It is not network traffic. CPU worker input/clock and snapshot
costs are reported separately. Window timings overlap CPU execution.

Mouse coordinates arrive through window events. Snapshot checks do not issue synchronous pointer
queries or submit unchanged pixels. Under forwarded X11, changed frames still require pixel
transfers. Softbuffer uses its ordinary X11 image path when shared memory is unavailable. The
frontend currently submits the full scaled surface for each changed frame, including cursor
movement. Event-driven input removes repeated polling round trips; it does not remove display
bandwidth costs.

## Architecture and validation

`rekursiv-model` supplies instruction and object-command semantics, including SoftFloat arithmetic.
The interpreter and RTL oracle share those semantics. The JIT independently emits arithmetic, stack, and fetch operations.
Differential tests compare those operations with the processor oracle.
The emulator adds an execution loop and a separate model of the OBJEKT maintenance datapath. It never calls the graph-walking `collect_ram` oracle. Collector
microcode chooses every root, mark, pager pass, body read, body write, and commit.

`rekursiv-devices` supplies the same external peripheral models to both executors. The Verilator
adapter drives their request/reply handshakes. The emulator waits for device completion before
instruction retirement. The executor prepares scalar values and indexed writes before it issues an
object or device request. Blocking instructions commit these writes only after success, preserving
all local destinations on failure. Launched prepared accesses commit local writes at acceptance and
defer object errors to the next barrier. Each indexed write uses the original operand and address;
preparation does not copy the register file, stacks, or roots. Native OBJEKT commands are validated
directly; they need no wire encoding and decoding.
With metrics disabled, resident OBJEKT commands return responses without allocating memory-access records.
Metrics and RTL comparisons retain those records through the observed command path.
Both paths share the same state transitions and fault checks.
External wire requests retain numeric-field validation and share the same command execution and fault handling. The GUI uses
[winit](https://docs.rs/winit/0.30/winit/) for window events and
[softbuffer](https://docs.rs/softbuffer/0.4/softbuffer/) for pixel presentation. Presentation needs
no GPU renderer. Guest microcode still performs all drawing and supplies the published bitmap. It
has no access to guest objects through the presentation helpers.

Headless clocks advance one millisecond per 1000 device ticks. Window clocks follow host elapsed
time and UTC. These modes deliberately produce different timing observations. Neither mode claims to
reproduce RTL cycle timing. Tests can schedule deterministic input through the shared device API.
Transaction history stays disabled in interactive sessions to avoid unbounded memory growth.

Run the emulator tests, including RTL comparisons:

```sh
cargo test --locked -p rekursiv-emulator
```

Run the original-image regression:

```sh
REKURSIV_ST80_DIR="$PWD/artifacts/st80" \
  cargo test --release --locked -p rekursiv-emulator --no-default-features \
  --test startup --test jit -- --ignored --nocapture
```

JIT tests compare 49,152 scalar and 16,384 stack/fetch instruction-state combinations with the processor oracle.
Other cases cover simultaneous destinations, block budgets, overlapping code edits, image replacement, deferred object errors, and device errors before retirement.
The pipeline tests compare JIT execution with RTL through stalls and collection.
A combined stack/fetch program checks old-state reads and pointer forwarding against RTL at each retirement.
An original-image regression compares JIT blocks with the interpreter through 5.12 million steps, including GC, BitBlt, and mouse input.

The allocation and paging tests compare registers and object state with RTL after each mutator
retirement. Other tests cover recovery failure, failed device writes, pixel clipping, keyboard
mapping, and microcode-driven input/display changes. Frontend tests compare threaded and direct
execution across collection with no snapshot consumer. They also check immutable frames, identical
publications, ordered input, shutdown, and fault reporting. Window tests cover physical key/button
transitions, focus loss, and matching display/input scaling through letterboxed viewports. A
saved-image cursor regression checks that the uploaded arrow follows mouse movement; a frontend test
preserves explicit guest unlink behavior. The original-image regression matches all 499 bytecodes in
Xerox's `trace2`. It also checks the RTL checkpoint: 2176 bytecode boundaries, three collections,
and two display publications before BitBlt. A second native test completes 32 BitBlts: 9870 bytecode
boundaries, 24 collections, and 33 display publications. Directed native and RTL tests independently
compare all Boolean rules, clipping, alignment, overlap, and refresh with expected pixels. Passing
these checks does not replace cycle-level RTL validation.
