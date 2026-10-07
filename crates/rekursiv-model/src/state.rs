//! OBJEKT registers and the prepared field-access latch.
#![deny(missing_docs)]

use rekursiv_asm::{Entry, Status, Word};

/// Address-generation pipeline register. Bounds/selection failures are latched
/// here and reported only when an instruction consumes this access. A speculative
/// prepare past the end of a loop is therefore harmless. Memory errors instead
/// leave this register (and all accompanying command effects) unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedAccess {
    /// Object reference captured when the access was prepared.
    pub reference: Word,
    /// Captured one-based field index, in the signed 40-bit index domain.
    pub index: i64,
    /// Resolved zero-based RAM word address; usable only when `status` is `Ok`.
    pub address: u32,
    /// Deferred lookup or bounds error, or `Ok` for a valid prepared access.
    pub status: Status,
}
impl Default for PreparedAccess {
    fn default() -> Self {
        Self {
            reference: Word::NIL,
            index: 0,
            address: 0,
            status: Status::NoSelection,
        }
    }
}

/// OBJEKT register state at a completed command boundary.
///
/// Combined controls read old operands and publish new values together. A saved
/// selection retains object identity; metadata consumers revalidate that identity
/// against the current pager before use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    /// Eight virtual registers that hold tagged words.
    pub vr: [Word; 8],
    /// Current signed 40-bit index. Object fields use indices one through size.
    pub index: i64,
    /// Auxiliary signed 40-bit index register.
    pub index_reg: i64,
    /// Last selected object snapshot. It can outlive its resident mapping.
    pub selected: Option<Entry>,
    /// Prepared access consumed by a later command with the prepared control set.
    pub prepared: PreparedAccess,
}
impl Default for State {
    fn default() -> Self {
        Self {
            vr: [Word::NIL; 8],
            index: 0,
            index_reg: 0,
            selected: None,
            prepared: PreparedAccess::default(),
        }
    }
}
