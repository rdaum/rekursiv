//! OBJEKT machine state, construction, and reset.
#![deny(missing_docs)]

use crate::{store::BackingStore, Slot, State};
use rekursiv_asm::{Entry, Word, ADDRESS_LIMIT};

/// Complete OBJEKT state: resident objects, register state, RAM, and backing storage.
///
/// The native emulator uses this model for object commands. RTL tests compare
/// its responses and request logs with hardware behavior at command boundaries.
/// Timing, handshakes, and stalls belong to the executor, not this model.
///
/// Fields remain public for image loading and test inspection. Direct setup must
/// preserve a power-of-two pager size, matching `persistent_roots` length, and
/// valid RAM ranges for resident entries. Normal execution uses the command and
/// service methods rather than replacing these structures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    /// Architectural virtual registers, index registers, selection, and prepared access.
    pub state: State,
    /// Direct-mapped resident metadata, indexed by the low identity bits.
    /// Lookup compares the complete reference to detect collisions.
    pub entries: Vec<Slot<Entry>>,
    /// Class references for compact type codes. An unset class makes lookup fail.
    pub classes: [Slot<Word>; 64],
    /// Object RAM in 40-bit words. Entry bases are zero-based word addresses.
    pub memory: Vec<Word>,
    /// Persistent object records behind the backing-store request interface.
    pub store: BackingStore,
    /// Next unused object identity; zero is reserved. Allocation never reuses an identity.
    pub next_identity: u64,
    /// Next free RAM word within the current allocation region.
    pub body_cursor: u32,
    /// Exclusive RAM allocation limit, in words. GC can restrict it to a semispace.
    pub allocation_limit: u32,
    /// One retention flag per pager slot for objects reachable from published storage.
    /// GC consults these flags when deciding which new resident objects must survive.
    pub persistent_roots: Vec<bool>,
    /// Maintenance lock. Normal object commands are rejected while it is set.
    pub maintenance: bool,
    /// Low-space thresholds, loaded by threshold commands. The low-space
    /// line is high while free words or free identities are below their
    /// threshold. Zero disables a threshold.
    pub word_threshold: u64,
    /// The identity threshold; see `word_threshold`.
    pub identity_threshold: u64,
    /// Set by an executor that has a collector. Allocation into a full pager
    /// then reports `OutOfSpace` so the collector can drop unreachable
    /// mappings, instead of saving a victim that may be garbage.
    pub collect_before_evict: bool,
    /// A collection has committed since allocation last found an empty slot,
    /// so the pager is full of retained objects and allocation may evict.
    pub eviction_permitted: bool,
    /// The allocation cursor at the last collection. A resident object whose
    /// body starts here or above was allocated or refilled since: it is young.
    pub young_boundary: u32,
    /// One flag per pager slot: a reference was written into this slot's old
    /// object since the last collection. A minor collection scans such objects.
    pub remembered: Vec<bool>,
}
impl Model {
    /// Construct an empty pager and zero-filled RAM with no backing records.
    ///
    /// Identity allocation starts at one. The entire RAM is initially available;
    /// a semispace executor sets [`Self::allocation_limit`] before execution.
    /// Compact classes are unset until bootstrap installs them.
    ///
    /// # Panics
    ///
    /// Panics unless `pager_entries` is a power of two and at least two.
    /// Panics unless `memory_words` is in `1..=rekursiv_asm::ADDRESS_LIMIT`.
    ///
    /// # Examples
    ///
    /// ```
    /// use rekursiv_asm::{Command, Status, Word};
    /// use rekursiv_model::Model;
    ///
    /// let mut model = Model::new(16, 1024);
    /// let class = Word::reference(100, true)?;
    /// let allocate = Command::allocate(class, 2, true)?;
    /// let reply = model.execute_response(allocate, false);
    /// assert_eq!(reply.status, Status::Ok);
    /// # Ok::<(), Status>(())
    /// ```
    pub fn new(pager_entries: usize, memory_words: usize) -> Self {
        assert!(pager_entries.is_power_of_two() && pager_entries >= 2);
        assert!(memory_words > 0 && memory_words <= ADDRESS_LIMIT as usize);
        Self {
            state: State::default(),
            entries: vec![Slot::default(); pager_entries],
            classes: [Slot::default(); 64],
            memory: vec![Word::ZERO; memory_words],
            store: BackingStore::default(),
            next_identity: 1,
            body_cursor: 0,
            allocation_limit: memory_words as u32,
            word_threshold: 0,
            identity_threshold: 0,
            collect_before_evict: false,
            eviction_permitted: false,
            persistent_roots: vec![false; pager_entries],
            maintenance: false,
            young_boundary: 0,
            remembered: vec![false; pager_entries],
        }
    }
    /// Restore power-on state while retaining pager and RAM capacities.
    ///
    /// This clears RAM, backing records, registers, compact classes, and all
    /// mappings. Allocation again starts at RAM word zero and identity one.
    pub fn reset(&mut self) {
        *self = Self::new(self.entries.len(), self.memory.len());
    }
    /// Words still free in the current allocation region.
    pub fn free_words(&self) -> u64 {
        u64::from(self.allocation_limit.saturating_sub(self.body_cursor))
    }
    /// Identities not yet issued.
    pub fn free_identities(&self) -> u64 {
        (rekursiv_asm::ID_MASK + 1).saturating_sub(self.next_identity)
    }
    /// Whether an object was allocated or refilled since the last collection.
    pub fn young(&self, e: &rekursiv_asm::Entry) -> bool {
        e.base >= self.young_boundary
    }
    /// Whether a collection that must place `needed` more words may be minor:
    /// a minor collection reclaims slots but no words, so it leaves an eighth
    /// of a semispace free besides the request.
    pub fn minor_possible(&self, needed: u32) -> bool {
        self.free_words() >= u64::from(needed) + self.memory.len() as u64 / 16
    }
    /// The low-space line LOGIK tests with the LowSpace condition.
    pub fn low_space(&self) -> bool {
        self.free_words() < self.word_threshold || self.free_identities() < self.identity_threshold
    }
}
