//! Prepare the Smalltalk-80 image for a board that boots from files.
//!
//!   cargo run --release -p rekursiv-emulator --example board_smalltalk -- \
//!       VirtualImage OUT_DIR PAGER_ENTRIES MEMORY_WORDS WIDTH HEIGHT
//!
//! Writes two files into OUT_DIR:
//!
//! - `REKURSIV.BOO`: the boot stream. It programs the interpreter and the
//!   collector, registers the compact class, reserves the image's identities
//!   and starts the processor.
//! - `records.bin`: every converted object, for the board's store tool
//!   (`mkstore`). Each record is little-endian: reference (8 bytes), class
//!   (8), size in words (4), cond (4), then the body words (8 each).
//!
//! The machine fetches its starting context from the store, so nothing is
//! installed in object memory beforehand.
use eyre::{ensure, eyre, Result};
use rekursiv_emulator::boot;
use rekursiv_model::boot_stream;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [image, out, pager, memory, width, height] = args.as_slice() else {
        return Err(eyre!(
            "usage: board_smalltalk VirtualImage OUT_DIR PAGER_ENTRIES MEMORY_WORDS WIDTH HEIGHT"
        ));
    };
    let loaded = boot::smalltalk_with_display(
        &std::fs::read(image)?,
        memory.parse()?,
        pager.parse()?,
        (width.parse()?, height.parse()?),
    )?;
    let machine = &loaded.machine;
    let entry = u16::try_from(loaded.symbols["start"])?;
    ensure!(
        machine.image().code.len() <= 8192,
        "control store too large"
    );

    let mut services = vec![boot_stream::reserve_identities(
        machine.objekt.next_identity,
    )];
    for (code, class) in machine.objekt.classes.iter().enumerate() {
        if let Some(class) = class.get() {
            services.push(boot_stream::compact_class(code as u8, class));
        }
    }
    let records = boot_stream::records(machine.image(), entry, &services).map_err(|e| eyre!(e))?;
    std::fs::create_dir_all(out)?;
    std::fs::write(format!("{out}/REKURSIV.BOO"), boot_stream::bytes(&records))?;

    let mut bytes = Vec::new();
    let mut words = 0;
    for record in machine.objekt.store.records.values() {
        bytes.extend(record.reference.bits().to_le_bytes());
        bytes.extend(record.class.bits().to_le_bytes());
        bytes.extend((record.body.len() as u32).to_le_bytes());
        bytes.extend(u32::from(record.cond).to_le_bytes());
        for word in &record.body {
            bytes.extend(word.bits().to_le_bytes());
        }
        words += record.body.len();
    }
    std::fs::write(format!("{out}/records.bin"), bytes)?;
    println!(
        "{} boot records ({} services); {} objects, {words} words; identities from {}",
        records.len(),
        services.len(),
        machine.objekt.store.records.len(),
        machine.objekt.next_identity
    );
    Ok(())
}
