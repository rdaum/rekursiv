//! Disk processor firmware. Current stage: find the object image on the
//! card and show its extents; refuse every store request, as the stub did.
#![no_std]
#![no_main]

mod fat;
mod hw;

use hw::{display, mailbox, sd};

core::arch::global_asm!(
    ".section .text.start",
    ".global _start",
    "_start:",
    "    la sp, _stack_top",
    "    la t0, _bss_start",
    "    la t1, _bss_end",
    "1:  bgeu t0, t1, 2f",
    "    sw zero, 0(t0)",
    "    addi t0, t0, 4",
    "    j 1b",
    "2:  call main",
    "3:  j 3b",
);

const SERVICE_ERROR: u32 = 10;

#[no_mangle]
extern "C" fn main() -> ! {
    display(0, 0x5244_4b32); // "RDK2"
    let status = sd::wait_ready();
    display(1, status);
    if status & sd::READY != 0 {
        match fat::find(b"REKURSIVIMG") {
            Ok(file) => {
                display(2, file.size);
                display(3, file.count as u32);
                display(4, file.extents[0].sector);
                display(5, file.extents[0].length);
                let sectors: u32 = file.extents[..file.count].iter().map(|e| e.length).sum();
                display(6, sectors);
            }
            Err(e) => display(2, 0xe000_0000 | e as u32),
        }
    }
    let mut served = 0u32;
    loop {
        if mailbox::waiting() {
            mailbox::respond_status(SERVICE_ERROR);
            served = served.wrapping_add(1);
            display(7, served);
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    display(0, 0xdead_dead);
    loop {}
}
