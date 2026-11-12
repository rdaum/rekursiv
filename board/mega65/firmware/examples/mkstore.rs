//! Create an empty object store file for the MEGA65 board.
//!
//!   cargo run --target x86_64-unknown-linux-gnu --example mkstore -- OUT SIZE_MIB [CAPACITY]
//!
//! CAPACITY is the number of identities the index covers (default 262144).
//! Copy OUT to the card's root directory as REKURSIV.IMG.
use disk_firmware::disk::{Disk, DiskError, Sector, SECTOR};
use disk_firmware::store::format;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

struct FileDevice(File);

impl Disk for FileDevice {
    fn read(&mut self, _: u32, _: &mut Sector) -> Result<(), DiskError> {
        Err(DiskError)
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
    let (out, mib, capacity) = match args.as_slice() {
        [out, mib] => (out, mib.parse::<u32>()?, 262_144),
        [out, mib, capacity] => (out, mib.parse::<u32>()?, capacity.parse()?),
        _ => return Err("usage: mkstore OUT SIZE_MIB [CAPACITY]".into()),
    };
    let sectors = mib * 2048;
    let file = File::create(out)?;
    file.set_len(u64::from(sectors) * SECTOR as u64)?;
    let sb = format(&mut FileDevice(file), sectors, capacity).map_err(|e| format!("{e:?}"))?;
    println!(
        "{out}: {sectors} sectors, index for {capacity} identities, log sectors {}..{}",
        sb.log_start, sb.log_end
    );
    Ok(())
}
