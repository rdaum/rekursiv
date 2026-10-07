//! Test specification for RAM retention, independent of the microcode's pager
//! passes and copy requests. It follows edges with a work list, then publishes
//! a complete candidate heap. It never fetches or deletes a backing record.
use crate::Model;
use rekursiv_asm::{Status, Word};
use std::collections::BTreeSet;
impl Model {
    pub fn collect_ram(
        &mut self,
        roots: &[Word],
        from_upper: bool,
        needed: u32,
    ) -> Result<(), Status> {
        let half = self.memory.len() as u32 / 2;
        let source_start = if from_upper { half } else { 0 };
        let source_end = source_start + half;
        let destination = if from_upper { 0 } else { half };
        let mut work = roots.to_vec();
        work.extend(self.state.vr);
        if self.state.prepared.status == Status::Ok {
            work.push(self.state.prepared.reference);
        }
        work.extend(self.classes.iter().flatten().copied());
        if let Some(e) = self.state.selected {
            work.extend([e.reference, e.class]);
        }
        for e in self.entries.iter().flatten() {
            if e.base < source_start || e.base as u64 + e.size as u64 > source_end as u64 {
                return Err(Status::BoundsError);
            }
            // A save can commit before a subsequent refill fails. Such a NEW
            // entry already has persistent identity and must retain its graph.
            if (e.modified && !e.new)
                || (e.new
                    && (self.persistent_roots[self.slot(e.reference)]
                        || self.store.records.contains_key(&e.reference.identity()?)))
            {
                work.push(e.reference);
            }
        }
        let mut retained = BTreeSet::new();
        while let Some(word) = work.pop() {
            if !word.is_reference() {
                continue;
            }
            let Some(e) = self.entries[self.slot(word)].filter(|e| e.reference == word) else {
                continue;
            };
            if !retained.insert(e.reference.bits()) {
                continue;
            }
            work.push(e.class);
            if e.reference.bits() & (1 << 37) != 0 {
                work.extend(&self.memory[e.base as usize..(e.base + e.size) as usize]);
            }
        }
        let size: u64 = self
            .entries
            .iter()
            .flatten()
            .filter(|e| retained.contains(&e.reference.bits()))
            .map(|e| e.size as u64)
            .sum();
        if size + needed as u64 > half as u64 {
            return Err(Status::OutOfSpace);
        }
        let mut candidate = self.clone();
        let mut cursor = destination;
        for entry in &mut candidate.entries {
            let Some(mut e) = *entry else {
                continue;
            };
            if !retained.contains(&e.reference.bits()) {
                *entry = None;
                continue;
            }
            candidate.memory[cursor as usize..(cursor + e.size) as usize]
                .copy_from_slice(&self.memory[e.base as usize..(e.base + e.size) as usize]);
            e.base = cursor;
            cursor += e.size;
            *entry = Some(e);
            if candidate
                .state
                .selected
                .is_some_and(|s| s.reference == e.reference)
            {
                candidate.state.selected = Some(e);
            }
        }
        candidate.body_cursor = cursor;
        candidate.allocation_limit = destination + half;
        candidate.relocate_prepared();
        *self = candidate;
        Ok(())
    }
}
