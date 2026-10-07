//! OBJEKT maintenance datapath, one privileged request at a time.
//!
//! This contains no graph walker or copy loop. The loaded collector microcode
//! chooses the roots, marks, pager passes, and every read/write. Relocations
//! become visible together at Commit, just as in rtl/objekt_gc.sv.
use rekursiv_asm::{processor::Recovery, Entry, Status, Word};
use rekursiv_model::Model;

pub(crate) struct RecoveryState {
    pub saved: rekursiv_model::processor::Processor,
    pub irq: bool,
    pub roots: Vec<Word>,
    pub needed: u32,
    pub committed: bool,
    initialized: bool,
    selected: usize,
    marked: Vec<bool>,
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
            selected: 0,
            marked: vec![false; model.entries.len()],
            bases: vec![None; model.entries.len()],
            complete: vec![false; model.entries.len()],
            cursor: if upper { 0 } else { half },
            end: if upper { half } else { half * 2 },
            write_offset: 0,
            copied: None,
        }
    }
    fn entry(&self, model: &Model) -> Option<Entry> {
        model.entries[self.selected]
    }
    pub fn execute(&mut self, op: Recovery, data: Word, model: &mut Model) -> Result<Word, Status> {
        if (!self.initialized && op != Recovery::Begin) || self.committed {
            return Err(Status::BadCommand);
        }
        let n = data.bits();
        let mut result = Word::ZERO;
        match op {
            Recovery::Begin => {
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
            }
            Recovery::Root => result = self.roots.get(n as usize).copied().unwrap_or(Word::ZERO),
            Recovery::Slot => {
                if n >= model.entries.len() as u64 {
                    return Err(Status::BoundsError);
                }
                self.selected = n as usize;
                self.copied = None;
                if let Some(e) = model.entries[self.selected] {
                    let persistent =
                        (e.modified && !e.new) || (e.new && model.persistent_roots[self.selected]);
                    result = Word::raw(
                        1 | (u64::from(self.marked[self.selected]) << 1)
                            | (((e.reference.bits() >> 37) & 1) << 2)
                            | (u64::from(persistent) << 3)
                            | (u64::from(e.new) << 4),
                    )?;
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
                if data.is_reference() && model.entries[slot].is_some_and(|e| e.reference == data) {
                    result = Word::raw(u64::from(!self.marked[slot]))?;
                    self.marked[slot] = true;
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
                            model.state.selected = Some(*e);
                        }
                    } else {
                        if entry.is_some_and(|e| {
                            model
                                .state
                                .selected
                                .is_some_and(|s| s.reference == e.reference)
                        }) {
                            model.state.selected = None;
                        }
                        *entry = None;
                    }
                }
                model.body_cursor = self.cursor;
                model.allocation_limit = self.end;
                self.committed = true;
            }
            _ => return Err(Status::BadCommand),
        }
        Ok(result)
    }
}
