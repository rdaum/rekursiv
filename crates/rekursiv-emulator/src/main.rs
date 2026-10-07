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
    let mut headless = false;
    let mut limit = None;
    let mut memory = None;
    let mut pager_entries = boot::DEFAULT_PAGER_ENTRIES;
    let mut trace = None;
    let mut frame = None;
    let mut stop = None;
    let mut when = None;
    let mut frames = None;
    let mut objekt_metrics = false;
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
            "--headless" => headless = true,
            "--objekt-metrics" => objekt_metrics = true,
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
                println!("rekursiv-emulator [--microcode FILE | --smalltalk VirtualImage]\n  No program: run the interactive workstation microcode demo.\n  --headless          Run without a window (deterministic device clock)\n  --steps N           Stop after N steps (headless default 10000000; window unlimited)\n  --memory-words N    External RAM words (Smalltalk default 16777216; otherwise 131072)\n  --pager-entries N   Pager slots, power of two from 2 to 65536 (default 65536)\n  --objekt-metrics   Report pager, transfer, allocation, and collector counters\n  --trace FILE        Write retired micro-PCs and numeric state\n  --stop-at LABEL     Stop before the named microinstruction\n  --when Rn=VALUE     Stop only when this register also matches\n  --frame FILE        Save the last published display as a PPM\n  --frames N          Close after N presentation checks (smoke tests)\nClose the window to exit. Escape is delivered to the guest.");
                return Ok(());
            }
            _ => bail!("unknown option {arg}; use --help"),
        }
    }
    ensure!(
        program.is_none() || smalltalk.is_none(),
        "choose microcode or a Smalltalk image"
    );
    ensure!(
        when.is_none() || stop.is_some(),
        "--when requires --stop-at"
    );
    ensure!(!headless || frames.is_none(), "--frames requires a window");
    // A full pager keeps the image's working set resident. The small peripheral
    // demo heap cannot hold it; explicit --memory-words still wins for tests.
    let memory = memory.unwrap_or(if smalltalk.is_some() {
        16_777_216
    } else {
        131_072
    });
    let mut loaded = if let Some(path) = smalltalk {
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
                    .image
                    .code
                    .get(pc as usize)
                    .is_some_and(Option::is_some),
                "breakpoint does not name populated code"
            );
            Ok(pc)
        })
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
