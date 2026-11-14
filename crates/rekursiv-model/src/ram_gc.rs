//! Test specification for RAM retention, independent of the microcode's pager
//! passes and copy requests. It follows edges with a work list, then publishes
//! a complete candidate heap. It never fetches or deletes a backing record.
#![deny(missing_docs)]

use crate::Model;
use rekursiv_asm::{Status, Word};
use std::collections::BTreeSet;
impl Model {
    /// Compute the expected live RAM set and copy it to the opposite semispace.
    ///
    /// `from_upper` selects the occupied half. `roots` supplies caller roots;
    /// registers, compact classes, selection, prepared access, and persistent
    /// obligations contribute additional roots. `needed` reserves destination
    /// words after the retained bodies. Backing records remain unchanged.
    /// Successful publication also relocates a valid prepared address.
    ///
    /// # Errors
    ///
    /// Invalid resident ranges or identities return a status. Insufficient
    /// destination capacity returns `OutOfSpace`. Errors leave the model unchanged.
    /// This oracle is for comparisons; the emulator executes collector microcode.
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
        for p in [self.state.prepared, self.state.stream] {
            if p.status == Status::Ok {
                work.push(p.reference);
            }
        }
        work.extend(self.classes.iter().flatten().copied());
        if let Some(e) = self.state.selected.get() {
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
            let Some(mut e) = entry.get() else {
                continue;
            };
            if !retained.contains(&e.reference.bits()) {
                *entry = None.into();
                continue;
            }
            candidate.memory[cursor as usize..(cursor + e.size) as usize]
                .copy_from_slice(&self.memory[e.base as usize..(e.base + e.size) as usize]);
            e.base = cursor;
            cursor += e.size;
            *entry = Some(e).into();
            if candidate
                .state
                .selected
                .is_some_and(|s| s.reference == e.reference)
            {
                candidate.state.selected = Some(e).into();
            }
        }
        candidate.body_cursor = cursor;
        candidate.allocation_limit = destination + half;
        candidate.eviction_permitted = true;
        candidate.young_boundary = cursor;
        candidate.remembered.fill(false);
        candidate.relocate_prepared();
        *self = candidate;
        Ok(())
    }

    /// Compute the expected result of a minor collection: drop every young
    /// object that is unreachable from the roots, the persistence roots, and
    /// the old objects remembered by the write barrier. Old objects count as
    /// live and are not traced. Nothing moves; afterwards every object is old.
    ///
    /// # Errors
    ///
    /// Invalid resident ranges return `BoundsError`, as for [`Self::collect_ram`].
    /// Errors leave the model unchanged.
    pub fn collect_minor(&mut self, roots: &[Word], from_upper: bool) -> Result<(), Status> {
        let half = self.memory.len() as u32 / 2;
        let source_start = if from_upper { half } else { 0 };
        let mut work = roots.to_vec();
        work.extend(self.state.vr);
        for p in [self.state.prepared, self.state.stream] {
            if p.status == Status::Ok {
                work.push(p.reference);
            }
        }
        work.extend(self.classes.iter().flatten().copied());
        if let Some(e) = self.state.selected.get() {
            work.extend([e.reference, e.class]);
        }
        for (slot, e) in self.entries.iter().enumerate() {
            let Some(e) = e.get() else { continue };
            if e.base < source_start || e.base as u64 + e.size as u64 > (source_start + half) as u64
            {
                return Err(Status::BoundsError);
            }
            if (e.modified && !e.new)
                || (e.new
                    && (self.persistent_roots[slot]
                        || self.store.records.contains_key(&e.reference.identity()?)))
            {
                work.push(e.reference);
            }
            // A remembered old object's fields are roots for the young.
            if self.remembered[slot] && !self.young(&e) && e.reference.bits() & (1 << 37) != 0 {
                work.extend(&self.memory[e.base as usize..(e.base + e.size) as usize]);
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
            if !self.young(&e) || !retained.insert(e.reference.bits()) {
                continue;
            }
            work.push(e.class);
            if e.reference.bits() & (1 << 37) != 0 {
                work.extend(&self.memory[e.base as usize..(e.base + e.size) as usize]);
            }
        }
        for slot in 0..self.entries.len() {
            let Some(e) = self.entries[slot].get() else {
                continue;
            };
            if self.young(&e) && !retained.contains(&e.reference.bits()) {
                if self
                    .state
                    .selected
                    .is_some_and(|s| s.reference == e.reference)
                {
                    self.state.selected = None.into();
                }
                self.entries[slot] = None.into();
            }
        }
        self.young_boundary = self.body_cursor;
        self.remembered.fill(false);
        self.eviction_permitted = true;
        Ok(())
    }
}
