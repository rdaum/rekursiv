//! OBJEKT machine state, construction, and reset.
#![deny(missing_docs)]

use crate::{store::BackingStore, State};
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
    pub entries: Vec<Option<Entry>>,
    /// Class references for compact type codes. An unset class makes lookup fail.
    pub classes: [Option<Word>; 64],
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
            entries: vec![None; pager_entries],
            classes: [None; 64],
            memory: vec![Word::ZERO; memory_words],
            store: BackingStore::default(),
            next_identity: 1,
            body_cursor: 0,
            allocation_limit: memory_words as u32,
            persistent_roots: vec![false; pager_entries],
            maintenance: false,
        }
    }
    /// Restore power-on state while retaining pager and RAM capacities.
    ///
    /// This clears RAM, backing records, registers, compact classes, and all
    /// mappings. Allocation again starts at RAM word zero and identity one.
    pub fn reset(&mut self) {
        *self = Self::new(self.entries.len(), self.memory.len());
    }
}
