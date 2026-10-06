//! Offline microcode assembly. This module does not interpret guest bytecodes.
use crate::{Error, Result};
use rekursiv_asm::{text, Word};

pub const SOURCE: &str = include_str!("../../../microcode/smalltalk/interpreter.uc");
pub const COLLECTOR_ENTRY: u16 = 384;

/// Assemble a root activation whose context and object graph are already
/// converted. Loading must finish while the processor is halted. The context
/// is both an explicit boot root and, after start, a tagged value register.
pub fn assemble(active_context: Word) -> Result<text::Assembly> {
    if !active_context.is_reference() {
        return Err(Error("active context must be a stored object".into()));
    }
    let assembly = text::assemble(
        SOURCE,
        0,
        &[("ACTIVE_CONTEXT", active_context.bits() as i64)],
    )
    .map_err(|e| Error(format!("Smalltalk microcode: {e}")))?;
    if assembly.code.keys().any(|&pc| pc >= COLLECTOR_ENTRY) {
        return Err(Error("interpreter overlaps the machine collector".into()));
    }
    Ok(assembly)
}
