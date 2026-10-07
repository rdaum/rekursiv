# Native microcode emulator

`rekursiv-emulator` executes Rekursiv microcode in a native Rust process, without Verilator.
It uses the same assembler, instruction definitions, and microcode files as the RTL machine.
The window presents device pixels and supplies keyboard and mouse input.

The emulator operates at instruction boundaries. It does not predict FPGA cycle counts or throughput.
Verilator remains necessary for handshake, pipeline, stall, reset, and synthesis validation.

## Run the workstation demo

Install Rust and a C compiler. The default window build also needs the platform window libraries.
On Debian or Ubuntu, install them with:

```sh
sudo apt-get install build-essential libx11-dev libxkbcommon-dev libwayland-dev
```

Run the included peripheral demo:

```sh
cargo run --release --locked -p rekursiv-emulator
```

The [demo microcode](../microcode/workstation.uc) uploads a stripe pattern and a cursor.
Mouse movement moves the cursor. A key or mouse-button press inverts the pattern.
Microcode consumes the input packets and uploads the replacement pixels.
The window adapter does not draw that pattern or choose its response to input.

Close the window to exit. Escape goes to the guest.
A stopped processor leaves its last frame visible, with its micro-PC in the window title.
The default window session has no instruction limit.

## Run existing microcode

Run the allocation and collection examples without a window:

```sh
cargo run --release --locked -p rekursiv-emulator --no-default-features -- \
  --headless --microcode microcode/allocation.uc
cargo run --release --locked -p rekursiv-emulator --no-default-features -- \
  --headless --microcode microcode/collection.uc --memory-words 512
```

The second example executes seven allocations and five collections.
The emulator executes the existing `ram-collector.uc` instructions for each collection.
The `--memory-words` value includes both semispaces. Each word carries 40 bits in a Rust `u64`.
The pager has 16 entries, matching the standard RTL test configuration.

A microcode file can supply its own `.collector` directive.
Otherwise, the loader installs the standard collector at microaddress 8064 and rejects overlap.
The loader supports the assembler's code, NAM, opcode-map, root, and entry directives.

## Run the Smalltalk image

Fetch the pinned Xerox V2 distribution:

```sh
python3 scripts/fetch-smalltalk-image.py
```

Run its saved process and watch it draw the desktop:

```sh
cargo run --release --locked -p rekursiv-emulator -- \
  --smalltalk artifacts/st80/VirtualImage --memory-words 1048576
```

For a reproducible run without a window, use:

```sh
cargo run --release --locked -p rekursiv-emulator --no-default-features -- \
  --headless --smalltalk artifacts/st80/VirtualImage --memory-words 1048576 \
  --steps 300000000 --frame artifacts/startup.ppm
```

The loader verifies and converts the image before execution. It then seeds backing storage and the boot context.
Runtime bytecode dispatch, method lookup, primitives, process scheduling, and collection execute through microcode.
The emulator does not supply missing Smalltalk operations through Rust callbacks.

Primitive 96 now draws and refreshes the display through microcode. Native startup draws the browser, transcript, and workspace.
BitBlt validates accessed words and uploads changed pixel groups. Disjoint copies need no temporary object.
Disk transfers and snapshot saving remain unfinished.
Low-space checks now collect before notification and recheck available space afterward.
The original image passes its former premature warning point with 1,048,576 RAM words.
A 200-million-step headless run completes 769 BitBlts and continues guest controller activity without a low-space signal.
This remains bounded execution evidence, not proof of complete desktop interaction.
The [rendering measurements](validation.md#bitblt-rendering-measurements) compare the same image checkpoint before and after the optimization.
The bounded startup regression establishes drawing progress, not complete interaction.
To stop at the earlier checkpoint before any drawing, add `--stop-at primitive_dispatch --when R0=96`.
The window presents complete published frames. It does not read a live Form from the heap.

## Controls and diagnostics

| Option | Behavior |
| --- | --- |
| `--microcode FILE` | Assemble and execute a standalone program |
| `--smalltalk FILE` | Convert and start the pinned Xerox V2 `VirtualImage` |
| `--headless` | Disable the window and use deterministic device time |
| `--steps N` | Stop after N instruction steps, including collection and Hold steps |
| `--memory-words N` | Set external RAM capacity; default 131072 words |
| `--stop-at LABEL` | Stop before an instruction at the named label |
| `--when Rn=VALUE` | Add a register condition to `--stop-at`; decimal or `0x` hexadecimal |
| `--trace FILE` | Record retired micro-PCs, collector mode, object result, and numeric registers |
| `--frame FILE` | Save the last published display as a PPM, without cursor composition |
| `--frames N` | Close a window session after N frames, for smoke tests |

Headless execution stops after ten million steps unless `--steps` supplies another limit.
Processor faults report the micro-PC, fault code, and object status where applicable.
A library caller can resume an explicit service break with `Machine::resume()`.
The CLI leaves service breaks stopped.

The exit summary reports active execution seconds, total elapsed seconds, and retired microinstructions per second for both intervals.
The rates include mutator and collector instructions. Hold steps and collector entry/return transitions do not count as retired instructions.
Active time includes instruction execution, emulated devices, recovery setup, and optional trace output. It excludes window presentation and stopped-window time.
Elapsed time includes those frontend costs. Both intervals exclude image loading/conversion and final trace flush/frame export.
The window title updates approximately once per second with the recent active execution rate.
Headless runs with fixed memory and step counts provide repeatable workloads. Trace output affects throughput.


The initial keyboard profile uses unshifted US ASCII and separate modifier transitions.
The frontend maps left/right Shift to 136/137, Control to 138, and Caps Lock to 139.
Left, middle, and right mouse buttons use codes 130, 129, and 128.
These values match `InputState class>>initialize` and `InputSensor` in the pinned Xerox V2 sources.
They belong to the frontend profile, not the CPU or object-memory implementation.
Other keyboard layouts, text composition, wheel input, and unmapped function keys remain unsupported.

The frontend preserves keyboard press/release pairs between frames and releases held keys after focus loss.
Caps Lock toggles a virtual lock state. Both physical Control keys share one guest state.
Mouse coordinates follow the scaled display rectangle, with clipping at its edges.
Published cursor pixels invert the display pixels at the device cursor position.
The CPU runs for approximately 16.7 ms between presentation and input updates.
Active execution has no additional frame-limit sleep. A stopped CPU uses a 60 Hz limit.

## Architecture and validation

`rekursiv-model` supplies instruction and object-command semantics, including SoftFloat arithmetic.
The RTL oracle and native emulator share those semantics. They are not independent implementations of every arithmetic operation.
The emulator adds an execution loop and a separate model of the OBJEKT maintenance datapath.
It never calls the graph-walking `collect_ram` oracle.
Collector microcode chooses every root, mark, pager pass, body read, body write, and commit.

`rekursiv-devices` supplies the same external peripheral models to both executors.
The Verilator adapter drives their request/reply handshakes. The emulator waits for device completion before instruction retirement.
A failed device reply preserves the instruction's local destinations.
The GUI uses [minifb](https://docs.rs/minifb/0.28.0/minifb/) for window presentation and physical input.
It has no access to guest objects through the presentation helpers.

Headless clocks advance one millisecond per 1000 device ticks.
Window clocks follow host elapsed time and UTC. These modes deliberately produce different timing observations.
Neither mode claims to reproduce RTL cycle timing.
Tests can schedule deterministic input through the shared device API.
Transaction history stays disabled in interactive sessions to avoid unbounded memory growth.

Run the emulator tests, including RTL comparisons:

```sh
cargo test --locked -p rekursiv-emulator
```

Run the original-image regression:

```sh
REKURSIV_ST80_DIR="$PWD/artifacts/st80" \
  cargo test --release --locked -p rekursiv-emulator --no-default-features \
  --test startup -- --ignored --nocapture
```

The allocation and paging tests compare registers and object state with RTL after each mutator retirement.
Other tests cover recovery failure, failed device writes, pixel clipping, keyboard mapping, and microcode-driven input/display changes.
The original-image regression matches all 499 bytecodes in Xerox's `trace2`.
It also checks the RTL checkpoint: 2176 bytecode boundaries, three collections, and two display publications before BitBlt.
A second native test completes 32 BitBlts: 9870 bytecode boundaries, 24 collections, and 33 display publications.
Directed native and RTL tests independently compare all Boolean rules, clipping, alignment, overlap, and refresh with expected pixels.
Passing these checks does not replace cycle-level RTL validation.
