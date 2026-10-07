//! Native execution fixtures shared by semantic tests and microcode profiles.
use super::*;
use rekursiv_emulator::Machine;

/// Build the same guest fixture for correctness tests and phase measurements.
/// Tiny correctness fixtures exhaust RAM deliberately; profiles use ample RAM
/// so collection does not obscure the microcode work being measured.
pub(super) fn machine(
    source: &source::Image,
    device: rekursiv_sim::device::Device,
    prepare: impl FnOnce(&mut target::Image),
    memory_words: usize,
    pager_entries: usize,
    exhaust: bool,
) -> Result<Machine> {
    let mut converted = target::Image::convert(source, &[ROOT])?;
    prepare(&mut converted);
    let assembly = interpreter::assemble(r(ROOT))?;
    let program = Program::from_assembly(&assembly)?
        .with_ram_collector(interpreter::COLLECTOR_ENTRY, pager_entries)?;
    let mut m = Machine::new(
        program,
        assembly.entry.unwrap(),
        pager_entries,
        memory_words,
    )?;
    for record in &converted.records {
        m.objekt.store.records.insert(
            record.reference.identity()?,
            Record {
                reference: record.reference,
                class: record.class,
                cond: record.cond,
                body: record.body.clone(),
            },
        );
    }
    let root = converted
        .records
        .iter()
        .find(|r| r.source_oop == ROOT)
        .unwrap();
    m.objekt.memory[..root.body.len()].copy_from_slice(&root.body);
    m.objekt.service(
        Service::Install(rekursiv_asm::Entry {
            reference: root.reference,
            class: root.class,
            size: root.body.len() as u32,
            base: 0,
            representation: root.body[0],
            new: false,
            modified: false,
            cond: false,
        }),
        false,
    );
    if exhaust {
        // Exhaust the first semispace. Refill and allocation
        // must retain the guest roots and full-width references.
        m.objekt.service(
            Service::Install(rekursiv_asm::Entry {
                reference: Word::reference(32000, false)?,
                class: r(16),
                size: 16,
                base: 240,
                representation: Word::ZERO,
                new: false,
                modified: false,
                cond: false,
            }),
            false,
        );
    }
    m.objekt
        .service(Service::ReserveIdentities(converted.next_identity), false);
    m.objekt.service(
        Service::CompactClass {
            code: 2,
            class: r(12),
        },
        false,
    );
    m.devices = device;
    Ok(m)
}
