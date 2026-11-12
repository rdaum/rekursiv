//! Sector devices: the card, and a file within it.

pub const SECTOR: usize = 512;
pub type Sector = [u8; SECTOR];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskError;

/// A device of 512-byte sectors.
pub trait Disk {
    fn read(&mut self, sector: u32, data: &mut Sector) -> Result<(), DiskError>;
    fn write(&mut self, sector: u32, data: &Sector) -> Result<(), DiskError>;
}

/// A run of consecutive device sectors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extent {
    pub sector: u32,
    pub length: u32,
}

pub const MAX_EXTENTS: usize = 64;

/// A file's sectors, numbered from zero, mapped through its extents onto
/// the device. Access beyond the file fails.
pub struct FileDisk<D> {
    pub device: D,
    pub extents: [Extent; MAX_EXTENTS],
    pub count: usize,
}

impl<D: Disk> FileDisk<D> {
    pub fn sectors(&self) -> u32 {
        self.extents[..self.count].iter().map(|e| e.length).sum()
    }
    fn map(&self, mut sector: u32) -> Result<u32, DiskError> {
        for e in &self.extents[..self.count] {
            if sector < e.length {
                return Ok(e.sector + sector);
            }
            sector -= e.length;
        }
        Err(DiskError)
    }
}

impl<D: Disk> Disk for FileDisk<D> {
    fn read(&mut self, sector: u32, data: &mut Sector) -> Result<(), DiskError> {
        let s = self.map(sector)?;
        self.device.read(s, data)
    }
    fn write(&mut self, sector: u32, data: &Sector) -> Result<(), DiskError> {
        let s = self.map(sector)?;
        self.device.write(s, data)
    }
}

impl<D: Disk + ?Sized> Disk for &mut D {
    fn read(&mut self, sector: u32, data: &mut Sector) -> Result<(), DiskError> {
        (**self).read(sector, data)
    }
    fn write(&mut self, sector: u32, data: &Sector) -> Result<(), DiskError> {
        (**self).write(sector, data)
    }
}
