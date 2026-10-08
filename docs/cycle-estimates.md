# Hardware cycle estimates

The native emulator can estimate processor cycles while it runs the normal microcode.
Enable the estimate for either interactive or headless execution:

```sh
cargo run --release --locked -p rekursiv-emulator -- \
  --squeak artifacts/squeak-1.1/Squeak1.1.image \
  --estimate-cycles dram --clock-mhz 100 --objekt-metrics
```

Add `--headless --steps 100000000` for a repeatable startup sample.
The exit report separates estimated hardware throughput from native emulator execution speed.
The estimate excludes bootstrap, image conversion, and JIT compilation.

## Timing assumptions

These profiles are examples, not specifications for a particular board.
All latencies use processor clocks.
Each transaction takes one acceptance cycle, the listed response latency, and any configured request wait.

| Profile | RAM response | Backing response | Device response | Request wait |
| --- | ---: | ---: | ---: | ---: |
| `sram` | 1 | 8 | 2 | 0 |
| `dram` | 12 | 40 | 2 | 0 |

RAM transactions transfer one 40-bit word.
Backing requests include metadata, individual words, and save commits.
The backing latency assumes an external memory adapter; it does not represent disk access time.
Device latency covers a register transaction, including display register writes.
Guest polling instructions count as ordinary execution.

The current RTL permits one outstanding transaction per channel.
Its transfer engine serializes RAM and backing requests, so the estimate adds their service costs.
It does not assume bursts or bandwidth that the RTL cannot use.
DRAM rows, banks, refresh, shared-bus contention, and display DMA are not modeled.

`--clock-mhz` converts cycles to seconds without changing the latency assumptions or pacing execution.
It does not establish that an FPGA can meet that clock frequency.
Library callers can supply other latencies through `timing::Config` and `timing::Channel`.

## Reading the report

The additive cycle categories sum to the estimated elapsed processor time:

- **Execute:** one cycle per local instruction or instruction attempt, including HOLD and GC entry/return.
- **Object issue:** one acceptance cycle per OBJEKT command.
- **Blocking object wait:** OBJEKT service and response cycles after acceptance.
- **Async join/wait:** the unhidden service tail and the required barrier join edge.
- **Numeric wait:** floating-point execution after the initial EXECUTE cycle.
- **Device wait:** request, response, and completion cycles after EXECUTE.
- **GC maintenance wait:** maintenance issue, RAM access where required, and response cycles.

Collector cycles are a subset of these categories, not an additional cost.
Throughput includes collector instructions; a separate mutator rate includes the time spent collecting in its denominator.
The report uses retired instructions for CPI, so faults, HOLD, and collector transitions can increase CPI without adding retirements.

RAM and backing service totals describe work, not additional elapsed time.
An asynchronous command can overlap local arithmetic, stack work, fetch controls, and floating-point execution.
The estimator records its completion time and waits only when a later instruction needs the result or reaches a barrier.
Even an already-completed reply needs one join cycle, as in LOGIK.

The hidden-service total covers joined commands.
If execution stops with a pending command, the report shows its remaining service cycles without forcing a join.
A ready reply can remain pending with zero remaining service cycles.

Most floating-point operations take three total cycles in the current RTL.
Divide and square root have operand-dependent latency.
The estimator charges conservative allowances of 32 cycles for binary32 and 60 for binary64, and reports their count.
Other than allocation pressure, object error paths also receive an approximation count.
Their issued traffic is counted, but their control-path timing is not calibrated.

## Scope and calibration

This is a passive execution-cost model, not a cycle-accurate emulator.
It observes the same command outcomes used by the functional executor.
The JIT accounts for local blocks in batches; object, floating-point, device, and collector paths account for their own costs.
Enable it before execution with `Machine::enable_cycle_estimate`, then inspect `Machine::cycle_estimate`.
The public types have Rust API documentation.

Guest clocks, interrupts, and input retain their existing behavior.
Changing a timing profile therefore compares costs for the same execution, without changing the program's path.
Hardware timers can change that path on a real board, particularly in polling loops.
Use the estimates to compare workloads and latency assumptions, not to promise desktop responsiveness.

The estimator is disabled by default.
When enabled, it records OBJEKT request effects, which adds native execution overhead.
Use a separate run without instrumentation to measure emulator performance.

The [calibration tests](../crates/rekursiv-emulator/tests/timing.rs) compare estimates directly with RTL clock counts:

```sh
cargo test --release --locked -p rekursiv-emulator --test timing -- --nocapture
```

These tests remove the harness's artificial command and response gates.
They cover resident accesses, cached first-word reads, allocation, eviction/refill, directory lookup, exchange, and device I/O.
They also cover asynchronous overlap, floating-point conversion, explicit collection, and collection after allocation pressure.
They check multiple external latencies, JIT/interpreter agreement, and execution-state preservation.
Verilator still validates the actual handshake behavior; the estimator does not replace those tests.

The calibration harness has 16 pager entries to force transfer paths.
Workload estimates use the emulator's configured pager capacity and actual traffic.
The desktop default is 65536 entries; small-pager stress-test throughput is not a desktop prediction.

## Squeak startup sample

A 100-million-step headless Squeak 1.1 run uses 65536 pager entries and 16777216 RAM words.
It retires 85,341,492 mutator instructions and 14,658,506 collector instructions, with one collection.
Both profiles produce the same framebuffer and instruction counts.

| Example profile | Estimated cycles | CPI | Total M instructions/s at 100 MHz | Mutator M instructions/s |
| --- | ---: | ---: | ---: | ---: |
| SRAM | 138,429,783 | 1.384 | 72.24 | 61.65 |
| DRAM | 191,473,003 | 1.915 | 52.23 | 44.57 |

The DRAM estimate includes 51,290,984 blocking object wait cycles and 28,309,155 GC maintenance wait cycles.
Collector execution takes 22.44% of the estimated elapsed time, including its local instructions.
No asynchronous commands, divide/square-root operations, or uncalibrated object error paths occur in this sample.
These figures describe a startup interval under the example latencies, not measured FPGA performance or interactive latency.
