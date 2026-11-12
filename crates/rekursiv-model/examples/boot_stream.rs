//! Assemble a microprogram into a boot stream for a board's boot sequencer.
//!
//!   cargo run -p rekursiv-model --example boot_stream -- PROGRAM.uc OUT.mem
//!
//! The stream replays the halted programming interface that the simulator
//! drives, then starts the processor. One 64-bit record per line, in hex, for
//! `$readmemh`:
//!
//!   bits 63:62  1 = programming write, 2 = start, 0 = end of stream
//!   write:      57:56 space, 50:48 lane, 47:32 address, 31:0 data
//!   start:      15:0 entry point
//!
//! The image has no collector, so the board leaves collection disabled.
use rekursiv_model::processor::Image;
use std::fmt::Write as _;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [source, output] = args.as_slice() else {
        return Err("usage: boot_stream PROGRAM.uc OUT.mem".into());
    };
    let assembly = rekursiv_asm::text::assemble(&std::fs::read_to_string(source)?, 0, &[])
        .map_err(|e| format!("{source}: {e:?}"))?;
    let image = Image::from_assembly(&assembly).map_err(|e| format!("{e:?}"))?;

    let mut records = Vec::new();
    let mut write = |space: u64, address: usize, lane: usize, data: u32| {
        records.push(
            1 << 62 | space << 56 | (lane as u64) << 48 | (address as u64) << 32 | u64::from(data),
        );
    };
    for (address, word) in image.roots.iter().enumerate() {
        write(3, address, 0, word.bits() as u32);
        write(3, address, 1, (word.bits() >> 32) as u32);
    }
    for (address, instruction) in image.code.iter().enumerate() {
        if let Some(i) = instruction {
            for (lane, data) in i
                .encode()
                .map_err(|e| format!("{e:?}"))?
                .into_iter()
                .enumerate()
            {
                write(0, address, lane, data);
            }
        }
    }
    for (address, word) in image.nam.iter().enumerate() {
        if let Some(word) = word {
            write(1, address, 0, *word as u32);
            write(1, address, 1, (*word >> 32) as u32);
        }
    }
    for (address, target) in image.map.iter().enumerate() {
        if let Some(target) = target {
            write(2, address, 0, u32::from(*target));
        }
    }
    records.push(2 << 62 | u64::from(assembly.entry.unwrap_or(0)));

    let mut text = String::new();
    for record in &records {
        writeln!(text, "{record:016x}")?;
    }
    std::fs::write(output, text)?;
    eprintln!("{} records", records.len());
    Ok(())
}
