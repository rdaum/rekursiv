//! First-stage loader, built into the bitstream. It reads REKURSIV.FW from
//! the card into RAM, checks it, and jumps to it.
//!
//! Display row 0: "RLD1" while loading; on failure 0xe4000000 plus
//! 0x1nn for a FAT error nn, 0x200 for a bad header, 0x300 for a bad CRC,
//! 0x400 if the card did not start.
#![no_std]
#![no_main]

use disk_firmware::hw::{display, sd, Card};
use disk_firmware::image::{self, FIRMWARE_BASE, FIRMWARE_LIMIT, FIRMWARE_MAGIC};
use disk_firmware::store::crc32;

disk_firmware::startup!();

fn fail(code: u32) -> ! {
    display(0, 0xe400_0000 | code);
    loop {}
}

#[no_mangle]
extern "C" fn main() -> ! {
    display(0, 0x524c_4431); // "RLD1"
    let status = sd::wait_ready();
    display(1, status);
    if status & sd::READY == 0 {
        fail(0x400);
    }
    let mut file = match image::open(Card, image::FIRMWARE_NAME) {
        Ok(file) => file,
        Err(e) => fail(0x100 | e as u32),
    };
    if file.size < 12 || file.size as usize - 12 > FIRMWARE_LIMIT - FIRMWARE_BASE {
        fail(0x200);
    }
    // Copy the whole file to FIRMWARE_BASE - 12, so the payload lands at
    // FIRMWARE_BASE; the header overwrites nothing the loader still needs
    // only if the loader ends below that, which the linker script checks.
    let base = (FIRMWARE_BASE - 12) as *mut u8;
    let mut at = 0usize;
    let result = file.sectors(|sector, valid| {
        unsafe { core::ptr::copy_nonoverlapping(sector.as_ptr(), base.add(at), valid) };
        at += valid;
    });
    if let Err(e) = result {
        fail(0x100 | e as u32);
    }
    let header = |i: usize| unsafe { core::ptr::read_unaligned(base.add(4 * i) as *const u32) };
    let length = header(1) as usize;
    if header(0) != FIRMWARE_MAGIC || length != file.size as usize - 12 {
        fail(0x200);
    }
    let payload = unsafe { core::slice::from_raw_parts(FIRMWARE_BASE as *const u8, length) };
    if crc32(payload) != header(2) {
        fail(0x300);
    }
    let entry: extern "C" fn() -> ! = unsafe { core::mem::transmute(FIRMWARE_BASE) };
    entry()
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    display(0, 0xdead_0001);
    loop {}
}
