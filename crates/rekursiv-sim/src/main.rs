use eyre::{bail, Result};
use rekursiv_sim::{programs, runtime, trace_path, Harness, Timing};
fn main() -> Result<()> {
    let mut example = String::from("field-access");
    let mut trace = None;
    let mut microcode = None;
    let mut timing = Timing::default();
    let mut store_delay = None;
    let mut store_latency = None;
    let mut list_names = false;
    let mut listing = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--example" => {
                example = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("--example needs a name"))?
            }
            "--microcode" => {
                microcode = Some(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("--microcode needs a file"))?,
                );
            }
            "--trace-file" => {
                trace = Some(trace_path(
                    &args
                        .next()
                        .ok_or_else(|| eyre::eyre!("--trace-file needs a path"))?,
                ))
            }
            "--memory-latency" => {
                timing.memory_latency = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing latency"))?
                    .parse()?
            }
            "--request-delay" => {
                timing.request_delay = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing request delay"))?
                    .parse()?
            }
            "--response-stall" => {
                timing.response_stall = args
                    .next()
                    .ok_or_else(|| eyre::eyre!("missing response stall"))?
                    .parse()?
            }
            "--store-delay" => {
                store_delay = Some(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("missing store delay"))?
                        .parse()?,
                )
            }
            "--store-latency" => {
                store_latency = Some(
                    args.next()
                        .ok_or_else(|| eyre::eyre!("missing store latency"))?
                        .parse()?,
                )
            }
            "--list" => list_names = true,
            "--listing" => listing = true,
            "--help" | "-h" => {
                println!("Usage: rekursiv-sim [--example NAME|all | --microcode FILE] [--memory-latency N] [--request-delay N] [--response-stall N] [--store-delay N] [--store-latency N] [--trace-file PATH] [--list] [--listing]");
                println!("Every example executes real RTL and checks it against the Rust model.");
                return Ok(());
            }
            _ => bail!("unknown argument: {arg}"),
        }
    }
    if list_names {
        for name in programs::NAMES {
            println!("{name}");
        }
        return Ok(());
    }
    if example != "all" && !programs::NAMES.contains(&example.as_str()) {
        bail!("unknown example: {example}");
    }
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, timing, trace.as_deref())?;
    if let Some(n) = store_delay {
        h.store_timing.request_delay = n;
    }
    if let Some(n) = store_latency {
        h.store_timing.memory_latency = n;
    }
    if let Some(path) = microcode {
        let source = std::fs::read_to_string(&path)?;
        let (image, entry) = rekursiv_sim::processor::assembly_image(&source)
            .map_err(|e| eyre::eyre!("{path}:{e}"))?;
        if listing {
            for (address, instruction) in image.code.iter().enumerate() {
                if let Some(i) = instruction {
                    print!("{address:04x}: ");
                    for lane in i.encode()?.into_iter().rev() {
                        print!("{lane:08x}");
                    }
                    println!();
                }
            }
        }
        h.load_processor(&image)?;
        h.start_processor(entry)?;
        let mut model = rekursiv_model::processor::Processor {
            pc: entry,
            ..Default::default()
        };
        let retired = h.run_processor(&image, &mut model, 1_000_000)?;
        println!("{path}: {retired} mutator instructions, {} machine collections; RTL/model agreement passed",h.stats.collections);
        h.flush_trace();
        return Ok(());
    }
    let names = if example == "all" {
        programs::NAMES.to_vec()
    } else {
        vec![example.as_str()]
    };
    for name in names {
        h.reset()?;
        let result = programs::run(&mut h, name)?;
        println!("{name}: {result}");
        if listing {
            for (i, (ports, response)) in h.commands.iter().enumerate() {
                println!(
                    "  {i:04}: {} -> {} {}",
                    ports.decode()?,
                    response.status,
                    response.data
                );
            }
        }
        println!("  {} commands, {} service operations, {} cycles, {} memory reads, {} memory writes; model/RTL agreement passed",
            h.stats.commands,h.stats.services,h.stats.cycles,h.stats.reads,h.stats.writes);
        println!(
            "  {} backing-store transactions, {} committed object saves",
            h.stats.store_transactions, h.stats.saved_objects
        );
        println!(
            "  {} compactions, {} collections",
            h.stats.compactions, h.stats.collections
        );
    }
    h.flush_trace();
    Ok(())
}
