//! Create an object store file for the MEGA65 board.
//!
//!   cargo run --target x86_64-unknown-linux-gnu --example mkstore -- OUT SIZE_MIB [CAPACITY [RECORDS]]
//!
//! CAPACITY is the number of identities the index covers (default 2^26).
//! RECORDS is a file of objects to save into the new store, as written by
//! the emulator's `board_smalltalk` example: for each object, little-endian,
//! reference (8 bytes), class (8), size in words (4), cond (4), body words
//! (8 each). Copy OUT to the card's root directory as REKURSIV.IMG.
use disk_firmware::disk::{Disk, DiskError, Sector, SECTOR};
use disk_firmware::store::{format, op, status, Request, Store};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

struct FileDevice(File);

impl Disk for FileDevice {
    fn read(&mut self, sector: u32, data: &mut Sector) -> Result<(), DiskError> {
        self.0
            .seek(SeekFrom::Start(u64::from(sector) * SECTOR as u64))
            .map_err(|_| DiskError)?;
        self.0.read_exact(data).map_err(|_| DiskError)
    }
    fn write(&mut self, sector: u32, data: &Sector) -> Result<(), DiskError> {
        self.0
            .seek(SeekFrom::Start(u64::from(sector) * SECTOR as u64))
            .map_err(|_| DiskError)?;
        self.0.write_all(data).map_err(|_| DiskError)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (out, mib, capacity, records) = match args.as_slice() {
        [out, mib] => (out, mib.parse::<u32>()?, 1 << 26, None),
        [out, mib, capacity] => (out, mib.parse::<u32>()?, capacity.parse()?, None),
        [out, mib, capacity, records] => {
            (out, mib.parse::<u32>()?, capacity.parse()?, Some(records))
        }
        _ => return Err("usage: mkstore OUT SIZE_MIB [CAPACITY [RECORDS]]".into()),
    };
    let sectors = mib * 2048;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(out)?;
    file.set_len(u64::from(sectors) * SECTOR as u64)?;
    let mut device = FileDevice(file);
    let mut sb = format(&mut device, sectors, capacity).map_err(|e| format!("{e:?}"))?;
    if let Some(records) = records {
        let bytes = std::fs::read(records)?;
        let mut store = Store::open(device).map_err(|e| format!("{e:?}"))?;
        let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let (mut at, mut count) = (0, 0);
        while at < bytes.len() {
            let (reference, size) = (u64_at(at), u32_at(at + 16));
            let mut request = |request: Request| {
                let reply = store.request(request);
                if reply.status == status::OK {
                    Ok(())
                } else {
                    Err(format!(
                        "object {reference:#x}: store status {}",
                        reply.status
                    ))
                }
            };
            request(Request {
                op: op::BEGIN_SAVE,
                reference,
                class: u64_at(at + 8),
                size,
                cond: u32_at(at + 20) != 0,
                ..Request::default()
            })?;
            for offset in 0..size {
                request(Request {
                    op: op::WRITE_WORD,
                    reference,
                    offset,
                    data: u64_at(at + 24 + 8 * offset as usize),
                    ..Request::default()
                })?;
            }
            request(Request {
                op: op::COMMIT_SAVE,
                reference,
                ..Request::default()
            })?;
            at += 24 + 8 * size as usize;
            count += 1;
        }
        sb = store.superblock();
        println!("{out}: {count} objects saved; log head {}", sb.log_head);
    }
    println!(
        "{out}: {sectors} sectors, index for {capacity} identities, log sectors {}..{}",
        sb.log_start, sb.log_end
    );
    Ok(())
}
