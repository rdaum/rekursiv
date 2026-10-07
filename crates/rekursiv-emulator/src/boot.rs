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
pub const COLLECTOR_ENTRY: u16 = 8064;
pub const DEFAULT_DISPLAY_SIZE: (u32, u32) = (1024, 768);
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
    load_smalltalk(bytes, memory_words, true)
}

/// Preserve the saved display configuration for Xerox trace/RTL comparisons.
pub fn smalltalk_saved_display(bytes: &[u8], memory_words: usize) -> Result<Loaded> {
    load_smalltalk(bytes, memory_words, false)
}

fn load_smalltalk(bytes: &[u8], memory_words: usize, configure_display: bool) -> Result<Loaded> {
    ensure!(
        rekursiv_smalltalk::checksum(bytes) == rekursiv_smalltalk::IMAGE_SHA256,
        "Smalltalk loader requires the pinned Xerox V2 image"
    );
    let mut source = source::Image::parse(bytes)?;
    let context = source.initial_context()?;
    if configure_display {
        configure_saved_display(&mut source, context)?;
    }
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

/// Offline configuration of the pinned snapshot, before any microcode runs.
/// SystemDictionary>>snapshotAs:thenQuit: saved a 640x100 Display to save disk
/// space, with its original height (480) in temporary 2. On resumption it sends
/// DisplayScreen displayHeight: height, which takes the width from Display.
/// Set those two inputs; the guest still allocates/registers its full display
/// and restores the desktop through its own displayExtent: implementation.
fn configure_saved_display(source: &mut source::Image, context: u16) -> Result<()> {
    let (width, height) = DEFAULT_DISPLAY_SIZE;
    let saved = source.objects.get_mut(&context).unwrap();
    ensure!(
        saved.pointer(layout::context::METHOD_OR_ARGUMENT_COUNT)? == 0x6b64
            && saved.pointer(layout::context::INSTRUCTION_POINTER)? == layout::integer_oop(144)?
            && saved.pointer(layout::context::TEMPORARY_START + 2)? == layout::integer_oop(480)?,
        "unexpected saved display restoration context"
    );
    let source::Body::Pointers(fields) = &mut saved.body else {
        unreachable!()
    };
    fields[layout::context::TEMPORARY_START + 2] = layout::integer_oop(height as i32)?;

    // Oop 0x033c is the Display association in this checksum-pinned image.
    let display = source.objects[&0x033c].pointer(1)?;
    let form = source.objects.get_mut(&display).unwrap();
    ensure!(
        form.pointer(1)? == layout::integer_oop(640)?
            && form.pointer(2)? == layout::integer_oop(100)?,
        "unexpected saved Display extent"
    );
    let bits = form.pointer(0)?;
    let source::Body::Pointers(fields) = &mut form.body else {
        unreachable!()
    };
    fields[1] = layout::integer_oop(width as i32)?;
    // Keep the short saved Form valid until the guest replaces it. Preserve
    // its pixels row by row rather than reinterpreting the old row stride.
    let source::Body::Words(words) = &mut source.objects.get_mut(&bits).unwrap().body else {
        eyre::bail!("saved Display bitmap is not a word object")
    };
    ensure!(
        words.len() == 40 * 100,
        "unexpected saved Display bitmap size"
    );
    let stride = width as usize / 16;
    let mut resized = vec![0; stride * 100];
    for (old, new) in words
        .as_chunks::<40>()
        .0
        .iter()
        .zip(resized.chunks_exact_mut(stride))
    {
        new[..40].copy_from_slice(old);
    }
    *words = resized;
    Ok(())
}
