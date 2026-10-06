//! Guest indices are zero-based. OBJEKT component indices are one-based.
//! Each target body starts with a raw format/length descriptor, so guest
//! pointer and word index zero occupies OBJEKT component two.
use crate::{Error, Result};
use rekursiv_asm::Word;

pub const NIL: u16 = 2;
pub const FALSE: u16 = 4;
pub const TRUE: u16 = 6;
pub const SCHEDULER_ASSOCIATION: u16 = 8;
pub const SMALL_INTEGER_CLASS: u16 = 12;
pub const ARRAY_CLASS: u16 = 16;
pub const METHOD_CONTEXT_CLASS: u16 = 22;
pub const BLOCK_CONTEXT_CLASS: u16 = 24;
pub const COMPILED_METHOD_CLASS: u16 = 34;
pub const SMALL_INTEGER_MIN: i32 = -16384;
pub const SMALL_INTEGER_MAX: i32 = 16383;

pub mod class {
    pub const SUPERCLASS: usize = 0;
    pub const METHOD_DICTIONARY: usize = 1;
    pub const INSTANCE_SPECIFICATION: usize = 2;
}
pub mod method_dictionary {
    pub const METHOD_ARRAY: usize = 1;
    pub const SELECTOR_START: usize = 2;
}
pub mod context {
    pub const SENDER: usize = 0;
    pub const INSTRUCTION_POINTER: usize = 1;
    pub const STACK_POINTER: usize = 2;
    pub const METHOD_OR_ARGUMENT_COUNT: usize = 3;
    pub const INITIAL_IP: usize = 4;
    pub const RECEIVER_OR_HOME: usize = 5;
    pub const TEMPORARY_START: usize = 6;
}
pub mod scheduler {
    pub const PROCESS_LISTS: usize = 0;
    pub const ACTIVE_PROCESS: usize = 1;
    pub const SUSPENDED_CONTEXT: usize = 1;
}

/// The complete fixed low-oop range from the Version 2 distribution is kept
/// in the machine's explicit root slots. Slot 26 will hold the active context;
/// slots 27..31 remain available to the interpreter. This is not a root for
/// every imported object: the ordinary guest graph supplies those edges.
pub const BOOT_ROOTS: [u16; 26] = [
    2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50,
    52,
];
pub const ACTIVE_CONTEXT_ROOT: usize = 26;

/// Every guest object uses a scanned reference. Raw scalar words inside a
/// scanned body remain non-references, including packed bytes and method code.
/// Uniform scan tags also avoid baking a guest object's format into its identity.
pub fn reference(oop: u16) -> Result<Word> {
    if oop == 0 || oop & 1 != 0 {
        return Err(Error(format!("invalid stored oop 0x{oop:04x}")));
    }
    Ok(Word::reference(u64::from(oop >> 1), true).unwrap())
}

pub fn small_integer(oop: u16) -> Result<i32> {
    if oop & 1 == 0 {
        return Err(Error(format!("oop 0x{oop:04x} is not a SmallInteger")));
    }
    Ok(i32::from((oop as i16) >> 1))
}

pub fn integer_oop(value: i32) -> Result<u16> {
    if !(SMALL_INTEGER_MIN..=SMALL_INTEGER_MAX).contains(&value) {
        return Err(Error(format!("SmallInteger overflow: {value}")));
    }
    Ok(((value as u16) << 1) | 1)
}

/// Dictionary lookup uses the unsigned high 15 bits of the original oop.
/// Do not substitute a hash of the physical address or the machine tag bits.
pub fn identity_hash(oop: u16) -> u16 {
    oop >> 1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodHeader(pub u16);
impl MethodHeader {
    pub fn parse(oop: u16) -> Result<Self> {
        small_integer(oop)?;
        Ok(Self(oop))
    }
    pub fn literal_count(self) -> usize {
        usize::from((self.0 >> 1) & 63)
    }
    pub fn flag(self) -> u16 {
        self.0 >> 13
    }
    pub fn temporary_count(self) -> usize {
        usize::from((self.0 >> 8) & 31)
    }
    pub fn large_context(self) -> bool {
        self.0 & 128 != 0
    }
    /// The guest context stores a one-based byte offset, including literals.
    pub fn initial_ip(self) -> usize {
        (1 + self.literal_count()) * 2 + 1
    }
}
