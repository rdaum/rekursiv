//! Assemble a microprogram into a boot stream for a board's boot sequencer.
//!
//!   cargo run -p rekursiv-model --example boot_stream -- PROGRAM.uc OUT.mem CODE_WORDS PAGER_ENTRIES
//!
//! CODE_WORDS and PAGER_ENTRIES describe the board's machine. Unless the
//! program defines its own collector, the standard RAM collector is placed at
//! the top of the control store, sized for that configuration.
//!
//! The stream replays the halted programming interface that the simulator
//! drives, then starts the processor. One 64-bit record per line, in hex, for
//! `$readmemh`:
//!
//!   bits 63:62  1 = programming write, 2 = start, 0 = end of stream
//!   write:      57:56 space, 50:48 lane, 47:32 address, 31:0 data
//!   start:      15:0 entry point, 16 collection enabled, 47:32 collector entry
use rekursiv_model::processor::{Image, STACK_WORDS};
use std::fmt::Write as _;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [source, output, code_words, pager_entries] = args.as_slice() else {
        return Err("usage: boot_stream PROGRAM.uc OUT.mem CODE_WORDS PAGER_ENTRIES".into());
    };
    let code_words: usize = code_words.parse()?;
    let pager_entries: usize = pager_entries.parse()?;
    // LOGIK's root table: registers, stacks, two literals per control word, extras.
    let root_count = 20 + STACK_WORDS + 2 * code_words + 32;
    let assembly = rekursiv_asm::text::assemble(
        &std::fs::read_to_string(source)?,
        0,
        &[
            ("CODE_WORDS", code_words as i64),
            ("STACK_WORDS", STACK_WORDS as i64),
            ("PAGER_ENTRIES", pager_entries as i64),
            ("ROOT_COUNT", root_count as i64),
        ],
    )
    .map_err(|e| format!("{source}: {e:?}"))?;
    let mut image = Image::from_assembly(&assembly).map_err(|e| format!("{e:?}"))?;
    if image.collector_entry.is_none() {
        let length = rekursiv_asm::collector::program(0, pager_entries, root_count)
            .map_err(|e| format!("{e:?}"))?
            .len();
        let entry = code_words
            .checked_sub(length)
            .ok_or("control store too small for the collector")?;
        image = image
            .with_ram_collector(entry as u16, pager_entries)
            .map_err(|e| format!("collector at {entry} overlaps the program: {e:?}"))?;
        // with_ram_collector sizes the root scan for the simulator's store.
        let code = rekursiv_asm::collector::program(entry as u16, pager_entries, root_count)
            .map_err(|e| format!("{e:?}"))?;
        for (slot, instruction) in image.code[entry..entry + code.len()].iter_mut().zip(code) {
            *slot = Some(instruction);
        }
    }
    if let Some(last) = image.code.iter().rposition(Option::is_some) {
        if last >= code_words {
            return Err(
                format!("program uses control word {last}; the board has {code_words}").into(),
            );
        }
    }

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
    let collector = image
        .collector_entry
        .map_or(0, |entry| 1 << 16 | u64::from(entry) << 32);
    records.push(2 << 62 | collector | u64::from(assembly.entry.unwrap_or(0)));

    let mut text = String::new();
    for record in &records {
        writeln!(text, "{record:016x}")?;
    }
    std::fs::write(output, text)?;
    eprintln!("{} records", records.len());
    Ok(())
}
