//! Squeak 1.1 guest constants from the archived Interpreter and ObjectMemory.
//!
//! Indices are zero-based guest fields. An oop is either a byte address of a
//! base object header or a signed 31-bit SmallInteger tagged in bit zero.
use crate::{Error, Result};

/// Image format magic used by the supplied Squeak 1.1 snapshot.
pub const IMAGE_VERSION: u32 = 6502;
/// Number of special objects defined by ObjectMemory in this release.
pub const SPECIAL_OBJECT_COUNT: usize = 31;

/// Indices into the image's specialObjectsArray; these are not fixed oops.
pub mod special {
    pub const NIL: usize = 0;
    pub const FALSE: usize = 1;
    pub const TRUE: usize = 2;
    pub const SCHEDULER: usize = 3;
    pub const INTEGER_CLASS: usize = 5;
    pub const FLOAT_CLASS: usize = 9;
    pub const METHOD_CONTEXT_CLASS: usize = 10;
    pub const BLOCK_CONTEXT_CLASS: usize = 11;
    pub const DISPLAY: usize = 14;
    pub const COMPILED_METHOD_CLASS: usize = 16;
    pub const SPECIAL_SELECTORS: usize = 23;
    pub const COMPACT_CLASSES: usize = 28;
}

/// Decode the entire signed 31-bit range, without Xerox's 15-bit restriction.
pub fn small_integer(oop: u32) -> Result<i32> {
    if oop & 1 == 0 {
        return Err(Error(format!(
            "Squeak oop 0x{oop:08x} is not a SmallInteger"
        )));
    }
    Ok((oop as i32) >> 1)
}

/// CompiledMethod header payload, after removal of the SmallInteger tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodHeader(pub u32);

impl MethodHeader {
    /// Reject noninteger and negative headers before deriving array bounds.
    pub fn parse(oop: u32) -> Result<Self> {
        let value = small_integer(oop)?;
        if value < 0 {
            return Err(Error("negative Squeak method header".into()));
        }
        Ok(Self(value as u32))
    }
    /// Includes quick-return codes 256 and above, not just ordinary primitives.
    pub fn primitive(self) -> u16 {
        (self.0 & 511) as u16
    }
    /// Number of pointer literals following the method header.
    pub fn literal_count(self) -> usize {
        ((self.0 >> 9) & 255) as usize
    }
    /// Number of receiver arguments, excluding the receiver itself.
    pub fn argument_count(self) -> usize {
        ((self.0 >> 24) & 31) as usize
    }
    /// Arguments and local temporaries that precede the evaluation stack.
    pub fn temporary_count(self) -> usize {
        ((self.0 >> 18) & 63) as usize
    }
    /// Context capacity for temporaries and evaluation slots.
    pub fn frame_size(self) -> usize {
        if self.0 & (1 << 17) == 0 {
            12
        } else {
            32
        }
    }
    /// One-based guest byte offset, including the 32-bit literal prefix.
    pub fn initial_ip(self) -> usize {
        (self.literal_count() + 1) * 4 + 1
    }
}
