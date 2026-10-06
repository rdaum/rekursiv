//! Offline Smalltalk-80 Version 2 image conversion.
//!
//! This crate does not execute guest bytecodes, primitives, or garbage collection.
//! Its output is a set of ordinary OBJEKT backing records and boot metadata.
//! The representation contract is in `docs/smalltalk-image.md`.
pub mod layout;
pub mod source;
pub mod target;

use std::fmt;

pub const IMAGE_URL: &str = "https://archive.org/download/smalltalk-80/image.tar.gz";
pub const IMAGE_SHA256: &str = "cac3a2d9690e8353d9ccfd073b1199bd49b43b5989607032a06a185cd4f23a1c";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;

pub fn checksum(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

/// Import the exact distribution image. General format parsing is available
/// separately for fixtures and diagnostics, but is not a compatibility claim.
pub fn import_distribution(bytes: &[u8]) -> Result<target::Image> {
    let digest = checksum(bytes);
    if digest != IMAGE_SHA256 {
        return Err(Error(format!(
            "unsupported image: SHA-256 {digest}; expected Xerox Version 2 {IMAGE_SHA256}"
        )));
    }
    let source = source::Image::parse(bytes)?;
    let mut roots = layout::BOOT_ROOTS.to_vec();
    roots.push(source.initial_context()?);
    let converted = target::Image::convert(&source, &roots)?;
    converted.verify_against(&source)?;
    Ok(converted)
}
