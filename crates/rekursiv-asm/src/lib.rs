//! Architectural values and symbolic, horizontal resident-OBJEKT commands.
use std::fmt;
pub const WORD_MASK: u64 = (1 << 40) - 1;
pub const ID_MASK: u64 = (1 << 37) - 1;
pub const ADDRESS_LIMIT: u32 = 1 << 24;
pub const INDEX_MIN: i64 = -(1 << 39);
pub const INDEX_MAX: i64 = (1 << 39) - 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Word(u64);
impl Word {
    pub const NIL: Self = Self(3 << 38);
    pub const ZERO: Self = Self(0);
    /// Width-checked bits; opaque bodies can hold noncanonical bit patterns.
    pub fn from_bits(bits: u64) -> Result<Self, Status> {
        if bits > WORD_MASK {
            Err(Status::BadValue)
        } else {
            Ok(Self(bits))
        }
    }
    pub fn raw(payload: u64) -> Result<Self, Status> {
        if payload >= 1 << 39 {
            Err(Status::BadValue)
        } else {
            Ok(Self(payload))
        }
    }
    pub fn reference(identity: u64, scan: bool) -> Result<Self, Status> {
        if identity == 0 || identity > ID_MASK {
            return Err(Status::InvalidReference);
        }
        Ok(Self((1 << 39) | ((scan as u64) << 37) | identity))
    }
    pub fn compact(code: u8, payload: u32) -> Result<Self, Status> {
        if code > 3 || (code == 0 && payload != 0) || (code == 1 && payload > 1) {
            return Err(Status::BadValue);
        }
        Ok(Self((3 << 38) | ((code as u64) << 32) | payload as u64))
    }
    pub fn signed(value: i32) -> Self {
        Self::compact(2, value as u32).unwrap()
    }
    pub fn unsigned(value: u32) -> Self {
        Self::compact(3, value).unwrap()
    }
    pub fn boolean(value: bool) -> Self {
        Self::compact(1, value as u32).unwrap()
    }
    pub const fn bits(self) -> u64 {
        self.0
    }
    pub fn is_reference(self) -> bool {
        self.0 >> 38 == 2 && self.0 & ID_MASK != 0
    }
    pub fn is_compact(self) -> bool {
        self.0 >> 38 == 3
    }
    pub fn compact_parts(self) -> Result<(u8, u32), Status> {
        if !self.is_compact() {
            return Err(Status::BadValue);
        }
        let code = ((self.0 >> 32) & 63) as u8;
        let payload = self.0 as u32;
        Self::compact(code, payload)?;
        Ok((code, payload))
    }
    pub fn identity(self) -> Result<u64, Status> {
        if self.is_reference() {
            Ok(self.0 & ID_MASK)
        } else {
            Err(Status::InvalidReference)
        }
    }
    pub fn index(value: i64) -> Result<Self, Status> {
        if !(INDEX_MIN..=INDEX_MAX).contains(&value) {
            return Err(Status::IndexOverflow);
        }
        Ok(Self(value as u64 & WORD_MASK))
    }
    pub fn as_index(self) -> i64 {
        ((self.0 << 24) as i64) >> 24
    }
}
impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:010x}", self.0)
    }
}

macro_rules! codes {
    ($name:ident { $($variant:ident = $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        #[repr(u8)]
        pub enum $name { #[default] $($variant = $value),+ }
        impl std::str::FromStr for $name {
            type Err = ();
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $(if s.eq_ignore_ascii_case(stringify!($variant)) { return Ok(Self::$variant); })+
                Err(())
            }
        }
        impl TryFrom<u8> for $name {
            type Error = Status;
            fn try_from(value: u8) -> Result<Self, Status> {
                match value { $($value => Ok(Self::$variant),)+ _ => Err(Status::BadCommand) }
            }
        }
    };
}
codes!(Status { Ok = 0, BadCommand = 1, BadValue = 2, InvalidReference = 3,
    NotResident = 4, NoSelection = 5, BoundsError = 6, IndexOverflow = 7,
    TypeError = 8, MemoryError = 9, ServiceError = 10, OutOfSpace = 11, IdentityExhausted = 12 });
impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Status {}
codes!(Pager { None = 0, ProbeBus = 1, ProbeVr = 2, ProbeType = 3, ProbeRepresentation = 4,
    Fetch = 5, Allocate = 6 });
codes!(Index { None = 0, Load = 1, Clear = 2, One = 3, Two = 4, Increment = 5,
    Decrement = 6, Step = 7, Next = 8, FromReg = 9 });
codes!(Register { None = 0, Load = 1, Increment = 2, Decrement = 3, FromIndex = 4 });
codes!(Memory { None = 0, Read = 1, Write = 2 });
codes!(Read { None = 0, Vr = 1, Reference = 2, Size = 3, Type = 4, Base = 5,
    Representation = 6, Index = 7, IndexReg = 8, Flags = 9 });

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Command {
    pub pager: Pager,
    pub index: Index,
    pub register: Register,
    pub memory: Memory,
    pub read: Read,
    pub load_vr: bool,
    pub vr: u8,
    pub data: Word,
    /// Optional equality check between the OLD selected type and this reference.
    pub expected_type: Option<Word>,
    pub alloc_size: u32,
    pub alloc_scan: bool,
}
impl Command {
    pub fn fetch(reference: Word) -> Self {
        Self {
            pager: Pager::Fetch,
            data: reference,
            ..Self::default()
        }
    }
    pub fn allocate(class: Word, size: u32, scan: bool) -> Result<Self, Status> {
        Self {
            pager: Pager::Allocate,
            data: class,
            alloc_size: size,
            alloc_scan: scan,
            ..Self::default()
        }
        .validate()
    }

    pub fn probe(value: Word) -> Self {
        Self {
            pager: Pager::ProbeBus,
            data: value,
            ..Self::default()
        }
    }
    pub fn index(value: i64) -> Result<Self, Status> {
        Ok(Self {
            index: Index::Load,
            data: Word::index(value)?,
            ..Self::default()
        })
    }
    pub fn read_field() -> Self {
        Self {
            memory: Memory::Read,
            ..Self::default()
        }
    }
    pub fn write_field(value: Word) -> Self {
        Self {
            memory: Memory::Write,
            data: value,
            ..Self::default()
        }
    }
    pub fn read(read: Read) -> Self {
        Self {
            read,
            ..Self::default()
        }
    }
    pub fn validate(self) -> Result<Self, Status> {
        if matches!(self.pager, Pager::Fetch | Pager::Allocate)
            && (self.index != Index::None
                || self.register != Register::None
                || self.memory != Memory::None
                || self.read != Read::None
                || self.load_vr
                || self.expected_type.is_some())
        {
            return Err(Status::BadCommand);
        }
        if self.alloc_size >= ADDRESS_LIMIT {
            return Err(Status::BadValue);
        }
        if self.pager == Pager::Allocate && !self.data.is_reference() {
            return Err(Status::InvalidReference);
        }
        if self.vr > 7
            || (self.pager != Pager::None && self.memory != Memory::None)
            || (self.pager != Pager::None && self.index == Index::Next)
            || (self.memory != Memory::None && self.read != Read::None)
        {
            return Err(Status::BadCommand);
        }
        if self.expected_type.is_some_and(|w| !w.is_reference()) {
            return Err(Status::BadValue);
        }
        Ok(self)
    }
    pub fn encode(self) -> Result<Ports, Status> {
        self.validate()?;
        Ok(Ports {
            pager: self.pager as u8,
            index: self.index as u8,
            register: self.register as u8,
            memory: self.memory as u8,
            read: self.read as u8,
            load_vr: self.load_vr as u8,
            vr: self.vr,
            data: self.data.bits(),
            check_type: self.expected_type.is_some() as u8,
            expected_type: self.expected_type.unwrap_or(Word::ZERO).bits(),
            alloc_size: self.alloc_size,
            alloc_scan: self.alloc_scan as u8,
        })
    }
}
impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "pager={:?} index={:?} reg={:?} mem={:?} read={:?} vr={} load={} data={}",
            self.pager,
            self.index,
            self.register,
            self.memory,
            self.read,
            self.vr,
            self.load_vr,
            self.data
        )?;
        if self.pager == Pager::Allocate {
            write!(f, " size={} scan={}", self.alloc_size, self.alloc_scan)?;
        }
        if let Some(t) = self.expected_type {
            write!(f, " type={t}")?;
        }
        Ok(())
    }
}
/// Provisional separate wrapper fields, not a historical packed control word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ports {
    pub pager: u8,
    pub index: u8,
    pub register: u8,
    pub memory: u8,
    pub read: u8,
    pub load_vr: u8,
    pub vr: u8,
    pub data: u64,
    pub check_type: u8,
    pub expected_type: u64,
    pub alloc_size: u32,
    pub alloc_scan: u8,
}
impl Ports {
    pub fn decode(self) -> Result<Command, Status> {
        if self.load_vr > 1 || self.check_type > 1 || self.alloc_scan > 1 {
            return Err(Status::BadCommand);
        }
        Command {
            pager: self.pager.try_into()?,
            index: self.index.try_into()?,
            register: self.register.try_into()?,
            memory: self.memory.try_into()?,
            read: self.read.try_into()?,
            load_vr: self.load_vr != 0,
            alloc_size: self.alloc_size,
            alloc_scan: self.alloc_scan != 0,
            vr: self.vr,
            data: Word::from_bits(self.data)?,
            expected_type: if self.check_type != 0 {
                Some(Word::from_bits(self.expected_type)?)
            } else {
                None
            },
        }
        .validate()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub reference: Word,
    pub class: Word,
    pub size: u32,
    pub base: u32,
    pub representation: Word,
    pub new: bool,
    pub modified: bool,
    pub cond: bool,
}
impl Entry {
    pub fn validate(self) -> Result<Self, Status> {
        if !self.reference.is_reference() || !self.class.is_reference() {
            return Err(Status::InvalidReference);
        }
        if self.size >= ADDRESS_LIMIT || self.base >= ADDRESS_LIMIT {
            return Err(Status::BadValue);
        }
        if self.size == 0 && self.representation != Word::NIL {
            return Err(Status::BadValue);
        }
        Ok(self)
    }
    pub fn flags(self) -> u8 {
        self.new as u8 | ((self.modified as u8) << 1) | ((self.cond as u8) << 2)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    ReadMemory {
        address: u32,
    },
    BeginRecovery,
    SetBodyCursor(u32),
    EndRecovery,
    Install(Entry),
    Invalidate(Word),
    CompactClass {
        code: u8,
        class: Word,
    },
    /// Raw writes are permitted only outside all published object ranges.
    WriteMemory {
        address: u32,
        value: Word,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: Status,
    pub data: Word,
}
impl Response {
    pub fn ok(data: Word) -> Self {
        Self {
            status: Status::Ok,
            data,
        }
    }
    pub fn error(status: Status) -> Self {
        Self {
            status,
            data: Word::NIL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_formats_and_limits() {
        assert_eq!(Word::NIL.bits(), 0xc000000000);
        assert_eq!(Word::reference(1, true).unwrap().bits(), 0xa000000001);
        assert_eq!(Word::reference(1, false).unwrap().bits(), 0x8000000001);
        assert_eq!(Word::signed(-1).bits(), 0xc2ffffffff);
        assert_eq!(Word::signed(i32::MIN).bits(), 0xc280000000);
        assert_eq!(Word::unsigned(u32::MAX).bits(), 0xc3ffffffff);
        assert!(Word::reference(0, false).is_err());
        assert!(Word::reference(1 << 37, false).is_err());
        assert!(Word::from_bits(1 << 40).is_err());
        assert!(Word::raw(1 << 39).is_err());
        assert!(Word::compact(4, 0).is_err());
        assert!(Word::compact(0, 1).is_err());
        assert!(Word::compact(1, 2).is_err());
        for n in [INDEX_MIN, -1, 0, 1, INDEX_MAX] {
            assert_eq!(Word::index(n).unwrap().as_index(), n);
        }
        assert!(Word::index(INDEX_MAX + 1).is_err());
    }
    #[test]
    fn fixed_port_vector_and_roundtrip() {
        let cmd = Command {
            pager: Pager::ProbeVr,
            index: Index::Increment,
            register: Register::FromIndex,
            load_vr: true,
            vr: 7,
            data: Word::NIL,
            read: Read::Index,
            ..Command::default()
        };
        let p = cmd.encode().unwrap();
        assert_eq!(
            p,
            Ports {
                pager: 2,
                index: 5,
                register: 4,
                memory: 0,
                read: 7,
                load_vr: 1,
                vr: 7,
                data: 0xc000000000,
                check_type: 0,
                expected_type: 0,
                alloc_size: 0,
                alloc_scan: 0
            }
        );
        assert_eq!(p.decode().unwrap(), cmd);
        for index in 0..=9 {
            for read in 0..=9 {
                let p = Ports {
                    index,
                    read,
                    ..Ports::default()
                };
                assert_eq!(p.decode().unwrap().encode().unwrap(), p);
            }
        }
        assert!(Ports {
            memory: 3,
            ..Ports::default()
        }
        .decode()
        .is_err());
        assert!(Ports {
            pager: 1,
            memory: 1,
            ..Ports::default()
        }
        .decode()
        .is_err());
        assert!(Ports {
            vr: 8,
            ..Ports::default()
        }
        .decode()
        .is_err());
    }
}

codes!(StoreOp { Metadata = 0, ReadWord = 1, BeginSave = 2, WriteWord = 3, CommitSave = 4 });
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoreRequest {
    pub op: StoreOp,
    pub reference: Word,
    pub class: Word,
    pub size: u32,
    pub cond: bool,
    pub offset: u32,
    pub data: Word,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoreReply {
    pub status: Status,
    pub reference: Word,
    pub class: Word,
    pub size: u32,
    pub cond: bool,
    pub data: Word,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Faults {
    /// Zero-based transaction number within this command.
    pub memory_at: Option<usize>,
    pub store_at: Option<usize>,
}

#[cfg(test)]
mod transfer_encoding_tests {
    use super::*;
    #[test]
    fn allocation_fields_and_standalone_restrictions() {
        let c = Command::allocate(Word::reference(1, true).unwrap(), 17, true).unwrap();
        let p = c.encode().unwrap();
        assert_eq!(p.pager, 6);
        assert_eq!(p.data, 0xa000000001);
        assert_eq!(p.alloc_size, 17);
        assert_eq!(p.alloc_scan, 1);
        assert_eq!(p.decode().unwrap(), c);
        assert_eq!(Command::fetch(c.data).encode().unwrap().pager, 5);
        assert!(Command::allocate(Word::NIL, 1, true).is_err());
        assert!(Command::allocate(c.data, ADDRESS_LIMIT, true).is_err());
        for pager in [Pager::Fetch, Pager::Allocate] {
            assert_eq!(
                Command {
                    pager,
                    index: Index::Increment,
                    ..c
                }
                .encode(),
                Err(Status::BadCommand)
            );
            assert_eq!(
                Command {
                    pager,
                    expected_type: Some(c.data),
                    ..c
                }
                .encode(),
                Err(Status::BadCommand)
            );
        }
        assert_eq!(Status::ServiceError as u8, 10);
        assert_eq!(Status::OutOfSpace as u8, 11);
        assert_eq!(Status::IdentityExhausted as u8, 12);
    }
}

/// Roots retained by software while the mutator is stopped.
#[derive(Clone, Debug, Default)]
pub struct Roots {
    pub driver: Vec<Word>,
    pub service: Vec<Word>,
    pub language: Vec<Word>,
    pub pending: Vec<Command>,
}
impl Roots {
    pub fn values(&self) -> Vec<Word> {
        let mut result = self.driver.clone();
        result.extend(&self.service);
        result.extend(&self.language);
        for c in &self.pending {
            result.push(c.data);
            result.extend(c.expected_type);
        }
        result
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryMode {
    Compact,
    Collect,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryReport {
    pub objects_before: usize,
    pub objects_after: usize,
    pub resident_before: usize,
    pub resident_after: usize,
    pub cursor_before: u32,
    pub cursor_after: u32,
}

pub mod processor;

pub mod collector;

pub mod text;
