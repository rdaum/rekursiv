//! Assemble the Squeak microcode profile; this module never executes guest code.
use super::{image::Image, target};
use crate::{Error, Result};
use rekursiv_asm::text;

/// Control store above this address belongs to the language-neutral collector.
pub const COLLECTOR_ENTRY: u16 = 8064;

const SOURCES: &[&str] = &[
    include_str!("../../../../microcode/squeak/interpreter.uc"),
    include_str!("../../../../microcode/squeak/bytecodes.uc"),
    include_str!("../../../../microcode/squeak/sends.uc"),
    include_str!("../../../../microcode/squeak/blocks.uc"),
    include_str!("../../../../microcode/squeak/messages.uc"),
    include_str!("../../../../microcode/squeak/primitives.uc"),
    include_str!("../../../../microcode/squeak/dispatch.uc"),
    include_str!("../../../../microcode/squeak/integers.uc"),
    include_str!("../../../../microcode/squeak/indexed.uc"),
    include_str!("../../../../microcode/squeak/storage.uc"),
    include_str!("../../../../microcode/squeak/perform.uc"),
    include_str!("../../../../microcode/squeak/scheduler.uc"),
    include_str!("../../../../microcode/squeak/floats.uc"),
    include_str!("../../../../microcode/squeak/bitmaps.uc"),
    include_str!("../../../../microcode/squeak/devices.uc"),
    include_str!("../../../../microcode/squeak/events.uc"),
    include_str!("../../../../microcode/squeak/files.uc"),
    include_str!("../../../../microcode/squeak/bulk.uc"),
    include_str!("../../../../microcode/squeak/streams.uc"),
    include_str!("../../../../microcode/squeak/bitblt.uc"),
    include_str!("../../../../microcode/squeak/pixels.uc"),
    include_str!("../../../../microcode/squeak/words.uc"),
    include_str!("../../../../microcode/squeak/glyphs.uc"),
    include_str!("../../../../microcode/squeak/scanner.uc"),
    include_str!("../../../../microcode/squeak/refresh.uc"),
];

/// Resolve immutable boot constants from the image's special-object array.
/// Guest memory remains authoritative for mutable roots and all method lookup.
/// Only loading is performed in Rust; bytecodes and primitives run in microcode.
pub fn assemble(image: &Image) -> Result<text::Assembly> {
    let mut source = String::new();
    for index in 0..31 {
        let bits = target::value(image.special(index)?)?.bits();
        source.push_str(&format!(".equ SPECIAL_{index} = {bits}\n"));
    }
    source.push_str(".equ NIL = SPECIAL_0\n.equ FALSE = SPECIAL_1\n.equ TRUE = SPECIAL_2\n");
    for part in SOURCES {
        source.push_str(part);
        source.push('\n');
    }
    let active = target::reference(image.initial_context()?)?;
    let assembly = text::assemble(&source, 0, &[("ACTIVE_CONTEXT", active.bits() as i64)])
        .map_err(|e| Error(format!("Squeak microcode: {e}")))?;
    if assembly.code.keys().any(|&pc| pc >= COLLECTOR_ENTRY) {
        return Err(Error("Squeak interpreter overlaps the collector".into()));
    }
    Ok(assembly)
}
