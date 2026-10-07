//! Resident-object lookup and prepared-address validity.
#![deny(missing_docs)]

use crate::{Model, PreparedAccess};
use rekursiv_asm::{Entry, Status, Word};

impl Model {
    /// Select the direct-mapped pager slot without checking its stored reference.
    pub(super) fn slot(&self, r: Word) -> usize {
        r.bits() as usize & (self.entries.len() - 1)
    }
    /// Resolve a resident reference or synthesize metadata for a compact value.
    ///
    /// This does not select an object, refill RAM, or access backing storage.
    /// Stored references require a complete reference match in their pager slot.
    /// Compact values use the configured class and carry their payload as the
    /// representation word, with no body in RAM.
    ///
    /// # Errors
    ///
    /// Returns `NotResident` for an absent or colliding mapping. Invalid words
    /// and unconfigured compact classes return the corresponding value error.
    pub fn resolve(&self, r: Word) -> Result<Entry, Status> {
        if r.is_compact() {
            let (code, payload) = r.compact_parts()?;
            let class = self.classes[code as usize].ok_or(Status::BadValue)?;
            Ok(Entry {
                reference: r,
                class,
                size: 0,
                base: 0,
                representation: Word::raw(payload as u64)?,
                new: false,
                modified: false,
                cond: false,
            })
        } else {
            r.identity()?;
            self.entries[self.slot(r)]
                .filter(|e| e.reference == r)
                .ok_or(Status::NotResident)
        }
    }
    /// Revalidate the selected reference instead of trusting cached metadata.
    pub(super) fn selected(&self) -> Result<Entry, Status> {
        self.resolve(self.state.selected.ok_or(Status::NoSelection)?.reference)
    }
    /// Capture an address or deferred error using the old object selection.
    pub(super) fn prepare_access(&self, index: i64) -> PreparedAccess {
        let reference = self
            .state
            .selected
            .map(|e| e.reference)
            .unwrap_or(Word::NIL);
        let mut access = PreparedAccess {
            reference,
            index,
            ..PreparedAccess::default()
        };
        access.status = match self.selected() {
            Err(status) => status,
            Ok(e)
                if index <= 0
                    || index > i64::from(e.size)
                    || u64::from(e.base) + index as u64 > self.memory.len() as u64 =>
            {
                Status::BoundsError
            }
            Ok(e) => {
                access.address = e.base + index as u32 - 1;
                Status::Ok
            }
        };
        access
    }
    /// Publishing/replacing a pager mapping invalidates its derived address.
    /// Re-preparation is required even if a later refill restores that identity.
    pub(super) fn invalidate_prepared_slot(&mut self, slot: usize) {
        if self.state.prepared.status == Status::Ok
            && self.slot(self.state.prepared.reference) == slot
        {
            self.state.prepared.status = Status::NotResident;
        }
    }
    /// Update a valid prepared address after successful RAM relocation.
    ///
    /// The collector retains the prepared reference as a root and calls this
    /// after publishing new pager bases. If that reference no longer resolves,
    /// the latch records the lookup error instead of an address.
    /// Existing deferred errors remain unchanged.
    pub fn relocate_prepared(&mut self) {
        let p = self.state.prepared;
        if p.status == Status::Ok {
            match self.resolve(p.reference) {
                Ok(e) => self.state.prepared.address = e.base + p.index as u32 - 1,
                Err(status) => self.state.prepared.status = status,
            }
        }
    }
}
