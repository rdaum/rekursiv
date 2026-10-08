//! Halted Squeak loading. Guest execution and allocation belong to microcode.
use super::{validate_pager_entries, Loaded, DEFAULT_PAGER_ENTRIES};
use crate::Machine;
use eyre::{ensure, Result};
use rekursiv_asm::{Entry, Service, Status, Word};
use rekursiv_model::{processor::Image, store::Record};
use rekursiv_smalltalk::squeak::{self, image, interpreter, target};

/// Load the pinned Squeak image with the default pager.
pub fn squeak(bytes: &[u8], memory_words: usize) -> Result<Loaded> {
    squeak_with_pager(bytes, memory_words, DEFAULT_PAGER_ENTRIES)
}

/// Load the pinned snapshot without changing its saved desktop or context.
pub fn squeak_with_pager(
    bytes: &[u8],
    memory_words: usize,
    pager_entries: usize,
) -> Result<Loaded> {
    ensure!(
        rekursiv_smalltalk::checksum(bytes) == squeak::IMAGE_SHA256,
        "Squeak loader requires the pinned Squeak 1.1 image"
    );
    squeak_source(&image::Image::parse(bytes)?, memory_words, pager_entries)
}

/// Load a validated source graph, also used by synthetic microcode/RTL fixtures.
/// This API is offline: no CPU instruction has executed when it returns.
pub fn squeak_source(
    source: &image::Image,
    memory_words: usize,
    pager_entries: usize,
) -> Result<Loaded> {
    validate_pager_entries(pager_entries)?;
    let converted = target::Image::convert(source)?;
    let assembly = interpreter::assemble(source)?;
    let mut program = Image::from_assembly(&assembly)?
        .with_ram_collector(interpreter::COLLECTOR_ENTRY, pager_entries)?;
    // Assembly roots 26..31 are interpreter-private; roots 0..2 are boot ABI.
    for (dst, bits) in program.roots[..3].iter_mut().zip(&converted.roots[..3]) {
        *dst = Word::from_bits(*bits)?;
    }
    let mut machine = Machine::new(
        program,
        assembly.entry.unwrap_or(0),
        pager_entries,
        memory_words,
    )?;
    for record in &converted.records {
        let reference = Word::from_bits(record.reference)?;
        machine.objekt.store.records.insert(
            reference.identity()?,
            Record {
                reference,
                class: Word::from_bits(record.class)?,
                cond: false,
                body: record
                    .body
                    .iter()
                    .map(|&w| Word::from_bits(w))
                    .collect::<Result<_, _>>()?,
            },
        );
    }
    let context = target::reference(source.initial_context()?)?;
    let record = machine.objekt.store.records[&context.identity()?].clone();
    ensure!(
        record.body.len() <= memory_words / 2,
        "saved Squeak context exceeds semispace"
    );
    machine.objekt.memory[..record.body.len()].copy_from_slice(&record.body);
    let entry = Entry {
        reference: context,
        class: record.class,
        size: record.body.len() as u32,
        base: 0,
        representation: record.body[0],
        new: false,
        modified: false,
        cond: false,
    };
    for service in [
        Service::Install(entry),
        Service::ReserveIdentities(converted.next_identity),
        Service::CompactClass {
            code: 2,
            class: Word::from_bits(converted.small_integer_class)?,
        },
    ] {
        let status = machine.objekt.service(service, false).response.status;
        ensure!(
            status == Status::Ok,
            "Squeak boot service failed: {status:?}"
        );
    }
    Ok(Loaded {
        machine,
        symbols: assembly.symbols,
    })
}
