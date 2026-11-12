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

pub mod mailbox {
    use super::{reg_read, reg_write};
    const BASE: usize = 0x2000_0000;

    pub fn waiting() -> bool {
        reg_read(BASE) & 1 != 0
    }
    pub fn respond_status(status: u32) {
        reg_write(BASE + 0x40, status);
        reg_write(BASE + 0x64, 1);
    }
}
