//! Assemble a microprogram into a boot stream for a board's boot sequencer.
//!
//!   cargo run -p rekursiv-model --example boot_stream -- PROGRAM.uc OUT.mem CODE_WORDS PAGER_ENTRIES [none]
//!
//! CODE_WORDS and PAGER_ENTRIES describe the board's machine; programs can use
//! both as constants. Unless the
//! program defines its own collector, the standard RAM collector is placed at
//! the top of the control store, sized for that configuration. A final
//! `none` leaves the collector out and starts the machine with collection
//! disabled, so a full pager evicts to the backing store.
//!
//! An output named `*.boo` holds the 64-bit records as little-endian binary,
//! the form the MEGA65 board loads from its SD card. Any other name gets one
//! record per line in hex. `rekursiv_model::boot_stream` defines the records.
use rekursiv_model::processor::{Image, STACK_WORDS};
use std::fmt::Write as _;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (args, collector) = match args.as_slice() {
        [rest @ .., last] if last == "none" => (rest, false),
        all => (all, true),
    };
    let [source, output, code_words, pager_entries] = args else {
        return Err("usage: boot_stream PROGRAM.uc OUT.mem CODE_WORDS PAGER_ENTRIES [none]".into());
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
    if collector && image.collector_entry.is_none() {
        let length = rekursiv_asm::collector::program(0, root_count)
            .map_err(|e| format!("{e:?}"))?
            .len();
        let entry = code_words
            .checked_sub(length)
            .ok_or("control store too small for the collector")?;
        image = image
            .with_ram_collector(entry as u16)
            .map_err(|e| format!("collector at {entry} overlaps the program: {e:?}"))?;
        // with_ram_collector sizes the root scan for the simulator's store.
        let code = rekursiv_asm::collector::program(entry as u16, root_count)
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

    let records = rekursiv_model::boot_stream::records(&image, assembly.entry.unwrap_or(0), &[])?;

    // A .boo file is the binary form the board loads from its SD card.
    if output.to_ascii_lowercase().ends_with(".boo") {
        std::fs::write(output, rekursiv_model::boot_stream::bytes(&records))?;
    } else {
        let mut text = String::new();
        for record in &records {
            writeln!(text, "{record:016x}")?;
        }
        std::fs::write(output, text)?;
    }
    eprintln!("{} records", records.len());
    Ok(())
}
