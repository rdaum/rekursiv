mod guest_trace;
use eyre::{bail, ensure, Result};
use rekursiv_emulator::{boot, presentation, Step};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
#[cfg(feature = "window")]
mod frontend;
#[cfg(feature = "window")]
mod window;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut program = None;
    let mut smalltalk = None;
    let mut squeak = None;
    let mut headless = false;
    let mut jit = true;
    let mut limit = None;
    let mut memory = None;
    let mut pager_entries = boot::DEFAULT_PAGER_ENTRIES;
    let mut trace = None;
    let mut guest_trace = None;
    let mut frame = None;
    let mut stop = None;
    let mut when = None;
    let mut frames = None;
    let mut objekt_metrics = false;
    let mut cycle_config = None;
    let mut clock_mhz = 100.0_f64;
    let mut clock_given = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--microcode" => {
                program = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("--microcode requires a path"))?,
                ))
            }
            "--smalltalk" => {
                smalltalk = Some(PathBuf::from(args.next().ok_or_else(|| {
                    eyre::eyre!("--smalltalk requires a VirtualImage path")
                })?))
            }
            "--squeak" => {
                squeak = Some(PathBuf::from(args.next().ok_or_else(|| {
                    eyre::eyre!("--squeak requires a Squeak1.1.image path")
                })?))
            }
            "--headless" => headless = true,
            "--engine" => {
                jit = match args.next().as_deref() {
                    Some("jit") => true,
                    Some("interpreter") => false,
                    _ => bail!("--engine requires jit or interpreter"),
                };
            }
            "--objekt-metrics" => objekt_metrics = true,
            "--estimate-cycles" => {
                cycle_config = Some(match args.next().as_deref() {
                    Some("sram") => rekursiv_emulator::timing::Config::SRAM,
                    Some("dram") => rekursiv_emulator::timing::Config::DRAM,
                    _ => bail!("--estimate-cycles requires sram or dram"),
                });
            }
            "--clock-mhz" => {
                clock_mhz = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("--clock-mhz requires a frequency"))?
                    .parse()?;
                ensure!(
                    clock_mhz.is_finite() && clock_mhz > 0.0,
                    "clock MHz must be finite and positive"
                );
                clock_given = true;
            }
            "--pager-entries" => {
                pager_entries = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("--pager-entries requires a count"))?
                    .parse()?;
                boot::validate_pager_entries(pager_entries)?;
            }
            "--steps" => {
                limit = Some(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("--steps requires a count"))?
                        .parse::<u64>()?,
                )
            }
            "--memory-words" => {
                memory = Some(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("--memory-words requires a count"))?
                        .parse::<usize>()?,
                );
            }
            "--guest-trace" => {
                guest_trace =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        eyre::eyre!("--guest-trace requires a path")
                    })?));
            }
            "--trace" => {
                trace = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("--trace requires a path"))?,
                ))
            }
            "--frame" => {
                frame = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("--frame requires a PPM path"))?,
                ))
            }
            "--stop-at" => {
                stop = Some(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("--stop-at requires a label"))?,
                )
            }
            "--when" => {
                let value = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("--when requires Rn=VALUE"))?;
                let (register, value) = value
                    .split_once('=')
                    .ok_or_else(|| eyre::eyre!("--when requires Rn=VALUE"))?;
                let register: usize = register
                    .strip_prefix('R')
                    .or_else(|| register.strip_prefix('r'))
                    .ok_or_else(|| eyre::eyre!("expected R0 through R15"))?
                    .parse()?;
                ensure!(register < 16, "expected R0 through R15");
                let value = if let Some(hex) = value.strip_prefix("0x") {
                    u32::from_str_radix(hex, 16)?
                } else {
                    value.parse()?
                };
                when = Some((register, value));
            }
            "--frames" => {
                frames = Some(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("--frames requires a count"))?
                        .parse::<u64>()?,
                )
            }
            "--help" | "-h" => {
                println!("rekursiv-emulator [--microcode FILE | --smalltalk VirtualImage | --squeak Squeak1.1.image]\n  No program: run the interactive workstation microcode demo.\n  --headless          Run without a window (deterministic device clock)\n  --engine MODE       jit (default) or interpreter\n  --steps N           Stop after N steps (headless default 10000000; window unlimited)\n  --memory-words N    External RAM words (Smalltalk default 16777216; otherwise 131072)\n  --pager-entries N   Pager slots, power of two from 2 to 65536 (default 65536)\n  --estimate-cycles PROFILE  Estimate hardware cycles: sram or dram (example latencies)\n  --clock-mhz MHZ     Convert estimated cycles to time (default 100; does not pace execution)\n  --objekt-metrics   Report pager, transfer, allocation, and collector counters\n  --guest-trace FILE  Write bytecode and primitive boundaries\n  --trace FILE        Write retired micro-PCs and numeric state\n  --stop-at LABEL     Stop before the named microinstruction\n  --when Rn=VALUE     Stop only when this register also matches\n  --frame FILE        Save the last published display as a PPM\n  --frames N          Close after N presentation checks (smoke tests)\nClose the window to exit. Escape is delivered to the guest.");
                return Ok(());
            }
            _ => bail!("unknown option {arg}; use --help"),
        }
    }
    ensure!(
        usize::from(program.is_some())
            + usize::from(smalltalk.is_some())
            + usize::from(squeak.is_some())
            <= 1,
        "choose microcode, a Xerox image, or a Squeak image"
    );
    ensure!(
        when.is_none() || stop.is_some(),
        "--when requires --stop-at"
    );
    ensure!(!headless || frames.is_none(), "--frames requires a window");
    ensure!(
        !clock_given || cycle_config.is_some(),
        "--clock-mhz requires --estimate-cycles"
    );
    // A full pager keeps the image's working set resident. The small peripheral
    // demo heap cannot hold it; explicit --memory-words still wins for tests.
    let memory = memory.unwrap_or(if smalltalk.is_some() || squeak.is_some() {
        16_777_216
    } else {
        131_072
    });
    let mut loaded = if let Some(path) = squeak.as_ref() {
        boot::squeak_with_pager(&std::fs::read(path)?, memory, pager_entries)?
    } else if let Some(path) = smalltalk {
        boot::smalltalk_with_pager(&std::fs::read(path)?, memory, pager_entries)?
    } else {
        boot::microcode_with_pager(
            &if let Some(path) = program {
                std::fs::read_to_string(path)?
            } else {
                include_str!("../../../microcode/workstation.uc").into()
            },
            memory,
            pager_entries,
        )?
    };
    loaded.machine.objekt_metrics_enabled = objekt_metrics;
    if let Some(config) = cycle_config {
        loaded.machine.enable_cycle_estimate(config)?;
    }
    if jit {
        let started = Instant::now();
        let functions = loaded.machine.enable_jit()?;
        eprintln!(
            "JIT: {functions} native functions compiled in {:.3} s",
            started.elapsed().as_secs_f64()
        );
    }
    // Headless clocks advance by device ticks for reproducible replay. The
    // window adapter supplies elapsed wall time and uses an effectively stopped
    // divider; the same timer/FIFO registers and acknowledgement rules apply.
    let utc = if headless {
        0
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
    };
    loaded.machine.devices = presentation::workstation(utc, if headless { 1000 } else { u64::MAX });
    if let Some(path) = &squeak {
        loaded.machine.devices.display_bitmap = Some(rekursiv_devices::Bitmap::new(1024, 768));
        let name = path
            .file_name()
            .ok_or_else(|| eyre::eyre!("image has no filename"))?
            .to_string_lossy();
        let mut files =
            rekursiv_devices::Files::new(b"C:\\", format!("C:\\{name}").as_bytes(), b'\\');
        let directory = path.parent().unwrap_or(std::path::Path::new("."));
        for name in [name.as_ref(), "SqueakV1.sources", "Squeak1.1.changes"] {
            match std::fs::read(directory.join(name)) {
                Ok(bytes) => {
                    files
                        .contents
                        .insert(format!("/{name}").into_bytes(), bytes);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
        }
        loaded.machine.devices.files = Some(files);
    }
    let breakpoint = stop
        .map(|label| {
            loaded
                .symbols
                .get(&label)
                .copied()
                .ok_or_else(|| eyre::eyre!("unknown label {label}"))
        })
        .transpose()?;
    let breakpoint = breakpoint
        .map(|pc| -> Result<u16> {
            let pc = u16::try_from(pc)?;
            ensure!(
                loaded
                    .machine
                    .image()
                    .code
                    .get(pc as usize)
                    .is_some_and(Option::is_some),
                "breakpoint does not name populated code"
            );
            Ok(pc)
        })
        .transpose()?;
    let mut guest_trace = guest_trace
        .map(|path| guest_trace::GuestTrace::create(&path, &loaded.symbols))
        .transpose()?;
    let mut trace = trace
        .map(std::fs::File::create)
        .transpose()?
        .map(std::io::BufWriter::new);
    let limit = limit.unwrap_or(if headless { 10_000_000 } else { u64::MAX });
    let mut steps = 0;
    let mut run = |machine: &mut rekursiv_emulator::Machine| -> Result<bool> {
        if steps >= limit
            || (breakpoint == Some(machine.cpu.pc)
                && when.is_none_or(|(register, value)| machine.cpu.rf[register] == value))
        {
            return Ok(false);
        }
        if trace.is_none() && guest_trace.is_none() && breakpoint.is_none() {
            // Bound each batch so the window worker can service input and stop
            // requests promptly. Debugging keeps exact single-step observation.
            steps += machine.run_steps((limit - steps).min(256))?;
            return Ok(!machine.cpu.halted && !machine.cpu.service);
        }
        if let Some(trace) = &mut guest_trace {
            trace.observe(machine)?;
        }
        let pc = machine.cpu.pc;
        let gc = machine.recovering();
        let step = machine.step()?;
        steps += 1;
        if step == Step::Retired {
            if let Some(trace) = &mut trace {
                writeln!(
                    trace,
                    "{pc:04x} gc={} next={:04x} object={:010x} rf={:08x?}",
                    u8::from(gc),
                    machine.cpu.pc,
                    machine.cpu.object,
                    machine.cpu.rf
                )?;
            }
        }
        Ok(!matches!(step, Step::Halted | Step::Service(_)))
    };
    // Loading/conversion is complete. Time the execution loop separately
    // from presentation and time spent inspecting a stopped window.
    let started = Instant::now();
    let mut execution_time = Duration::ZERO;
    let result = if headless {
        let result = (|| {
            while run(&mut loaded.machine)? {}
            Ok(())
        })();
        execution_time += started.elapsed();
        result
    } else {
        #[cfg(feature = "window")]
        {
            window::run(&mut loaded.machine, &mut run, frames, &mut execution_time)
        }
        #[cfg(not(feature = "window"))]
        {
            bail!("this build has no window support; use --headless or enable the window feature");
        }
    };
    let elapsed = started.elapsed();
    if loaded.machine.cpu.halted && loaded.symbols.contains_key("primitive_unimplemented") {
        eprintln!(
            "Squeak stop: status={}, R0={}, method={:010x}, context={:010x}",
            loaded.machine.cpu.rf[15],
            loaded.machine.cpu.rf[0],
            loaded.machine.objekt.state.vr[7].bits(),
            loaded.machine.objekt.state.vr[0].bits()
        );
    }
    if let Some(trace) = &mut guest_trace {
        trace.flush()?;
    }
    if let Some(trace) = &mut trace {
        trace.flush()?;
    }
    eprintln!("PC {}: {} mutator instructions, {} collector instructions, {} collections, {} device requests; halted={}, service={}", loaded.machine.cpu.pc, loaded.machine.stats.retired, loaded.machine.stats.collector_retired, loaded.machine.stats.collections, loaded.machine.stats.device_requests, loaded.machine.cpu.halted, loaded.machine.cpu.service);
    eprintln!(
        "Execution: {:.3} s active, {:.3} s elapsed; {:.0} microinstructions/s active ({:.0}/s elapsed)",
        execution_time.as_secs_f64(), elapsed.as_secs_f64(),
        loaded.machine.stats.instructions_per_second(execution_time),
        loaded.machine.stats.instructions_per_second(elapsed),
    );
    if let Some(stats) = loaded.machine.jit_statistics() {
        eprintln!(
            "JIT execution: {} instructions in {} native blocks, {} single-word preparations",
            stats.block_instructions, stats.block_calls, stats.preparations
        );
    }
    if let Some(estimate) = loaded.machine.cycle_estimate() {
        eprintln!("{}", estimate.report(&loaded.machine.stats, clock_mhz)?);
    }
    if objekt_metrics {
        eprintln!(
            "{}",
            loaded.machine.stats.objekt.report(&loaded.machine.objekt)
        );
    }
    if let Some(path) = frame {
        let visible = loaded
            .machine
            .devices
            .display_bitmap
            .as_ref()
            .and_then(|d| d.visible.as_ref())
            .ok_or_else(|| eyre::eyre!("no display frame was published"))?;
        presentation::save_ppm(visible, &path)?;
    }
    result
}
