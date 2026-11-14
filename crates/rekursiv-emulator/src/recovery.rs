//! OBJEKT maintenance datapath, one privileged request at a time.
//!
//! This contains no graph walker or copy loop. The loaded collector microcode
//! chooses the roots, marks, and every read/write. The datapath keeps the work
//! list: the first mark of a slot queues it, and Next takes the most recently
//! queued slot. NextMarked then visits the marked slots in ascending order for
//! copying. Relocations become visible together at Commit, just as in
//! rtl/objekt_gc.sv.
//!
//! BeginMinor starts a minor collection, unless the collection is explicit
//! or too few words remain free: old
//! objects count as live, the work list starts with the persistence roots
//! and the remembered old objects, and Commit drops the unmarked young
//! objects in place. It declines with a zero result otherwise, and the
//! microcode then begins a full collection.
use rekursiv_asm::{processor::Recovery, Entry, Status, Word};
use rekursiv_model::Model;

pub(crate) struct RecoveryState {
    pub saved: rekursiv_model::processor::Processor,
    pub irq: bool,
    pub roots: Vec<Word>,
    pub needed: u32,
    pub committed: bool,
    initialized: bool,
    /// This collection is minor: nothing is copied.
    minor: bool,
    /// An explicit gc=Collect, which always collects in full.
    explicit: bool,
    selected: usize,
    marked: Vec<bool>,
    /// Marked slots not yet taken by Next, most recent last.
    queue: Vec<usize>,
    /// Where NextMarked resumes its ascending scan.
    scan: usize,
    /// Slots the last NextMarked examined, for the cycle estimate.
    pub scanned: u64,
    bases: Vec<Option<u32>>,
    complete: Vec<bool>,
    cursor: u32,
    end: u32,
    write_offset: u32,
    copied: Option<(u32, Word)>,
}
impl RecoveryState {
    pub fn new(
        saved: rekursiv_model::processor::Processor,
        irq: bool,
        roots: Vec<Word>,
        needed: u32,
        explicit: bool,
        model: &Model,
    ) -> Self {
        let half = model.memory.len() as u32 / 2;
        let upper = model.allocation_limit == model.memory.len() as u32;
        Self {
            saved,
            irq,
            roots,
            needed,
            committed: false,
            initialized: false,
            minor: false,
            explicit,
            selected: 0,
            marked: vec![false; model.entries.len()],
            queue: Vec::new(),
            scan: 0,
            scanned: 0,
            bases: vec![None; model.entries.len()],
            complete: vec![false; model.entries.len()],
            cursor: if upper { 0 } else { half },
            end: if upper { half } else { half * 2 },
            write_offset: 0,
            copied: None,
        }
    }
    fn entry(&self, model: &Model) -> Option<Entry> {
        model.entries[self.selected].get()
    }
    /// A NEW or modified object that persistence requires in RAM, or the
    /// object of a valid prepared access.
    fn retained(model: &Model, slot: usize, e: &Entry) -> bool {
        (e.modified && !e.new)
            || (e.new && model.persistent_roots[slot])
            || [model.state.prepared, model.state.stream]
                .iter()
                .any(|p| p.status == Status::Ok && p.reference == e.reference)
    }
    fn flags(&self, model: &Model, slot: usize) -> Result<Word, Status> {
        let Some(e) = model.entries[slot].get() else {
            return Ok(Word::ZERO);
        };
        Word::raw(
            1 | (u64::from(self.marked[slot]) << 1)
                | (((e.reference.bits() >> 37) & 1) << 2)
                | (u64::from(Self::retained(model, slot, &e)) << 3)
                | (u64::from(e.new) << 4),
        )
    }
    pub fn execute(&mut self, op: Recovery, data: Word, model: &mut Model) -> Result<Word, Status> {
        if (!self.initialized && !matches!(op, Recovery::Begin | Recovery::BeginMinor))
            || self.committed
        {
            return Err(Status::BadCommand);
        }
        let n = data.bits();
        let mut result = Word::ZERO;
        match op {
            Recovery::Begin | Recovery::BeginMinor => {
                if op == Recovery::BeginMinor
                    && (self.explicit || !model.minor_possible(self.needed))
                {
                    return Ok(Word::ZERO);
                }
                let half = model.memory.len() as u32 / 2;
                let source = if self.end == half { half } else { 0 };
                if self.initialized
                    || !model.memory.len().is_multiple_of(2)
                    || model.entries.iter().flatten().any(|e| {
                        e.base < source
                            || u64::from(e.base) + u64::from(e.size) > u64::from(source + half)
                    })
                {
                    return Err(Status::BoundsError);
                }
                self.initialized = true;
                self.minor = op == Recovery::BeginMinor;
                for (slot, entry) in model.entries.iter().enumerate() {
                    if entry.is_some_and(|e| {
                        Self::retained(model, slot, &e)
                            || (self.minor && model.remembered[slot] && !model.young(&e))
                    }) {
                        self.marked[slot] = true;
                        self.queue.push(slot);
                    }
                }
                if self.minor {
                    self.cursor = model.body_cursor;
                    self.end = model.allocation_limit;
                    result = Word::raw(1)?;
                }
            }
            Recovery::Root => result = self.roots.get(n as usize).copied().unwrap_or(Word::ZERO),
            Recovery::Slot => {
                if n >= model.entries.len() as u64 {
                    return Err(Status::BoundsError);
                }
                self.selected = n as usize;
                self.copied = None;
                result = self.flags(model, self.selected)?;
            }
            Recovery::Next => {
                if let Some(slot) = self.queue.pop() {
                    self.selected = slot;
                    self.copied = None;
                    result = self.flags(model, slot)?;
                }
            }
            Recovery::NextMarked => {
                let start = self.scan;
                let found = (start..self.marked.len()).find(|&slot| self.marked[slot]);
                self.scan = found.map_or(self.marked.len(), |slot| slot + 1);
                self.scanned = (self.scan - start) as u64;
                if let Some(slot) = found {
                    self.selected = slot;
                    self.copied = None;
                    result = self.flags(model, slot)?;
                }
            }
            Recovery::Info => {
                let e = self.entry(model).ok_or(Status::NotResident)?;
                result = match n {
                    0 => e.reference,
                    1 => e.class,
                    2 => Word::raw(e.size as u64)?,
                    _ => return Err(Status::BadCommand),
                };
            }
            Recovery::Mark => {
                let slot = n as usize & (model.entries.len() - 1);
                // A minor collection treats old objects as live, untraced.
                if data.is_reference()
                    && model.entries[slot]
                        .is_some_and(|e| e.reference == data && !(self.minor && !model.young(&e)))
                {
                    result = Word::raw(u64::from(!self.marked[slot]))?;
                    if !self.marked[slot] {
                        self.marked[slot] = true;
                        self.queue.push(slot);
                    }
                }
            }
            Recovery::ReadBody | Recovery::WriteBody => {
                let e = self.entry(model).ok_or(Status::BoundsError)?;
                if n >= u64::from(e.size)
                    || u64::from(e.base) + u64::from(e.size) > model.memory.len() as u64
                {
                    return Err(Status::BoundsError);
                }
                if op == Recovery::ReadBody {
                    result = model.memory[e.base as usize + n as usize];
                    self.copied = Some((n as u32, result));
                } else {
                    let base = self.bases[self.selected].ok_or(Status::BadCommand)?;
                    let (offset, word) = self.copied.ok_or(Status::BadCommand)?;
                    if u64::from(offset) != n || self.write_offset != offset {
                        return Err(Status::BadCommand);
                    }
                    model.memory[base as usize + offset as usize] = word;
                    self.copied = None;
                    self.write_offset += 1;
                    self.complete[self.selected] = self.write_offset == e.size;
                }
            }
            Recovery::Stage => {
                let e = self.entry(model).ok_or(Status::BadCommand)?;
                if !self.marked[self.selected] || self.bases[self.selected].is_some() {
                    return Err(Status::BadCommand);
                }
                if u64::from(self.cursor) + u64::from(e.size) > u64::from(self.end) {
                    return Err(Status::OutOfSpace);
                }
                self.bases[self.selected] = Some(self.cursor);
                self.cursor += e.size;
                self.write_offset = 0;
                self.complete[self.selected] = e.size == 0;
            }
            Recovery::Commit if self.minor => {
                if u64::from(self.cursor) + u64::from(self.needed) > u64::from(self.end) {
                    return Err(Status::OutOfSpace);
                }
                for slot in 0..model.entries.len() {
                    let Some(e) = model.entries[slot].get() else {
                        continue;
                    };
                    if model.young(&e) && !self.marked[slot] {
                        if model
                            .state
                            .selected
                            .is_some_and(|s| s.reference == e.reference)
                        {
                            model.state.selected = None.into();
                        }
                        model.entries[slot] = None.into();
                    }
                }
                model.young_boundary = model.body_cursor;
                model.remembered.fill(false);
                model.eviction_permitted = true;
                self.committed = true;
            }
            Recovery::Commit => {
                if self
                    .marked
                    .iter()
                    .zip(&self.complete)
                    .any(|(mark, done)| *mark && !done)
                {
                    return Err(Status::BadCommand);
                }
                if u64::from(self.cursor) + u64::from(self.needed) > u64::from(self.end) {
                    return Err(Status::OutOfSpace);
                }
                for (slot, entry) in model.entries.iter_mut().enumerate() {
                    if self.marked[slot] {
                        let e = entry.as_mut().ok_or(Status::BadCommand)?;
                        e.base = self.bases[slot].ok_or(Status::BadCommand)?;
                        if model
                            .state
                            .selected
                            .is_some_and(|s| s.reference == e.reference)
                        {
                            model.state.selected = Some(*e).into();
                        }
                    } else {
                        if entry.is_some_and(|e| {
                            model
                                .state
                                .selected
                                .is_some_and(|s| s.reference == e.reference)
                        }) {
                            model.state.selected = None.into();
                        }
                        *entry = None.into();
                    }
                }
                model.relocate_prepared();
                model.body_cursor = self.cursor;
                model.allocation_limit = self.end;
                model.eviction_permitted = true;
                model.young_boundary = self.cursor;
                model.remembered.fill(false);
                self.committed = true;
            }
            _ => return Err(Status::BadCommand),
        }
        Ok(result)
    }
}
