//! Offline loading adapters. Only this module knows the Smalltalk image format.
use crate::Machine;
use eyre::{ensure, Result};
use rekursiv_asm::{text, Entry, Service, Status};
use rekursiv_model::{
    processor::{Image, CODE_WORDS, STACK_WORDS},
    store::Record,
};
use rekursiv_smalltalk::{interpreter, layout, source, target};
use std::collections::BTreeMap;
pub const PAGER_ENTRIES: usize = 16;
pub const COLLECTOR_ENTRY: u16 = 3968;
pub struct Loaded {
    pub machine: Machine,
    pub symbols: BTreeMap<String, i64>,
}

pub fn microcode(source: &str, memory_words: usize) -> Result<Loaded> {
    let assembly = text::assemble(
        source,
        0,
        &[
            ("CODE_WORDS", CODE_WORDS as i64),
            ("STACK_WORDS", STACK_WORDS as i64),
            ("PAGER_ENTRIES", PAGER_ENTRIES as i64),
            (
                "ROOT_COUNT",
                (20 + STACK_WORDS + 2 * CODE_WORDS + 32) as i64,
            ),
        ],
    )?;
    let mut image = Image::from_assembly(&assembly)?;
    if image.collector_entry.is_none() {
        image = image.with_ram_collector(COLLECTOR_ENTRY, PAGER_ENTRIES)?;
    }
    Ok(Loaded {
        machine: Machine::new(
            image,
            assembly.entry.unwrap_or(0),
            PAGER_ENTRIES,
            memory_words,
        )?,
        symbols: assembly.symbols,
    })
}

/// Convert and verify before starting execution, then seed external storage.
/// Subsequent object faults, allocation, and collection execute in the machine.
pub fn smalltalk(bytes: &[u8], memory_words: usize) -> Result<Loaded> {
    ensure!(
        rekursiv_smalltalk::checksum(bytes) == rekursiv_smalltalk::IMAGE_SHA256,
        "Smalltalk loader requires the pinned Xerox V2 image"
    );
    let source = source::Image::parse(bytes)?;
    let context = source.initial_context()?;
    let converted = target::Image::convert(&source, &layout::BOOT_ROOTS)?;
    converted.verify_against(&source)?;
    let assembly = interpreter::assemble(layout::reference(context)?)?;
    let mut program =
        Image::from_assembly(&assembly)?.with_ram_collector(COLLECTOR_ENTRY, PAGER_ENTRIES)?;
    program.roots[..26].copy_from_slice(&converted.roots[..26]);
    let mut machine = Machine::new(
        program,
        assembly.entry.unwrap_or(0),
        PAGER_ENTRIES,
        memory_words,
    )?;
    for r in &converted.records {
        machine.objekt.store.records.insert(
            r.reference.identity()?,
            Record {
                reference: r.reference,
                class: r.class,
                cond: r.cond,
                body: r.body.clone(),
            },
        );
    }
    let root = converted
        .records
        .iter()
        .find(|r| r.source_oop == context)
        .unwrap();
    ensure!(
        root.body.len() <= memory_words / 2,
        "saved context exceeds the active semispace"
    );
    machine.objekt.memory[..root.body.len()].copy_from_slice(&root.body);
    let entry = Entry {
        reference: root.reference,
        class: root.class,
        size: root.body.len() as u32,
        base: 0,
        representation: root.body[0],
        new: false,
        modified: false,
        cond: root.cond,
    };
    for service in [
        Service::Install(entry),
        Service::ReserveIdentities(converted.next_identity),
        Service::CompactClass {
            code: 2,
            class: layout::reference(12)?,
        },
    ] {
        let status = machine.objekt.service(service, false).response.status;
        ensure!(status == Status::Ok, "boot service failed: {status:?}");
    }
    Ok(Loaded {
        machine,
        symbols: assembly.symbols,
    })
}
