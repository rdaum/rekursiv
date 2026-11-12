//! Disk processor firmware: find REKURSIV.IMG on the card, open the object
//! store in it, and serve OBJEKT's backing-store requests from it.
//!
//! Display rows: 0 signature or error, 1 SD status, 2 image size in sectors,
//! 3 extents, 4 committed generation, 5 log head, 6 requests served,
//! 7 last request (op in bits 31:28, identity below).
#![no_std]
#![no_main]

mod hw;

use disk_firmware::disk::FileDisk;
use disk_firmware::fat;
use disk_firmware::store::{status, Reply, Store};
use hw::{display, mailbox, sd, Card};

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

/// Serve every request with a service error, showing why on row 0.
fn refuse(error: u32) -> ! {
    display(0, error);
    let mut served = 0u32;
    loop {
        if mailbox::request().is_some() {
            mailbox::respond(&Reply {
                status: status::SERVICE_ERROR,
                ..Reply::default()
            });
            served = served.wrapping_add(1);
            display(6, served);
        }
    }
}

fn open() -> Result<Store<FileDisk<Card>>, u32> {
    let sd_status = sd::wait_ready();
    display(1, sd_status);
    if sd_status & sd::READY == 0 {
        return Err(0xe100_0000);
    }
    let mut card = Card;
    let file = fat::find(&mut card, b"REKURSIVIMG").map_err(|e| 0xe200_0000 | e as u32)?;
    let disk = FileDisk {
        device: card,
        extents: file.extents,
        count: file.count,
    };
    display(2, disk.sectors());
    display(3, file.count as u32);
    Store::open(disk).map_err(|e| 0xe300_0000 | e as u32)
}

#[no_mangle]
extern "C" fn main() -> ! {
    display(0, 0x5244_4b33); // "RDK3"
    let mut store = match open() {
        Ok(store) => store,
        Err(error) => refuse(error),
    };
    let mut served = 0u32;
    loop {
        let sb = store.superblock();
        display(4, sb.generation);
        display(5, sb.log_head);
        let Some(request) = mailbox::request() else {
            continue;
        };
        let reply = store.request(request);
        mailbox::respond(&reply);
        served = served.wrapping_add(1);
        display(6, served);
        display(
            7,
            u32::from(request.op) << 28 | (request.reference as u32 & 0x0fff_ffff),
        );
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    display(0, 0xdead_dead);
    loop {}
}
