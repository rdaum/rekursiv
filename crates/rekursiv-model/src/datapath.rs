//! Resident command execution: registers, indexing, type checks, and fields.
#![deny(missing_docs)]

use crate::{MemoryEffect, Model};
use rekursiv_asm::{
    Command, Index, Memory, Pager, Read, Register, Status, Word, ADDRESS_LIMIT, ID_MASK, INDEX_MAX,
    INDEX_MIN,
};

impl Model {
    /// Evaluate a resident command against old operands, then publish local state.
    ///
    /// The caller validates static controls and the maintenance lock. Index and
    /// register changes, selection, type checks, and field access share one fault
    /// boundary. Every fallible check precedes publication of state or a RAM write.
    /// A failed RAM attempt can still set `effect` for traffic observations.
    /// Streaming pager commands are dispatched elsewhere.
    pub(super) fn transition(
        &mut self,
        c: Command,
        memory_error: bool,
        effect: &mut Option<MemoryEffect>,
    ) -> Result<Word, Status> {
        if matches!(c.pager, Pager::Fetch | Pager::Allocate | Pager::Exchange) {
            return Err(Status::BadCommand);
        }
        // Borrow old operands and stage only the registers this command changes.
        // No state is published until every fallible check has succeeded. This
        // preserves combined-control semantics without copying all eight VRs
        // and both metadata latches on every field read or index operation.
        let before = &self.state;
        let mut selection = None;
        let idx = match c.index {
            Index::None => before.index,
            Index::Load => c.data.as_index(),
            Index::Clear => 0,
            Index::One => 1,
            Index::Two => 2,
            Index::Increment => before.index + 1,
            Index::Decrement => before.index - 1,
            Index::Step => before.index + c.data.as_index(),
            Index::FromReg => before.index_reg,
            Index::Next => {
                let e = self.selected()?;
                if e.size == 0 || before.index < 0 || before.index > e.size as i64 {
                    return Err(Status::BoundsError);
                }
                selection = Some(e);
                if before.index == e.size as i64 {
                    1
                } else {
                    before.index + 1
                }
            }
        };
        if !(INDEX_MIN..=INDEX_MAX).contains(&idx) {
            return Err(Status::IndexOverflow);
        }
        let prepared = c.prepare.then(|| self.prepare_access(idx));
        let reg = match c.register {
            Register::None => before.index_reg,
            Register::Load => c.data.as_index(),
            Register::Increment => before.index_reg + 1,
            Register::Decrement => before.index_reg - 1,
            Register::FromIndex => before.index,
        };
        if !(INDEX_MIN..=INDEX_MAX).contains(&reg) {
            return Err(Status::IndexOverflow);
        }
        let mut output = Word::NIL;
        if c.pager != Pager::None {
            let r = match c.pager {
                Pager::ProbeBus => c.data,
                Pager::ProbeVr => before.vr[c.vr as usize],
                Pager::ProbeType => self.selected()?.class,
                Pager::ProbeRepresentation => self.selected()?.representation,
                _ => unreachable!(),
            };
            let e = self.resolve(r)?;
            output = e.reference;
            selection = Some(e);
        }
        let needs_metadata = matches!(
            c.read,
            Read::Size | Read::Type | Read::Base | Read::Representation | Read::Flags
        ) || c.expected_type.is_some()
            || (c.memory != Memory::None && !c.prepared);
        let old_entry = if needs_metadata {
            Some(self.selected()?)
        } else {
            None
        };
        if let Some(e) = old_entry {
            // A simultaneous page retains its newly selected snapshot; reads use old selection.
            if c.pager == Pager::None {
                selection = Some(e);
            }
            if c.expected_type.is_some_and(|t| t != e.class) {
                return Err(Status::TypeError);
            }
        }
        output = match c.read {
            Read::None => output,
            Read::Vr => before.vr[c.vr as usize],
            Read::Reference => before.selected.ok_or(Status::NoSelection)?.reference,
            Read::Index => Word::index(before.index)?,
            Read::IndexReg => Word::index(before.index_reg)?,
            Read::Size => Word::raw(old_entry.unwrap().size as u64)?,
            Read::Type => old_entry.unwrap().class,
            Read::Base => Word::raw(old_entry.unwrap().base as u64)?,
            Read::Representation => old_entry.unwrap().representation,
            Read::Flags => Word::raw(old_entry.unwrap().flags() as u64)?,
            Read::FreeWords => {
                Word::raw(self.allocation_limit.saturating_sub(self.body_cursor) as u64)?
            }
            Read::FreeIdentities => Word::raw((ID_MASK + 1).saturating_sub(self.next_identity))?,
        };
        if c.memory != Memory::None {
            let (mut e, index, prepared_address) = if c.prepared {
                let p = before.prepared;
                if p.status != Status::Ok {
                    return Err(p.status);
                }
                (self.resolve(p.reference)?, p.index, Some(p.address))
            } else {
                (old_entry.unwrap(), before.index, None)
            };
            if index <= 0 || index > e.size as i64 {
                return Err(Status::BoundsError);
            }
            let address = prepared_address
                .map(u64::from)
                .unwrap_or(e.base as u64 + index as u64 - 1);
            if address >= ADDRESS_LIMIT as u64 || address >= self.memory.len() as u64 {
                return Err(Status::BoundsError);
            }
            if c.memory == Memory::Read && index == 1 {
                output = e.representation;
            } else {
                *effect = Some(MemoryEffect {
                    address: address as u32,
                    write: c.memory == Memory::Write,
                    value: c.data,
                });
                if memory_error {
                    return Err(Status::MemoryError);
                }
                if c.memory == Memory::Read {
                    output = self.memory[address as usize];
                } else {
                    self.memory[address as usize] = c.data;
                    e.modified = true;
                    if index == 1 {
                        e.representation = c.data;
                    }
                    let slot = self.slot(e.reference);
                    self.entries[slot] = Some(e);
                    if selection
                        .or(before.selected)
                        .is_some_and(|selected| selected.reference == e.reference)
                    {
                        selection = Some(e);
                    }
                    output = c.data;
                }
            }
        }
        // RAM writes above cannot be followed by a command failure. Publish the
        // staged registers now, retaining untouched latches and virtual registers.
        self.state.index = idx;
        self.state.index_reg = reg;
        if let Some(selected) = selection {
            self.state.selected = Some(selected);
        }
        if let Some(prepared) = prepared {
            self.state.prepared = prepared;
        }
        if c.load_vr {
            self.state.vr[c.vr as usize] = c.data;
        }
        Ok(output)
    }
}
