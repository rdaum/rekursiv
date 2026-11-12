//! The object store in REKURSIV.IMG, as specified in docs/store-format.md.
//!
//! `Store::request` serves one operation of OBJEKT's streaming backing-store
//! interface with the semantics of rekursiv-model's in-memory BackingStore,
//! but keeps every committed record on the disk.
use crate::disk::{Disk, Sector, SECTOR};

pub const ID_MASK: u64 = (1 << 37) - 1;
pub const NIL: u64 = 3 << 38;

/// Status codes shared with OBJEKT.
pub mod status {
    pub const OK: u8 = 0;
    pub const BAD_VALUE: u8 = 2;
    pub const INVALID_REFERENCE: u8 = 3;
    pub const SERVICE_ERROR: u8 = 10;
}

pub mod op {
    pub const METADATA: u8 = 0;
    pub const READ_WORD: u8 = 1;
    pub const BEGIN_SAVE: u8 = 2;
    pub const WRITE_WORD: u8 = 3;
    pub const COMMIT_SAVE: u8 = 4;
    pub const BEGIN_BATCH: u8 = 5;
    pub const COMMIT_BATCH: u8 = 6;
    pub const ABORT_BATCH: u8 = 7;
    pub const NEXT_RECORD: u8 = 8;
    pub const FIND_RECORD: u8 = 9;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub op: u8,
    pub reference: u64,
    pub class: u64,
    pub size: u32,
    pub cond: bool,
    pub offset: u32,
    pub data: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reply {
    pub status: u8,
    pub reference: u64,
    pub class: u64,
    pub size: u32,
    pub cond: bool,
    pub data: u64,
}

fn is_reference(word: u64) -> bool {
    word >> 38 == 2 && word & ID_MASK != 0
}

// ---------------------------------------------------------------------------
// On-disk structures
// ---------------------------------------------------------------------------

const SUPER_MAGIC: &[u8; 8] = b"RKSTORE1";
const RECORD_MAGIC: &[u8; 8] = b"RKRECORD";
const JOURNAL_MAGIC: &[u8; 8] = b"RKJOURNL";
const VERSION: u32 = 1;
const HEADER_BYTES: u32 = 40;
const JOURNAL_SECTORS: u32 = 4;
/// Journal capacity: a 16-byte header, then 16 bytes per entry, then a CRC.
pub const MAX_BATCH: usize = (JOURNAL_SECTORS as usize * SECTOR - 20) / 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Superblock {
    pub generation: u32,
    pub journal_start: u32,
    pub journal_sectors: u32,
    pub index_start: u32,
    pub capacity: u32,
    pub log_start: u32,
    pub log_end: u32,
    pub log_head: u32,
    pub journal_pending: bool,
    pub highest: u32,
}

fn get32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn get64(b: &[u8], at: usize) -> u64 {
    u64::from(get32(b, at)) | u64::from(get32(b, at + 4)) << 32
}
fn put64(b: &mut [u8], at: usize, v: u64) {
    put32(b, at, v as u32);
    put32(b, at + 4, (v >> 32) as u32);
}

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 != 0 {
                crc >> 1 ^ 0xedb8_8320
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

/// CRC-32 (IEEE 802.3), as zlib computes it.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc = CRC_TABLE[((crc ^ u32::from(byte)) & 0xff) as usize] ^ crc >> 8;
    }
    !crc
}

impl Superblock {
    fn encode(&self, b: &mut Sector) {
        b.fill(0);
        b[0..8].copy_from_slice(SUPER_MAGIC);
        put32(b, 8, VERSION);
        put32(b, 12, self.generation);
        put32(b, 16, self.journal_start);
        put32(b, 20, self.journal_sectors);
        put32(b, 24, self.index_start);
        put32(b, 28, self.capacity);
        put32(b, 32, self.log_start);
        put32(b, 36, self.log_end);
        put32(b, 40, self.log_head);
        put32(b, 44, u32::from(self.journal_pending));
        put32(b, 48, self.highest);
        let crc = crc32(&b[..508]);
        put32(b, 508, crc);
    }
    fn decode(b: &Sector) -> Option<Self> {
        if &b[0..8] != SUPER_MAGIC || get32(b, 8) != VERSION || get32(b, 508) != crc32(&b[..508]) {
            return None;
        }
        Some(Self {
            generation: get32(b, 12),
            journal_start: get32(b, 16),
            journal_sectors: get32(b, 20),
            index_start: get32(b, 24),
            capacity: get32(b, 28),
            log_start: get32(b, 32),
            log_end: get32(b, 36),
            log_head: get32(b, 40),
            journal_pending: get32(b, 44) != 0,
            highest: get32(b, 48),
        })
    }
}

/// Lay out an empty store in a file of `sectors` sectors, with an index for
/// identities below `capacity`. Writes the superblocks and a zero index.
pub fn format<D: Disk>(disk: &mut D, sectors: u32, capacity: u32) -> Result<Superblock, Error> {
    let index_start = 2 + JOURNAL_SECTORS;
    let index_sectors = capacity.div_ceil(64);
    let log_start = index_start + index_sectors;
    if capacity < 2 || log_start + 16 > sectors {
        return Err(Error::Layout);
    }
    let zero = [0u8; SECTOR];
    for s in index_start..log_start {
        disk.write(s, &zero).map_err(|_| Error::Disk)?;
    }
    let sb = Superblock {
        generation: 1,
        journal_start: 2,
        journal_sectors: JOURNAL_SECTORS,
        index_start,
        capacity,
        log_start,
        log_end: sectors,
        log_head: log_start,
        journal_pending: false,
        highest: 0,
    };
    let mut b = [0u8; SECTOR];
    sb.encode(&mut b);
    disk.write(0, &b).map_err(|_| Error::Disk)?;
    Superblock {
        generation: 0,
        ..sb
    }
    .encode(&mut b);
    disk.write(1, &b).map_err(|_| Error::Disk)?;
    Ok(sb)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Disk = 1,
    NotFormatted = 2,
    Journal = 3,
    Layout = 4,
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

struct Save {
    reference: u64,
    class: u64,
    size: u32,
    cond: bool,
    next: u32,
    start: u32,
    /// The record sector being filled, and its file sector.
    buffer: Sector,
    buffer_sector: u32,
}

struct Batch {
    entries: [(u32, u64); MAX_BATCH],
    count: usize,
}

struct Record {
    reference: u64,
    class: u64,
    size: u32,
    cond: bool,
    start: u32,
}

pub struct Store<D> {
    disk: D,
    sb: Superblock,
    /// Which superblock copy (0 or 1) is current.
    current: u32,
    cache: Sector,
    cache_sector: Option<u32>,
    /// Next free log sector, counting records staged in an open batch.
    head: u32,
    save: Option<Save>,
    batch: Option<Batch>,
    /// A commit passed its commit point but its index update failed.
    unapplied: bool,
}

fn sectors_for(size: u32) -> u32 {
    (HEADER_BYTES + 8 * size).div_ceil(SECTOR as u32)
}

impl<D: Disk> Store<D> {
    /// Open a formatted store, finishing any commit interrupted after its
    /// commit point.
    pub fn open(mut disk: D) -> Result<Self, Error> {
        let mut b = [0u8; SECTOR];
        let mut best: Option<(u32, Superblock)> = None;
        for copy in 0..2 {
            disk.read(copy, &mut b).map_err(|_| Error::Disk)?;
            if let Some(sb) = Superblock::decode(&b) {
                if best.is_none_or(|(_, s)| sb.generation > s.generation) {
                    best = Some((copy, sb));
                }
            }
        }
        let (current, sb) = best.ok_or(Error::NotFormatted)?;
        let mut store = Self {
            disk,
            sb,
            current,
            cache: [0; SECTOR],
            cache_sector: None,
            head: sb.log_head,
            save: None,
            batch: None,
            unapplied: sb.journal_pending,
        };
        if store.unapplied {
            store.replay().map_err(|_| Error::Journal)?;
        }
        Ok(store)
    }

    pub fn superblock(&self) -> Superblock {
        self.sb
    }

    pub fn into_disk(self) -> D {
        self.disk
    }

    fn read(&mut self, sector: u32) -> Result<&Sector, ()> {
        if self.cache_sector != Some(sector) {
            self.cache_sector = None;
            self.disk.read(sector, &mut self.cache).map_err(|_| ())?;
            self.cache_sector = Some(sector);
        }
        Ok(&self.cache)
    }

    fn write(&mut self, sector: u32, data: &Sector) -> Result<(), ()> {
        if self.cache_sector == Some(sector) {
            self.cache_sector = None;
        }
        self.disk.write(sector, data).map_err(|_| ())
    }

    fn write_superblock(&mut self, sb: Superblock) -> Result<(), ()> {
        let mut b = [0u8; SECTOR];
        sb.encode(&mut b);
        let copy = 1 - self.current;
        self.write(copy, &b)?;
        self.current = copy;
        self.sb = sb;
        Ok(())
    }

    fn index_entry(&mut self, id: u64) -> Result<u64, ()> {
        if id == 0 || id >= u64::from(self.sb.capacity) {
            return Ok(0);
        }
        let sector = self.sb.index_start + (id / 64) as u32;
        let at = (id % 64) as usize * 8;
        Ok(get64(self.read(sector)?, at))
    }

    fn record(&mut self, id: u64) -> Result<Option<Record>, ()> {
        let entry = self.index_entry(id)?;
        if entry == 0 {
            return Ok(None);
        }
        let start = entry as u32;
        let b = self.read(start)?;
        if &b[0..8] != RECORD_MAGIC {
            return Err(());
        }
        Ok(Some(Record {
            reference: get64(b, 8),
            class: get64(b, 16),
            size: get32(b, 24),
            cond: get32(b, 28) & 1 != 0,
            start,
        }))
    }

    /// Write index entries, then a superblock marking the journal applied.
    fn apply(&mut self, entries: &[(u32, u64)]) -> Result<(), ()> {
        for &(id, entry) in entries {
            let sector = self.sb.index_start + id / 64;
            let mut b = *self.read(sector)?;
            put64(&mut b, (id % 64) as usize * 8, entry);
            self.write(sector, &b)?;
        }
        let sb = Superblock {
            generation: self.sb.generation + 1,
            journal_pending: false,
            ..self.sb
        };
        self.write_superblock(sb)?;
        self.unapplied = false;
        Ok(())
    }

    /// Finish a commit recorded in the journal.
    fn replay(&mut self) -> Result<(), ()> {
        let mut journal = [0u8; JOURNAL_SECTORS as usize * SECTOR];
        for i in 0..JOURNAL_SECTORS {
            let at = i as usize * SECTOR;
            let mut b = [0u8; SECTOR];
            self.disk
                .read(self.sb.journal_start + i, &mut b)
                .map_err(|_| ())?;
            journal[at..at + SECTOR].copy_from_slice(&b);
        }
        let count = get32(&journal, 8) as usize;
        if &journal[0..8] != JOURNAL_MAGIC || count > MAX_BATCH {
            return Err(());
        }
        let end = 16 + 16 * count;
        if get32(&journal, end) != crc32(&journal[..end]) {
            return Err(());
        }
        let mut entries = [(0u32, 0u64); MAX_BATCH];
        for (i, e) in entries[..count].iter_mut().enumerate() {
            *e = (
                get64(&journal, 16 + 16 * i) as u32,
                get64(&journal, 24 + 16 * i),
            );
        }
        self.apply(&entries[..count])
    }

    /// Publish entries atomically: journal, commit point, index, clean.
    fn commit(&mut self, entries: &[(u32, u64)]) -> Result<(), ()> {
        let mut journal = [0u8; JOURNAL_SECTORS as usize * SECTOR];
        journal[0..8].copy_from_slice(JOURNAL_MAGIC);
        put32(&mut journal, 8, entries.len() as u32);
        let mut highest = self.sb.highest;
        for (i, &(id, entry)) in entries.iter().enumerate() {
            put64(&mut journal, 16 + 16 * i, u64::from(id));
            put64(&mut journal, 24 + 16 * i, entry);
            highest = highest.max(id);
        }
        let end = 16 + 16 * entries.len();
        let crc = crc32(&journal[..end]);
        put32(&mut journal, end, crc);
        let used = end.div_ceil(SECTOR).max(1);
        for i in 0..used {
            let mut b = [0u8; SECTOR];
            b.copy_from_slice(&journal[i * SECTOR..(i + 1) * SECTOR]);
            self.write(self.sb.journal_start + i as u32, &b)?;
        }
        let sb = Superblock {
            generation: self.sb.generation + 1,
            log_head: self.head,
            journal_pending: true,
            highest,
            ..self.sb
        };
        self.write_superblock(sb)?;
        // Committed. A failure from here on leaves the commit to be finished
        // by the next request or the next start.
        self.unapplied = true;
        let _ = self.apply(entries);
        Ok(())
    }

    fn metadata(&mut self, id: u64) -> Result<Option<Record>, u8> {
        self.record(id).map_err(|_| status::SERVICE_ERROR)
    }

    /// Serve one request.
    pub fn request(&mut self, req: Request) -> Reply {
        let mut reply = Reply::default();
        if let Err(s) = self.serve(req, &mut reply) {
            reply = Reply {
                status: s,
                ..Reply::default()
            };
        }
        reply
    }

    fn serve(&mut self, req: Request, reply: &mut Reply) -> Result<(), u8> {
        if self.unapplied {
            self.replay().map_err(|_| status::SERVICE_ERROR)?;
        }
        match req.op {
            op::NEXT_RECORD | op::FIND_RECORD => {
                let id = req.reference;
                if id > ID_MASK {
                    return Err(status::BAD_VALUE);
                }
                reply.reference = NIL;
                reply.class = NIL;
                let found = if req.op == op::FIND_RECORD {
                    self.metadata(id)?
                } else {
                    let mut found = None;
                    let mut next = id + 1;
                    while next <= u64::from(self.sb.highest) && found.is_none() {
                        found = self.metadata(next)?;
                        next += 1;
                    }
                    found
                };
                if let Some(r) = found {
                    reply.reference = r.reference;
                    reply.class = r.class;
                    reply.size = r.size;
                    reply.cond = r.cond;
                }
                return Ok(());
            }
            op::BEGIN_BATCH => {
                self.save = None;
                self.head = self.sb.log_head;
                self.batch = Some(Batch {
                    entries: [(0, 0); MAX_BATCH],
                    count: 0,
                });
                return Ok(());
            }
            op::ABORT_BATCH => {
                self.save = None;
                self.batch = None;
                self.head = self.sb.log_head;
                return Ok(());
            }
            op::COMMIT_BATCH => {
                if self.save.is_some() || self.batch.is_none() {
                    return Err(status::SERVICE_ERROR);
                }
                let batch = self.batch.take().unwrap();
                if batch.count != 0 {
                    let entries = batch.entries;
                    if self.commit(&entries[..batch.count]).is_err() {
                        self.head = self.sb.log_head;
                        return Err(status::SERVICE_ERROR);
                    }
                }
                return Ok(());
            }
            _ => {}
        }
        if !is_reference(req.reference) {
            return Err(status::INVALID_REFERENCE);
        }
        let id = req.reference & ID_MASK;
        match req.op {
            op::METADATA | op::READ_WORD => {
                let r = self
                    .metadata(id)?
                    .filter(|r| r.reference == req.reference)
                    .ok_or(status::INVALID_REFERENCE)?;
                if req.op == op::METADATA {
                    reply.reference = r.reference;
                    reply.class = r.class;
                    reply.size = r.size;
                    reply.cond = r.cond;
                } else {
                    if req.offset >= r.size {
                        return Err(status::SERVICE_ERROR);
                    }
                    let at = HEADER_BYTES + 8 * req.offset;
                    let b = self
                        .read(r.start + at / SECTOR as u32)
                        .map_err(|_| status::SERVICE_ERROR)?;
                    reply.data = get64(b, (at % SECTOR as u32) as usize);
                }
            }
            op::BEGIN_SAVE => {
                if !is_reference(req.class) || req.size >= 1 << 24 {
                    return Err(status::SERVICE_ERROR);
                }
                let start = self.head;
                if id >= u64::from(self.sb.capacity)
                    || start + sectors_for(req.size) > self.sb.log_end
                {
                    return Err(status::SERVICE_ERROR);
                }
                let mut buffer = [0u8; SECTOR];
                buffer[0..8].copy_from_slice(RECORD_MAGIC);
                put64(&mut buffer, 8, req.reference);
                put64(&mut buffer, 16, req.class);
                put32(&mut buffer, 24, req.size);
                put32(&mut buffer, 28, u32::from(req.cond));
                put64(&mut buffer, 32, u64::from(self.sb.generation));
                self.save = Some(Save {
                    reference: req.reference,
                    class: req.class,
                    size: req.size,
                    cond: req.cond,
                    next: 0,
                    start,
                    buffer,
                    buffer_sector: start,
                });
            }
            op::WRITE_WORD => {
                let s = self.save.as_ref().ok_or(status::SERVICE_ERROR)?;
                if s.reference != req.reference || req.offset != s.next || s.next >= s.size {
                    return Err(status::SERVICE_ERROR);
                }
                let at = HEADER_BYTES + 8 * req.offset;
                let sector = s.start + at / SECTOR as u32;
                if sector != s.buffer_sector {
                    let (full, full_sector) = (s.buffer, s.buffer_sector);
                    self.write(full_sector, &full)
                        .map_err(|_| status::SERVICE_ERROR)?;
                    let s = self.save.as_mut().unwrap();
                    s.buffer = [0; SECTOR];
                    s.buffer_sector = sector;
                }
                let s = self.save.as_mut().unwrap();
                put64(
                    &mut s.buffer,
                    (at % SECTOR as u32) as usize,
                    req.data & ((1 << 40) - 1),
                );
                s.next += 1;
            }
            op::COMMIT_SAVE => {
                let s = self.save.as_ref().ok_or(status::SERVICE_ERROR)?;
                if s.reference != req.reference || s.next != s.size {
                    return Err(status::SERVICE_ERROR);
                }
                let (full, full_sector) = (s.buffer, s.buffer_sector);
                let length = sectors_for(s.size);
                let entry = u64::from(s.start) | u64::from(length) << 32;
                let end = s.start + length;
                let _ = (s.class, s.cond);
                self.write(full_sector, &full)
                    .map_err(|_| status::SERVICE_ERROR)?;
                if let Some(batch) = &mut self.batch {
                    if batch.count == MAX_BATCH {
                        return Err(status::SERVICE_ERROR);
                    }
                    batch.entries[batch.count] = (id as u32, entry);
                    batch.count += 1;
                    self.save = None;
                    self.head = end;
                } else {
                    self.head = end;
                    if self.commit(&[(id as u32, entry)]).is_err() {
                        self.head = self.sb.log_head;
                        return Err(status::SERVICE_ERROR);
                    }
                    self.save = None;
                }
            }
            _ => return Err(status::SERVICE_ERROR),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
