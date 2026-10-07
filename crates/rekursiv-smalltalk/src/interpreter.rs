//! Offline microcode assembly. This module does not interpret guest bytecodes.
use crate::{Error, Result};
use rekursiv_asm::{text, Word};

pub const SOURCE: &str = include_str!("../../../microcode/smalltalk/interpreter.uc");
pub const SENDS_SOURCE: &str = include_str!("../../../microcode/smalltalk/sends.uc");
pub const BLOCKS_SOURCE: &str = include_str!("../../../microcode/smalltalk/blocks.uc");
pub const MESSAGES_SOURCE: &str = include_str!("../../../microcode/smalltalk/messages.uc");
pub const PRIMITIVES_SOURCE: &str = include_str!("../../../microcode/smalltalk/primitives.uc");
pub const INTEGERS_SOURCE: &str = include_str!("../../../microcode/smalltalk/integers.uc");
pub const INDEXED_SOURCE: &str = include_str!("../../../microcode/smalltalk/indexed.uc");
pub const STORAGE_SOURCE: &str = include_str!("../../../microcode/smalltalk/storage.uc");
pub const PERFORM_SOURCE: &str = include_str!("../../../microcode/smalltalk/perform.uc");
pub const SCHEDULER_SOURCE: &str = include_str!("../../../microcode/smalltalk/scheduler.uc");
pub const EVENTS_SOURCE: &str = include_str!("../../../microcode/smalltalk/events.uc");
pub const CLOCKS_SOURCE: &str = include_str!("../../../microcode/smalltalk/clocks.uc");
pub const SYSTEM_SOURCE: &str = include_str!("../../../microcode/smalltalk/system.uc");
pub const LOWSPACE_SOURCE: &str = include_str!("../../../microcode/smalltalk/lowspace.uc");
pub const DISK_SOURCE: &str = include_str!("../../../microcode/smalltalk/disk.uc");
pub const BITMAPS_SOURCE: &str = include_str!("../../../microcode/smalltalk/bitmaps.uc");
pub const INPUT_SOURCE: &str = include_str!("../../../microcode/smalltalk/input.uc");
pub const STREAMS_SOURCE: &str = include_str!("../../../microcode/smalltalk/streams.uc");
pub const FLOATS_SOURCE: &str = include_str!("../../../microcode/smalltalk/floats.uc");
pub const IDENTITIES_SOURCE: &str = include_str!("../../../microcode/smalltalk/identities.uc");
pub const BITBLT_SOURCE: &str = include_str!("../../../microcode/smalltalk/bitblt.uc");
pub const COLLECTOR_ENTRY: u16 = 8064;

/// Assemble a root activation whose context and object graph are already
/// converted. Loading must finish while the processor is halted. The context
/// is both an explicit boot root and, after start, a tagged value register.
pub fn assemble(active_context: Word) -> Result<text::Assembly> {
    if !active_context.is_reference() {
        return Err(Error("active context must be a stored object".into()));
    }
    let assembly = text::assemble(
        &format!(
            "{SOURCE}\n{SENDS_SOURCE}\n{BLOCKS_SOURCE}\n{MESSAGES_SOURCE}\n{PRIMITIVES_SOURCE}\n{INTEGERS_SOURCE}\n{INDEXED_SOURCE}\n{STORAGE_SOURCE}\n{PERFORM_SOURCE}\n{SCHEDULER_SOURCE}\n{EVENTS_SOURCE}\n{CLOCKS_SOURCE}\n{SYSTEM_SOURCE}\n{LOWSPACE_SOURCE}\n{INPUT_SOURCE}\n{BITMAPS_SOURCE}\n{DISK_SOURCE}\n{STREAMS_SOURCE}\n{FLOATS_SOURCE}\n{IDENTITIES_SOURCE}\n{BITBLT_SOURCE}"
        ),
        0,
        &[("ACTIVE_CONTEXT", active_context.bits() as i64)],
    )
    .map_err(|e| Error(format!("Smalltalk microcode: {e}")))?;
    if assembly.code.keys().any(|&pc| pc >= COLLECTOR_ENTRY) {
        return Err(Error("interpreter overlaps the machine collector".into()));
    }
    Ok(assembly)
}
