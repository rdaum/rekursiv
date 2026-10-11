# Native microcode emulator

`rekursiv-emulator` executes Rekursiv microcode in a native Rust process, without Verilator. It uses
the same assembler, instruction definitions, and microcode files as the RTL machine. The window
presents device pixels and supplies keyboard and mouse input.

The emulator operates at instruction boundaries. An optional [cycle estimator](cycle-estimates.md)
estimates hardware throughput under explicit memory and device latency assumptions.
Verilator remains necessary for handshake, pipeline, stall, and reset validation.
Synthesis and timing analysis must establish the achievable FPGA clock frequency.

The CLI uses a [Cranelift](https://docs.wasmtime.dev/api/cranelift_jit/index.html) JIT by default.
It translates arithmetic, flag calculations, branches, stack controls, fetch controls, and validation checks into native code.
Native blocks execute up to 32 instructions per call, including resident OBJEKT commands.
Each instruction retains its device tick, retirement boundary, and fault checks.

Resident object instructions call the existing OBJEKT model from native blocks.
A Fetch miss stops the block before command issue; the general executor handles refill and collection.
Allocation and asynchronous commands use native preparation followed by the general executor.
OBJEKT metrics and cycle estimates use the general command path to retain complete accounting.
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

Floating-point, device, and root-access instructions use the checked interpreter.
Collector local instructions use validated scalar decoding or native blocks.
Collector maintenance operations execute individually through the recovery state machine.
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
| `--bytecodes N`             | Stop after exactly N guest dispatches; optional `--steps` safety cap              |
| `--estimate-cycles PROFILE` | Estimate hardware cycles with example `sram` or `dram` latencies             |
| `--clock-mhz MHZ`   | Convert estimated cycles to time; default 100, requires `--estimate-cycles`       |
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

Smalltalk and Squeak also report guest bytecodes/s in the window title and exit summary.
The title shows the recent active rate; the exit summary shows total counts and both active and elapsed rates.
A bytecode counts when the first control word at the guest's `decoded` boundary retires successfully.
Operand bytes, collector instructions, and work inside primitives do not add bytecodes to this count.
The count describes dispatches, not completed bytecodes or equal amounts of work across different bytecodes.

The loader supplies the boundary address; the executor does not decode Smalltalk or Squeak opcodes.
Native blocks encode the successful dispatch count in their existing return value.
Accounting updates the total only for blocks that report a dispatch, without micro-PC polling or a per-instruction callback.
Single-step and interpreter execution compare the configured address at successful mutator retirement.
There is a small accounting cost; guest tracing is not required.

A 500-million-step comparison against `6fca778` measured the counting cost on Cortex-X925 CPU 7.
Five alternating Squeak pairs measured 72.92 million microinstructions/s without counting and 72.36 million/s with counting (−0.8%).
Three Xerox pairs measured 64.12 million/s without counting and 64.37 million/s with counting (+0.4%).
Run-to-run ranges overlap; these results do not establish zero overhead on every workload.
All final processor counters and framebuffer hashes match their baselines.
Repeated runs report 2,010,226 Squeak bytecodes and 2,477,743 Xerox bytecodes at that step budget.

JIT startup reports its function count and compilation time separately.
The exit report counts instructions retired in native blocks and individual native preparations.
Preparations include attempts that later fault or enter collection, so they are not retirement counts.

### Graphics measurements

The running window title reports recent **FPS**, **display updates/s**, **kwords/s**
for the display, and **cursor updates/s**, alongside the execution rates. These graphics
rates use wall time; bytecode and microinstruction rates still use active CPU execution time.

FPS counts successful window surface submissions, including cursor motion and OS repaints.
It does not count presentation checks or guarantee compositor scanout at that rate. An idle
window can report zero FPS. `--frames N` retains its smoke-test meaning: N presentation checks,
including unchanged screens. The exit summary reports submissions, wall-time FPS, checks,
and unchanged checks separately.

Guest display/cursor updates count successful bitmap publications, including sparse patches,
identical content, and empty patches. Published words count the accepted 32-bit payload writes
in those committed uploads; duplicate offsets count as traffic. Abandoned/pending uploads and
rejected requests do not count. Words are not pixels or unique changed words. Display and cursor
traffic are separate, and pointer motion alone is not a cursor bitmap publication. Counters are
cumulative across replaced worker snapshots, even when the UI skips an unchanged image.

Both windowed and headless exit summaries report total updates and published words, with rates
per active and elapsed second. A headless run has no presentation FPS. These counters observe
peripheral work, without interpreting guest graphics operations or changing hardware cycle costs.

#### Repeatable graphics workloads

Run the synthetic Squeak workloads through ordinary guest sends and microcode:

```sh
cargo run --release --locked -p rekursiv-emulator --example graphics-bench -- \
  --iterations 8
cargo run --release --locked -p rekursiv-emulator --example graphics-bench -- \
  --iterations 8 --estimate-cycles sram
```

CSV output covers fill, aligned copy, unaligned copy with preserved edges, overlapping scroll,
and mapped monochrome glyphs into an 8-bit display. `--case fill|copy|unaligned|scroll|glyph`
selects one case; `--engine interpreter` provides a reference execution engine. Each case verifies
the guest bitmap and published display against a scalar pixel oracle before reporting success.
Copy, fill, and glyph operands alternate so repeated operations continue to change pixels.
Repeated scrolling eventually makes parts of the bitmap uniform, so compare identical iteration
counts. The benchmark accepts 1–1000 operations per case, defaulting to 8. For native timing,
use longer batches and repeated runs; the default cases are short and sensitive to host scheduling.

The fixed display is 64×32, RAM is 131,072 words, and the pager defaults to 65,536 entries.
`--pager-entries N` permits explicit paging-pressure experiments. The timed interval excludes
image construction/loading and JIT compilation, but includes guest display registration, sends,
drawing, publication, collection if needed, and final return. There is one initial display
publication in addition to the requested drawing operations. Reported costs per operation
amortize that setup; they are not isolated inner-loop costs or full-screen FPS.

CSV includes native operations/s, microinstructions and OBJEKT commands per operation, display
traffic, and (when enabled) estimated cycles/operation and operations/s at `--clock-mhz N`
(default 100). `--estimate-cycles dram` selects the alternate example memory timings. Estimation
adds host overhead: compare native rates with estimation disabled and hardware work with it
enabled. Estimated rates are not measurements from an FPGA board. A faster implementation may
publish fewer patches or words, so fixed workloads and verified pixels are the optimization
criterion, rather than maximizing the update counter.

With the `ac99f2c` microcode and the default eight operations/pager configuration, the initial
baseline is below. Interpreter and JIT SRAM runs match these work counts exactly; all cases
match the independent pixel oracle. Costs include the setup described above.

| Workload | Microinstructions/op | OBJEKT commands/op | SRAM cycles/op | DRAM cycles/op |
| --- | ---: | ---: | ---: | ---: |
| Fill | 63,979.25 | 6,122.25 | 83,284.25 | 105,766.25 |
| Aligned copy | 83,642.25 | 8,087.25 | 109,402.00 | 142,784.50 |
| Unaligned copy | 242,679.25 | 22,775.25 | 304,255.00 | 374,553.50 |
| Overlapping scroll | 292,352.25 | 29,144.25 | 370,178.50 | 458,845.75 |
| Mapped glyph | 47,693.25 | 5,453.25 | 65,627.75 | 88,239.25 |

The subsequent graphics microcode optimization uses a dedicated word loop for unmapped
Boolean operations, including differently aligned sources, and expands four mapped glyph
pixels at a time. Row addresses and rule selection are hoisted out of the word loop, and
halftone rows advance through a cached wrapped index. These are ordinary processor
instructions; there is no host raster shortcut or new hardware operation.

With the same eight-operation workload and memory configuration:

| Workload | Microinstructions/op | OBJEKT commands/op | SRAM cycles/op | SRAM speedup | DRAM cycles/op | DRAM speedup |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Fill | 21,418.25 | 4,618.25 | 37,715.25 | 2.208× | 60,197.25 | 1.757× |
| Aligned copy | 30,174.25 | 6,071.25 | 51,902.00 | 2.108× | 85,284.50 | 1.674× |
| Unaligned copy | 29,116.25 | 5,275.25 | 48,412.00 | 6.285× | 78,670.50 | 4.761× |
| Overlapping scroll | 39,227.50 | 9,270.50 | 69,036.75 | 5.362× | 112,221.75 | 4.089× |
| Mapped glyph | 30,958.25 | 4,433.25 | 46,852.75 | 1.401× | 69,464.25 | 1.270× |

Interpreter and JIT SRAM work counts agree. Scalar-oracle tests cover all Boolean rules,
packed depths 1–32, source alignments, clipped edges and aliases. RTL retirement comparisons
exercise word/glyph/scanner paths with memory stalls and pager refills. The archived image
still restores its desktop and opens a mouse menu.

These kernel gains translate to **1.101× SRAM / 1.055× DRAM** over one million Squeak
dispatches with 16,777,216 RAM words and 65,536 pager entries. Total estimated cycles fall
from 536,160,595 to 487,016,954 (SRAM), and 945,818,035 to 896,607,261 (DRAM).
Mutator instructions fall from 278,027,528 to 231,621,157; both versions execute 14,658,506
collector instructions and produce the same final framebuffer hash. This is an image
startup workload, not a sustained interactive FPS measurement. Mixed-depth mappings and
transparent rules still use the general pixel path, overlapping sources still require
snapshots, and display publication costs remain included.

With cycle estimation disabled, three alternating native pairs on a Ryzen 9 7940HS,
pinned to CPU 7 at the same million-dispatch budget, measured median throughput of
202,973 → 217,816 bytecodes/s (**1.073×**). All six framebuffer hashes match. These
host measurements exclude loading/JIT compilation and do not predict FPGA clock speed.

### OBJEKT pipeline improvements

The next pass, measured against `3d81c92`, changes the RTL as well as microcode:

- Resident RAM operations forward their completion to LOGIK on the commit edge.
  Backpressure captures the same reply in the existing response registers, without
  repeating memory writes or architectural effects. This saves one cycle when the
  consumer is ready, for both synchronous and asynchronous memory commands.
- Allocation/refill enters its FILL setup state once. Subsequent words advance directly
  to the next RAM write or backing read, saving one cycle per additional body word.
  External transactions remain serialized; these figures assume no burst support.
- Squeak's Boolean word kernel completes source fetches before reading the destination.
  That read also prepares the write address. Changed words use `prepared, launch` to
  avoid another destination fetch and overlap local loop bookkeeping. An OBJEKT barrier
  joins the write before further object work. Exit clears the prepared root. If RAM
  reports an error, the existing launch contract retains intervening local arithmetic
  and faults at the barrier; it never publishes the failed write or continues drawing.

The emulator's functional OBJEKT model has no new execution shortcut. Its passive cost
estimator was updated to match the changed state machines, and the timing suite compares
those estimates with actual RTL clock counts. The simulator samples response payloads
after driving external RAM inputs, so a forwarded reply cannot escape the oracle checks.

The same eight-operation graphics benchmark now measures:

| Workload | SRAM cycles/op | SRAM speedup | DRAM cycles/op | DRAM speedup |
| --- | ---: | ---: | ---: | ---: |
| Fill | 32,357.50 | 1.166× | 49,305.50 | 1.221× |
| Aligned copy | 45,931.00 | 1.130× | 75,699.50 | 1.127× |
| Unaligned copy | 43,173.00 | 1.121× | 68,589.50 | 1.147× |
| Overlapping scroll | 62,433.50 | 1.106× | 101,074.75 | 1.110× |
| Mapped glyph | 45,121.00 | 1.038× | 67,732.50 | 1.026× |

These speedups are additional to the preceding raster changes. Glyph microcode is
unchanged in this pass; its improvement comes from OBJEKT. Interpreter and JIT work
counts match, and the pixel oracle also passes with a 16-entry pager.

At one million dispatches, 16,777,216 RAM words and 65,536 pager entries:

| Guest | SRAM cycles before → after | SRAM speedup | DRAM cycles before → after | DRAM speedup |
| --- | ---: | ---: | ---: | ---: |
| Xerox V2 | 410,177,113 → 385,822,448 | 1.063× | 743,385,081 → 719,030,416 | 1.034× |
| Squeak 1.1 | 487,016,954 → 461,597,635 | 1.055× | 896,607,261 → 868,203,855 | 1.033× |

Both final framebuffer hashes match their respective baselines. Squeak hides 544,398
asynchronous service cycles across 272,199 writes in the SRAM profile; joining still
costs a cycle, so hidden service is not itself the net speedup.

Yosys 0.66 `synth_xilinx -family xcup -top objekt -noiopad` passes structural checks
for the default 16-entry/512-word configuration. Including OBJEKT's submodules, LUT1–LUT6
counts change from 26,829 to 27,210 (+1.4%); FDRE+FDSE remains 5,909. This is synthesis
of OBJEKT alone, without a board memory controller or placement/routing. Completion
forwarding introduces a combinational RAM-response-to-LOGIK path: clock timing must be
checked on the eventual board. These counts do not establish the feasibility or clock
speed of the 65,536-entry configuration used for image estimates.

Further transfer buffering/bursts need explicit ordering, cancellation and partial-failure
tests before the model can assume their throughput. Separating the metadata cache from
body residency remains a distinct hierarchy profile: it changes which replacements save
objects and therefore the observable NEW/MOD flags. This pass retains the direct-mapped
pager, resident-only probes, save/refill policy and flag behavior.

### Compare with a native Smalltalk VM

For a quick practical comparison, use the same portable Smalltalk work on the same host,
without requiring the same image version. `examples/smalltalk-workload.rs` assembles two
methods into a synthetic Squeak 1.1 graph and executes them through the normal microcode,
OBJEKT model and microinstruction JIT. The corresponding Smalltalk source is in
[`scripts/smalltalk-workload.st`](../scripts/smalltalk-workload.st); a stock Squeak 6.0
Cog VM compiles and executes it normally. Neither side supplies benchmark answers from
the host; both check the final result.

```sh
cargo build --release --locked -p rekursiv-emulator --example smalltalk-workload
taskset -c 7 target/release/examples/smalltalk-workload fib 3
taskset -c 7 target/release/examples/smalltalk-workload loop 3
# With a downloaded native bundle; pass absolute paths for the image and script:
taskset -c 7 /path/to/bundle/bin/squeak -headless /path/to/bundle/shared/Squeak.image \
  "$PWD/scripts/smalltalk-workload.st"
```

The native script sets author initials in the temporary image session to avoid a first-use
prompt, writes `rekursiv-benchmark.csv` beside the image, and exits without saving the image.
Errors write `rekursiv-error.txt` and exit unsuccessfully. It warms each method with ten
calls and measures three batches of 10,000 calls; Rekursiv defaults to three calls per
process. Compare time per completed call, not raw batch times. Rekursiv excludes fixture
loading and JIT compilation; native timing excludes startup, source compilation, warmup
and the full collection before each batch. Any collection during a timed batch is included.

On a Ryzen 9 7940HS, each process pinned to logical CPU 7 and run sequentially, three
trials measured the following median times per call. Rekursiv used revision `64be8ed`,
16,777,216 RAM words and 65,536 pager entries. The native VM was the production x86-64
Cog/Spur VM `202206021410` from the official
[Squeak 6.0-22104 Linux bundle](https://files.squeak.org/6.0/Squeak6.0-22104-64bit/Squeak6.0-22104-64bit-202206021410-Linux-x64.tar.gz).

| Workload | Result | Rekursiv | Native Cog | Cog speedup |
| --- | ---: | ---: | ---: | ---: |
| Recursive Fibonacci(18), base cases return 1 | 4,181 | 157.49 ms | 19.5 µs | 8,077× |
| Sum integers 0 through 9,999 with a while loop | 49,995,000 | 169.97 ms | 16.0 µs | 10,623× |

Rekursiv ranges were 156.69–160.92 ms and 168.54–170.12 ms respectively; native ranges
were 19.4–19.6 µs and 16.0–16.4 µs. No Rekursiv collections occurred. These deliberately
small workloads measure arithmetic, branches and recursive sends, not graphics, plugins
or general application performance. The source-level work and inputs match; image versions,
bytecode encodings, compilers and object representations differ. Cog compiles guest methods
directly to host code, whereas Rekursiv's JIT still executes the emulated microarchitecture.
This is a comparison of the complete software stacks, not an FPGA performance prediction
or a comparison of their reported bytecodes/s.

### Compare microcode revisions

Use equal guest-dispatch budgets when changing microcode. A fixed microinstruction budget would
execute more guest work in the optimized version. `--bytecodes N` stops immediately after the Nth
successful `decoded` retirement; `--steps` can supply a safety cap. Without an explicit step cap,
the bytecode limit takes precedence over the usual headless default. It requires a Smalltalk or
Squeak image.

```sh
python3 scripts/bench-guest.py --guest smalltalk \
  --baseline-binary artifacts/dispatch-optimization/baseline-bytecodes \
  --bytecodes 1000000 --runs 3 --cpu 7
python3 scripts/bench-guest.py --guest squeak \
  --baseline-binary artifacts/dispatch-optimization/baseline-bytecodes \
  --bytecodes 1000000 --runs 1 --estimate-cycles sram
```

Both binaries must support `--bytecodes`. The optimization baseline is `79dd69a` with only that CLI
stopping option backported, built in a separate checkout. The script alternates order, saves logs
and frames, records binary/image hashes, and reports median native throughput and framebuffer
agreement. With `--estimate-cycles sram|dram`, it also reports cycles per bytecode and estimated
hardware speedup. Native execution rates with estimation enabled include observer cost and must not
be compared with ordinary native rates.

The shared [Smalltalk core](../microcode/smalltalk-common/README.md) replaces profile-specific
copies of dispatch, variable access, stack operations, branches, arithmetic, returns and activation
helpers. Each profile supplies layout constants and its own 256-entry primitive jump table. Tables
live in the existing control store, leaving the bytecode NAM at 256 entries. Horizontal scheduling
combines VR reads with index setup and Fetch with independent address calculations. Context
initialization takes two loop words per slot instead of five; rotate/mask sequences replace repeated
shifts. Context materialization and scheduler checks still occur at every bytecode boundary.

At one million dispatches, with 16,777,216 RAM words and the default 65,536 pager entries, the
hardware estimates are:

| Guest      | Mutator words before → after | SRAM cycles/bytecode before → after | SRAM speedup | DRAM cycles/bytecode before → after | DRAM speedup |
| ---------- | ---------------------------- | ----------------------------------- | ------------ | ----------------------------------- | ------------ |
| Xerox V2   | 211,098,453 → 191,408,296    | 442.276 → 410.177                   | 1.078×       | 775.484 → 743.385                   | 1.043×       |
| Squeak 1.1 | 302,647,526 → 278,027,528    | 573.023 → 536.161                   | 1.069×       | 982.681 → 945.818                   | 1.039×       |

Cycle totals include collection: Squeak executes 14,658,506 collector instructions in both versions;
Xerox executes none in this interval. Both profiles produce the same framebuffer hashes before and
after. These are image-startup workloads, including drawing and primitive work, not steady-state
bytecode microbenchmarks. Deterministic device time advances with microinstructions, so equal
dispatch counts and matching frames alone do not establish equivalent guest execution. Guest traces
and semantic/RTL regressions provide separate checks.

These improvements do **not** achieve the 2× target. At a hypothetical 100 MHz, the SRAM estimates
correspond to about 243,797 Xerox and 186,511 Squeak dispatches/s. They use the existing passive
timing model's example memory latencies, exclude bootstrap, and are not measured board performance
or evidence of attainable FPGA clock frequency. DRAM waits dilute the instruction savings. In the
optimized Xerox SRAM run, object issue and blocking object waits still account for roughly 53% of
cycles. Reducing transfers/handshakes and guest runtime work matters more for the next hardware pass
than further host JIT tuning.

The JIT separately follows conditional fallthroughs with exact successful-prefix exits and forwards
scalar processor values across helper calls. Those changes improve host execution only; they do not
contribute to the hardware cycle savings. Three alternating native pairs, pinned to CPU 7
on an AMD Ryzen 9 7940HS with the same million-dispatch workloads and estimation disabled,
measured these medians (loading and JIT compilation excluded):

| Guest | Baseline dispatches/s | Optimized dispatches/s | Native speedup |
| --- | ---: | ---: | ---: |
| Xerox V2 | 237,811 | 268,999 | 1.131× |
| Squeak 1.1 | 186,253 | 214,118 | 1.150× |

Longer native traces increase one-time JIT compilation: the same runs measured median startup
compilation of 2.326 → 4.193 seconds for Xerox and 2.359 → 4.484 seconds for Squeak.
Short sessions can therefore take longer overall despite the improved active execution rate.

Validation includes matching opcode/primitive histograms across the million-dispatch runs,
identical guest traces through the first 1,000 dispatches for both images, the archived Xerox
startup checkpoint and Squeak desktop/menu test, and native/interpreter/RTL comparisons for
bytecodes, primitives, allocation, collection, graphics and memory stalls. All 90 selected Xerox
send/primitive tests passed, including two rerun after rebuilding a stale Verilator cache.
The timing suite also checks the estimator against RTL, including asynchronous overlap and GC.

For a label/opcode/primitive work histogram without supplying any guest semantics:

```sh
cargo run --release -p rekursiv-emulator --example dispatch-profile -- \
  smalltalk artifacts/st80/VirtualImage 1000000
```

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
The comparison below uses `0856f18` as the baseline and `1e65be2` as the optimized version.
Both use the JIT, deterministic device time, and 16,777,216 RAM words.
Each result is the median of three alternating pairs, pinned to Cortex-X925 CPU 7 (3.9 GHz maximum).

| Guest | Step budget | Baseline | Optimized | Speedup |
| --- | --- | --- | --- | --- |
| Squeak 1.1 | 500 million | 29.19 million/s | 73.45 million/s | 2.52× |
| Squeak 1.1 | 1 billion | 27.19 million/s | 68.06 million/s | 2.50× |
| Xerox V2 | 500 million | 25.66 million/s | 63.83 million/s | 2.49× |

Every run matches its baseline's final counters and framebuffer hash.
The 500-million-step Squeak run includes 17.59 million collector instructions and one completed collection.
The billion-step run includes 29.22 million collector instructions and three collections.
Xerox has no collections in this interval.
At 500 million steps, Squeak ranges from 72.23 to 73.84 million/s; Xerox ranges from 63.63 to 63.94 million/s.
At one billion steps, Squeak ranges from 68.04 to 68.18 million/s.
Rates exclude image loading and compilation.
Squeak JIT compilation takes about 1.86 seconds, compared with 0.76 seconds for the baseline.

On Linux, `taskset -c 7 python3 scripts/bench-emulator.py` pins an engine comparison to that core.
Choose a suitable CPU number on your host; mixed CPU types can affect comparisons.
The [CPU microbenchmark commands](#cpu-microbenchmarks) also support saved-binary comparisons.
These measurements describe native emulation, not measured FPGA throughput.

The optimizations reduce retirement-record copies and execute resident OBJEKT commands within native blocks.
A compact dispatch table keeps cold source snapshots separate from frequently accessed fields.
Collector local instructions use predecoded scalar execution, and idle device ticks skip unused handshake work.
Instruction boundaries, peripheral ticks, fault ordering, and collection semantics remain intact.

A `perf stat` comparison used the same 500-million-step Squeak workload on CPU 7.
These counters include image loading and JIT compilation, unlike the execution rates above.
The command uses the Cortex-X925 PMU; other processors need their own event names.

```sh
perf stat -e armv8_pmuv3_1/cycles/u,armv8_pmuv3_1/instructions/u,armv8_pmuv3_1/l1d_cache_refill/u,armv8_pmuv3_1/l1i_cache_refill/u,armv8_pmuv3_1/stall_backend/u,armv8_pmuv3_1/stall_frontend/u \
  -- taskset -c 7 target/release/rekursiv-emulator --headless \
  --squeak artifacts/squeak-1.1/Squeak1.1.image --steps 500000000
```

| Host counter | Baseline | Optimized |
| --- | --- | --- |
| User cycles | 69.74 billion | 33.48 billion |
| Retired host instructions | 285.84 billion | 171.20 billion |
| Backend stall cycles | 17.00 billion | 4.24 billion |
| Frontend stall cycles | 5.16 billion | 5.17 billion |
| L1 data-cache refills | 384.05 million | 329.43 million |
| L1 instruction-cache refills | 759.10 million | 1,027.76 million |

The larger native blocks reduce dispatch overhead but increase instruction-cache refills and compilation cost.
The profile points to instruction processing and dependent state updates, rather than bulk arithmetic suited to SIMD.
Remaining hotspots include device ticks, OBJEKT commands, and the general executor for unsupported native instructions.

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

The 23 cases separate direct lookup, validation, resident commands, observed commands, allocation, and refill.
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

A separate baseline Linux perf profile ran 500 million Smalltalk steps with the JIT and 16,777,216 RAM words.
It used the same core, no window, no detailed OBJEKT counters, and a release build with debug information.
Cycle sampling began after a 1.5-second delay to exclude startup on this host.
The run retired approximately 25.6 million microinstructions per second and performed no collections.
`Model::execute_response` accounted for about **17.2% of sampled CPU time**, including its callees.
Resident `transition` accounted for 8.3% of total time, and command validation accounted for 4.4%.
These are overlapping profile shares, not separate costs to add together.
A separate metrics-enabled run had 25,287,341 stored-reference fetch hits and 3,268 misses: a 99.99% hit rate.

The profile identified validation and resident state publication as optimization targets.
The resident datapath now borrows old operands and stages individual register changes until all checks succeed.
It avoids cloning and replacing the entire architectural `State` on each command.
Typed command boundaries use `Command::check` to validate a borrowed command without returning a copy.
Every validation check remains active; `Command::validate` retains its existing public behavior.
Combined controls still consume old operands, and resident command failures leave registers and RAM unchanged.
The relevant code is in [command.rs](../crates/rekursiv-model/src/command.rs),
[datapath.rs](../crates/rekursiv-model/src/datapath.rs), and
[pager.rs](../crates/rekursiv-model/src/pager.rs).

Fresh before/after microbenchmarks on the same core measured these command costs:

| Command | Before, ns | After, ns |
| --- | ---: | ---: |
| No-op | 15.0 | 6.5 |
| Resident field read | 18.2 | 11.3 |
| Prepared field read | 16.7 | 12.9 |
| Resident field write | 21.7 | 14.4 |
| Resident fetch hit | 12.9 | 12.0 |

Three alternating pairs of headless JIT runs compared the optimized emulator with `4ebe8c2`.
Both binaries used 16,777,216 RAM words and ran on CPU 7 with deterministic device time.
Rates below are medians in millions of retired microinstructions per second, excluding loading and JIT compilation.

| Step budget | Before | After | Throughput gain |
| --- | ---: | ---: | ---: |
| 50 million | 34.0 | 36.4 | 7.1% |
| 500 million | 25.6 | 27.9 | 9.3% |

Every pair produced matching final counters and framebuffer hashes. These runs performed no collections.
The workspace tests include RTL comparisons and collection tests; the original-image trace regression also passes its collection checkpoint.

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

## CPU microbenchmarks

Run the local-datapath and resident-object loops with `micromeasure`:

```sh
cargo bench --locked -p rekursiv-emulator --bench execution
```

The benchmarks exclude setup and JIT compilation. Each operation executes 256 microinstructions.
Cases compare the interpreter and JIT, with and without workstation device ticks.
The programs are in `microcode/bench/`.

At `1e65be2`, the same Cortex-X925 core measured these median rates:

| Loop | Interpreter | JIT | JIT with workstation ticks |
| --- | --- | --- | --- |
| Local datapath | 36.07 million/s | 237.83 million/s | 178.71 million/s |
| Resident objects | 26.89 million/s | 107.71 million/s | 94.86 million/s |

Each case uses 20 samples after warmup; compilation stays outside measurement.
These small loops fit in cache and do not represent complete guest performance.
The full-image comparisons above include transfers, collection, and guest device traffic.

To compare a saved executable against the current build, run:

```sh
python3 scripts/bench-emulator.py --guest squeak --baseline-binary /path/to/saved-emulator \
  --runs 3 --steps 500000000 --output artifacts/emulator-comparison
```

The script alternates execution order and checks final counters and framebuffer hashes.
Its report records executable hashes, image hash, CPU affinity, and median throughput.
