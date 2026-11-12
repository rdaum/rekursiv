//! Memory-mapped registers of the disk processor (rtl/disk_processor.sv).
use core::ptr::{read_volatile, write_volatile};

fn reg_read(address: usize) -> u32 {
    unsafe { read_volatile(address as *const u32) }
}
fn reg_write(address: usize, value: u32) {
    unsafe { write_volatile(address as *mut u32, value) }
}

pub fn display(row: usize, value: u32) {
    reg_write(0x3000_0000 + 4 * (row & 7), value);
}

pub mod sd {
    use super::{reg_read, reg_write};
    const BASE: usize = 0x1000_0000;
    pub const READY: u32 = 1 << 0;
    pub const BUSY: u32 = 1 << 1;
    pub const FAILED: u32 = 1 << 2;
    pub const ERROR: u32 = 1 << 4;

    pub fn status() -> u32 {
        reg_read(BASE)
    }
    /// Wait for card setup to finish or fail; returns the status.
    pub fn wait_ready() -> u32 {
        loop {
            let s = status();
            if s & (READY | FAILED) != 0 {
                return s;
            }
        }
    }
    fn run(command: u32, sector: u32) -> u32 {
        reg_write(BASE + 4, sector);
        reg_write(BASE + 8, command);
        loop {
            let s = status();
            if s & BUSY == 0 {
                return s;
            }
        }
    }
    /// Read one sector into the buffer; returns the status (ERROR on failure).
    pub fn read(sector: u32) -> u32 {
        run(1, sector)
    }
    /// Write the buffer to one sector; returns the status.
    pub fn write_sector(sector: u32) -> u32 {
        run(2, sector)
    }
    /// The 512-byte sector buffer.
    pub fn buffer() -> &'static mut [u8; 512] {
        unsafe { &mut *((BASE + 0x200) as *mut [u8; 512]) }
    }
}

/// The SD card as a sector device, through the controller's buffer.
pub struct Card;

impl disk_firmware::disk::Disk for Card {
    fn read(
        &mut self,
        sector: u32,
        data: &mut disk_firmware::disk::Sector,
    ) -> Result<(), disk_firmware::disk::DiskError> {
        if sd::read(sector) & sd::ERROR != 0 {
            return Err(disk_firmware::disk::DiskError);
        }
        data.copy_from_slice(sd::buffer());
        Ok(())
    }
    fn write(
        &mut self,
        sector: u32,
        data: &disk_firmware::disk::Sector,
    ) -> Result<(), disk_firmware::disk::DiskError> {
        sd::buffer().copy_from_slice(data);
        if sd::write_sector(sector) & sd::ERROR != 0 {
            return Err(disk_firmware::disk::DiskError);
        }
        Ok(())
    }
}

pub mod mailbox {
    use super::{reg_read, reg_write};
    use disk_firmware::store::{Reply, Request};
    const BASE: usize = 0x2000_0000;

    fn word(at: usize) -> u64 {
        u64::from(reg_read(BASE + at)) | u64::from(reg_read(BASE + at + 4)) << 32
    }

    /// The waiting request, if any.
    pub fn request() -> Option<Request> {
        if reg_read(BASE) & 1 == 0 {
            return None;
        }
        Some(Request {
            op: reg_read(BASE + 0x04) as u8,
            reference: word(0x08),
            class: word(0x10),
            size: reg_read(BASE + 0x18),
            cond: reg_read(BASE + 0x1c) != 0,
            offset: reg_read(BASE + 0x20),
            data: word(0x24),
        })
    }

    pub fn respond(reply: &Reply) {
        reg_write(BASE + 0x40, u32::from(reply.status));
        reg_write(BASE + 0x44, reply.reference as u32);
        reg_write(BASE + 0x48, (reply.reference >> 32) as u32);
        reg_write(BASE + 0x4c, reply.class as u32);
        reg_write(BASE + 0x50, (reply.class >> 32) as u32);
        reg_write(BASE + 0x54, reply.size);
        reg_write(BASE + 0x58, u32::from(reply.cond));
        reg_write(BASE + 0x5c, reply.data as u32);
        reg_write(BASE + 0x60, (reply.data >> 32) as u32);
        reg_write(BASE + 0x64, 1);
    }
}
