//! Disk processor firmware, loaded from REKURSIV.FW by the loader. It opens
//! the object store in REKURSIV.IMG, programs the machine from the boot
//! stream in REKURSIV.BOO and starts it, then serves OBJEKT's backing-store
//! requests.
//!
//! Display rows: 0 signature or error, 1 SD status, 2 image size in sectors,
//! 3 boot records sent, 4 committed generation, 5 log head, 6 requests
//! served, 7 last request (op in bits 31:28, identity below).
//! Errors in row 0: e1 card, e2nn store file (FAT error nn), e3nn store
//! format, e5nn boot file (FAT error nn), e6 memory check failed.
#![no_std]
#![no_main]

use disk_firmware::disk::FileDisk;
use disk_firmware::fat;
use disk_firmware::hw::{display, machine, mailbox, sd, Card};
use disk_firmware::image;
use disk_firmware::store::{status, Reply, Store};

disk_firmware::startup!();

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
    let file = fat::find(&mut card, image::STORE_NAME).map_err(|e| 0xe200_0000 | e as u32)?;
    let disk = FileDisk {
        device: card,
        extents: file.extents,
        count: file.count,
    };
    display(2, disk.sectors());
    Store::open(disk).map_err(|e| 0xe300_0000 | e as u32)
}

/// Program the machine from the boot stream and start it.
fn boot_machine() -> Result<(), u32> {
    while machine::status() & machine::MEMORY_TESTED == 0 {}
    if machine::status() & machine::MEMORY_FAILED != 0 {
        return Err(0xe600_0000);
    }
    let mut file = image::open(Card, image::BOOT_NAME).map_err(|e| 0xe500_0000 | e as u32)?;
    let mut sent = 0u32;
    file.sectors(|sector, valid| {
        for record in sector[..valid].chunks_exact(8) {
            let record = u64::from_le_bytes(record.try_into().unwrap());
            if record >> 62 == 0 {
                continue;
            }
            machine::record(record);
            sent += 1;
        }
        display(3, sent);
    })
    .map_err(|e| 0xe500_0000 | e as u32)
}

#[no_mangle]
extern "C" fn main() -> ! {
    display(0, 0x5244_4b34); // "RDK4"
    let mut store = match open() {
        Ok(store) => store,
        Err(error) => refuse(error),
    };
    if let Err(error) = boot_machine() {
        refuse(error);
    }
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
    display(0, 0xdead_0002);
    loop {}
}
