//! Files the disk processor loads from the card's root directory.
//!
//! REKURSIV.FW is the main firmware: a 12-byte header (magic, payload length,
//! CRC-32 of the payload, all little-endian) followed by the code, which the
//! loader copies to `FIRMWARE_BASE` and enters at its first instruction.
//! REKURSIV.BOO is a boot stream: 8-byte little-endian boot_stream records.
use crate::disk::{Disk, FileDisk, Sector, SECTOR};
use crate::fat;

pub const FIRMWARE_NAME: &[u8; 11] = b"REKURSIVFW ";
pub const BOOT_NAME: &[u8; 11] = b"REKURSIVBOO";
pub const STORE_NAME: &[u8; 11] = b"REKURSIVIMG";
pub const FIRMWARE_MAGIC: u32 = u32::from_le_bytes(*b"RKFW");
pub const FIRMWARE_BASE: usize = 0x4000;
pub const FIRMWARE_LIMIT: usize = 0xe000;

/// A file opened for sequential sector reads.
pub struct Reader<D> {
    pub disk: FileDisk<D>,
    pub size: u32,
}

pub fn open<D: Disk>(mut device: D, name: &[u8; 11]) -> Result<Reader<D>, fat::Error> {
    let file = fat::find(&mut device, name)?;
    Ok(Reader {
        disk: FileDisk {
            device,
            extents: file.extents,
            count: file.count,
        },
        size: file.size,
    })
}

impl<D: Disk> Reader<D> {
    /// Call `each` with every sector of the file and the count of valid bytes in it.
    pub fn sectors(&mut self, mut each: impl FnMut(&Sector, usize)) -> Result<(), fat::Error> {
        let mut buffer = [0u8; SECTOR];
        let mut left = self.size as usize;
        let mut sector = 0;
        while left != 0 {
            self.disk
                .read(sector, &mut buffer)
                .map_err(|_| fat::Error::Read)?;
            let valid = left.min(SECTOR);
            each(&buffer, valid);
            left -= valid;
            sector += 1;
        }
        Ok(())
    }
}
