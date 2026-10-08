//! Squeak 1.1 image conversion, inspection and offline microcode assembly.
//!
//! Squeak's 32-bit image and VM contract differ from Xerox Version 2. Keep
//! source addresses, method headers, identity hashes and primitive numbers in
//! this language profile; none of them belong in OBJEKT or the device layer.
//! Reading an image is not a claim that the Xerox microcode can execute it.
pub mod audit;
pub mod image;
pub mod interpreter;
pub mod layout;
pub mod target;

/// Historical Windows distribution, including the image and VM Smalltalk source.
pub const ARCHIVE_URL: &str = "https://files.squeak.org/1.1/Squeak1.1.zip";
/// Checksum of the image inside the pinned distribution, not of the ZIP archive.
pub const IMAGE_SHA256: &str = "4903675c5f3a979b9804a35851e4c8e6dccc58cef172de85b59d86db37e1d049";
