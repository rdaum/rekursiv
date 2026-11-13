//! The record stream a board's boot sequencer replays to program and start
//! the machine. It mirrors the halted programming interface that the
//! simulator drives.
//!
//! Each record is 64 bits:
//!
//! ```text
//! bits 63:62  1 = programming write, 2 = start, 3 = service, 0 = end of stream
//! write:      57:56 space, 50:48 lane, 47:32 address, 31:0 data
//! start:      15:0 entry point, 16 collection enabled, 47:32 collector entry
//! service:    61:58 OBJEKT service operation, 57:52 code, 39:0 value
//! ```
//!
//! A service record runs one halted OBJEKT service operation. The value is
//! presented as both the class and the representation operand, which covers
//! the two operations a boot needs: registering a compact class and reserving
//! identities.
use crate::processor::Image;
use rekursiv_asm::Word;

/// Register `class` for compact type `code`.
pub fn compact_class(code: u8, class: Word) -> u64 {
    3 << 62 | 2 << 58 | u64::from(code & 63) << 52 | class.bits()
}

/// Make allocation start at `next_identity` or later.
pub fn reserve_identities(next_identity: u64) -> u64 {
    3 << 62 | 8 << 58 | (next_identity & ((1 << 40) - 1))
}

/// Records that program `image`, run `services`, and start at `entry`.
pub fn records(image: &Image, entry: u16, services: &[u64]) -> Result<Vec<u64>, String> {
    let mut records = Vec::new();
    let mut write = |space: u64, address: usize, lane: usize, data: u32| {
        records.push(
            1 << 62 | space << 56 | (lane as u64) << 48 | (address as u64) << 32 | u64::from(data),
        );
    };
    for (address, word) in image.roots.iter().enumerate() {
        write(3, address, 0, word.bits() as u32);
        write(3, address, 1, (word.bits() >> 32) as u32);
    }
    for (address, instruction) in image.code.iter().enumerate() {
        if let Some(i) = instruction {
            for (lane, data) in i
                .encode()
                .map_err(|e| format!("{e:?}"))?
                .into_iter()
                .enumerate()
            {
                write(0, address, lane, data);
            }
        }
    }
    for (address, word) in image.nam.iter().enumerate() {
        if let Some(word) = word {
            write(1, address, 0, *word as u32);
            write(1, address, 1, (*word >> 32) as u32);
        }
    }
    for (address, target) in image.map.iter().enumerate() {
        if let Some(target) = target {
            write(2, address, 0, u32::from(*target));
        }
    }
    records.extend_from_slice(services);
    let collector = image
        .collector_entry
        .map_or(0, |entry| 1 << 16 | u64::from(entry) << 32);
    records.push(2 << 62 | collector | u64::from(entry));
    Ok(records)
}

/// The binary form a board loads from its SD card: little-endian records.
pub fn bytes(records: &[u64]) -> Vec<u8> {
    records.iter().flat_map(|r| r.to_le_bytes()).collect()
}
