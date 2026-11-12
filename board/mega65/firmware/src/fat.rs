//! Read-only FAT32: find a file in the root directory and list its extents.
//!
//! The card may be partitioned (MBR with a FAT32 entry first) or a bare
//! FAT32 volume. Only 512-byte sectors are supported.
use crate::disk::{Disk, Extent, Sector, MAX_EXTENTS};

pub struct File {
    pub size: u32,
    pub extents: [Extent; MAX_EXTENTS],
    pub count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Read = 1,
    NoVolume = 2,
    Geometry = 3,
    NotFound = 4,
    BadChain = 5,
    TooFragmented = 6,
}

struct Volume {
    fat_start: u32,
    data_start: u32,
    cluster_sectors: u32,
    root_cluster: u32,
}

fn u16_at(b: &Sector, at: usize) -> u32 {
    u32::from(b[at]) | u32::from(b[at + 1]) << 8
}
fn u32_at(b: &Sector, at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn is_fat32_boot(b: &Sector) -> bool {
    b[0x1fe] == 0x55 && b[0x1ff] == 0xaa && &b[0x52..0x57] == b"FAT32"
}

struct Reader<'a, D> {
    disk: &'a mut D,
    buffer: Sector,
    buffered: Option<u32>,
}

impl<D: Disk> Reader<'_, D> {
    fn read(&mut self, sector: u32) -> Result<&Sector, Error> {
        if self.buffered != Some(sector) {
            self.buffered = None;
            self.disk
                .read(sector, &mut self.buffer)
                .map_err(|_| Error::Read)?;
            self.buffered = Some(sector);
        }
        Ok(&self.buffer)
    }

    fn volume(&mut self) -> Result<Volume, Error> {
        let mut start = 0;
        let b = self.read(0)?;
        if !is_fat32_boot(b) {
            if b[0x1fe] != 0x55 || b[0x1ff] != 0xaa {
                return Err(Error::NoVolume);
            }
            let entry = (0..4)
                .map(|i| 0x1be + 16 * i)
                .find(|&e| matches!(b[e + 4], 0x0b | 0x0c))
                .ok_or(Error::NoVolume)?;
            start = u32_at(b, entry + 8);
            if !is_fat32_boot(self.read(start)?) {
                return Err(Error::NoVolume);
            }
        }
        let b = &self.buffer;
        let cluster_sectors = u32::from(b[0x0d]);
        if u16_at(b, 0x0b) != 512 || cluster_sectors == 0 || !cluster_sectors.is_power_of_two() {
            return Err(Error::Geometry);
        }
        let fat_start = start + u16_at(b, 0x0e);
        let data_start = fat_start + u32::from(b[0x10]) * u32_at(b, 0x24);
        Ok(Volume {
            fat_start,
            data_start,
            cluster_sectors,
            root_cluster: u32_at(b, 0x2c),
        })
    }

    /// The cluster after `cluster`, or None at the end of the chain.
    fn next(&mut self, v: &Volume, cluster: u32) -> Result<Option<u32>, Error> {
        let b = self.read(v.fat_start + cluster / 128)?;
        let next = u32_at(b, (cluster as usize % 128) * 4) & 0x0fff_ffff;
        match next {
            0x0fff_fff8..=0x0fff_ffff => Ok(None),
            0 | 1 | 0x0fff_fff7 => Err(Error::BadChain),
            n => Ok(Some(n)),
        }
    }
}

impl Volume {
    fn first_sector(&self, cluster: u32) -> u32 {
        self.data_start + (cluster - 2) * self.cluster_sectors
    }
}

/// Find an 8.3 name, given as 11 bytes ("REKURSIVIMG"), in the root directory.
pub fn find<D: Disk>(disk: &mut D, name: &[u8; 11]) -> Result<File, Error> {
    let mut r = Reader {
        disk,
        buffer: [0; 512],
        buffered: None,
    };
    let v = r.volume()?;
    let mut cluster = Some(v.root_cluster);
    let mut found = None;
    'directory: while let Some(c) = cluster {
        for s in 0..v.cluster_sectors {
            let b = r.read(v.first_sector(c) + s)?;
            for e in (0..512).step_by(32) {
                match b[e] {
                    0x00 => break 'directory,
                    0xe5 => continue,
                    _ => {}
                }
                if b[e + 11] & 0x18 == 0 && &b[e..e + 11] == name {
                    found = Some((
                        u16_at(b, e + 0x14) << 16 | u16_at(b, e + 0x1a),
                        u32_at(b, e + 0x1c),
                    ));
                    break 'directory;
                }
            }
        }
        cluster = r.next(&v, c)?;
    }
    let (first, size) = found.ok_or(Error::NotFound)?;
    let mut file = File {
        size,
        extents: [Extent::default(); MAX_EXTENTS],
        count: 0,
    };
    let mut cluster = Some(first).filter(|&c| c >= 2);
    while let Some(c) = cluster {
        let sector = v.first_sector(c);
        let last = file.count.checked_sub(1).map(|i| &mut file.extents[i]);
        match last {
            Some(e) if e.sector + e.length == sector => e.length += v.cluster_sectors,
            _ => {
                if file.count == MAX_EXTENTS {
                    return Err(Error::TooFragmented);
                }
                file.extents[file.count] = Extent {
                    sector,
                    length: v.cluster_sectors,
                };
                file.count += 1;
            }
        }
        cluster = r.next(&v, c)?;
    }
    Ok(file)
}
