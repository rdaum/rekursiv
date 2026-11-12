//! Object allocation, resident fetches, eviction, and streaming refill.
//!
//! Transfers share the backing-store protocol with RTL tests. Request logs and
//! injected fault positions preserve publication order across partial failures.
#![deny(missing_docs)]

use crate::{MemoryEffect, Model, Outcome};
use rekursiv_asm::{
    Command, Entry, Faults, Pager, Response, Status, StoreOp, StoreReply, StoreRequest, Word,
    ADDRESS_LIMIT, ID_MASK,
};
impl Model {
    /// The identity the next allocation takes: the first one at or after
    /// `next_identity` whose slot holds no resident object, or `next_identity`
    /// itself when every slot is occupied.
    ///
    /// Identities are never reused, so without this an allocation would
    /// displace whatever long-lived object shares its slot while other slots
    /// stand empty. Skipped identities are never issued. Nothing changes
    /// until the allocation reserves its identity.
    ///
    /// # Errors
    ///
    /// `IdentityExhausted` when the search runs off the identity space.
    pub fn free_identity(&self) -> Result<u64, Status> {
        for identity in (self.next_identity..).take(self.entries.len()) {
            if identity > ID_MASK {
                return Err(Status::IdentityExhausted);
            }
            if self.entries[identity as usize & (self.entries.len() - 1)].is_none() {
                return Ok(identity);
            }
        }
        Ok(self.next_identity)
    }
    /// Cycles hardware spends choosing the next allocation's identity beyond
    /// the acceptance cycle: one for each occupied candidate it passes over.
    /// When every slot is occupied that is one per slot, the last of them
    /// reading the victim.
    pub fn allocation_skips(&self) -> u64 {
        let occupied = |identity: &u64| {
            *identity <= ID_MASK
                && self.entries[*identity as usize & (self.entries.len() - 1)].is_some()
        };
        (self.next_identity..)
            .take(self.entries.len())
            .take_while(occupied)
            .count() as u64
    }
    /// Record a persistence dependency: a record being streamed to the store
    /// names `word`. If that is a resident object, collection must keep it
    /// while it is NEW, because only RAM holds it. The mark is made as the
    /// reference is sent and stays if the save later fails.
    pub(super) fn protect(&mut self, word: Word) {
        let slot = self.slot(word);
        if self.entries[slot].is_some_and(|e| e.reference == word) {
            self.persistent_roots[slot] = true;
        }
    }
    /// Validate and execute a standalone fetch or allocation transaction.
    ///
    /// A resident fetch returns without RAM traffic. A miss or allocation can
    /// reserve RAM, evict a dirty victim, and initialize or refill a body before
    /// publishing its mapping. Failures can consume identities or RAM and leave
    /// earlier RAM writes or completed victim saves visible. Request observations
    /// include the failing access, indexed independently by each fault channel.
    ///
    /// This low-level test entry point validates the command but does not check
    /// the maintenance lock. Normal executors use [`Self::execute`] or
    /// [`Self::execute_response`], which enforce that boundary.
    pub fn execute_transfer(&mut self, command: Command, faults: Faults) -> Outcome {
        match command.validate() {
            Ok(command) => self.transfer_validated(command, faults),
            Err(status) => Outcome::error(status),
        }
    }
    // Caller has validated the command; transfer-specific checks remain below.
    pub(super) fn transfer_validated(&mut self, c: Command, faults: Faults) -> Outcome {
        let mut out = Outcome {
            response: Response::ok(Word::NIL),
            memory: Vec::new(),
            store: Vec::new(),
        };
        let result = (|| {
            if c.pager == Pager::Fetch {
                match self.resolve(c.data) {
                    Ok(e) => {
                        self.state.selected = Some(e).into();
                        return Ok(e.reference);
                    }
                    Err(Status::NotResident) => {}
                    Err(e) => return Err(e),
                }
            } else if c.pager != Pager::Allocate {
                return Err(Status::BadCommand);
            }
            let mut incoming = if c.pager == Pager::Allocate {
                let identity = self.free_identity()?;
                Entry {
                    reference: Word::reference(identity, c.alloc_scan)?,
                    class: c.data,
                    size: c.alloc_size,
                    base: 0,
                    representation: Word::NIL,
                    new: true,
                    modified: false,
                    cond: false,
                }
            } else {
                let reply = self.store_effect(
                    StoreRequest {
                        reference: c.data,
                        ..Default::default()
                    },
                    faults,
                    &mut out,
                )?;
                if reply.reference != c.data || !reply.class.is_reference() {
                    return Err(Status::InvalidReference);
                }
                Entry {
                    reference: c.data,
                    class: reply.class,
                    size: reply.size,
                    base: 0,
                    representation: Word::NIL,
                    new: false,
                    modified: false,
                    cond: reply.cond,
                }
            };
            let end = self.body_cursor as u64 + incoming.size as u64;
            if end > self.allocation_limit as u64 {
                return Err(Status::OutOfSpace);
            }
            incoming.base = self.body_cursor & (ADDRESS_LIMIT - 1);
            self.body_cursor = end as u32;
            if c.pager == Pager::Allocate {
                self.next_identity = (incoming.reference.bits() & ID_MASK) + 1;
            }
            let slot = self.slot(incoming.reference);
            if let Some(victim) = self.entries[slot].get() {
                if victim.new || victim.modified {
                    if victim.base as u64 + victim.size as u64 > self.memory.len() as u64 {
                        return Err(Status::BoundsError);
                    }
                    self.protect(victim.class);
                    self.store_effect(
                        StoreRequest {
                            op: StoreOp::BeginSave,
                            reference: victim.reference,
                            class: victim.class,
                            size: victim.size,
                            cond: victim.cond,
                            ..Default::default()
                        },
                        faults,
                        &mut out,
                    )?;
                    for offset in 0..victim.size {
                        let data = self.memory_effect(
                            MemoryEffect {
                                address: victim.base + offset,
                                write: false,
                                value: Word::ZERO,
                            },
                            faults,
                            &mut out,
                        )?;
                        if victim.reference.bits() & (1 << 37) != 0 {
                            self.protect(data);
                        }
                        self.store_effect(
                            StoreRequest {
                                op: StoreOp::WriteWord,
                                reference: victim.reference,
                                offset,
                                data,
                                ..Default::default()
                            },
                            faults,
                            &mut out,
                        )?;
                    }
                    self.store_effect(
                        StoreRequest {
                            op: StoreOp::CommitSave,
                            reference: victim.reference,
                            ..Default::default()
                        },
                        faults,
                        &mut out,
                    )?;
                    self.persistent_roots[slot] = true;
                }
            }
            // Refill does not mutate backing storage after victim publication.
            // Borrow its body once instead of searching the record tree for
            // every word. Preserve the individual requests, fault positions,
            // and memory writes of the streaming hardware interface below.
            let body = if c.pager == Pager::Fetch {
                self.store
                    .records
                    .get(&incoming.reference.identity()?)
                    .filter(|r| r.reference == incoming.reference)
                    .map(|r| r.body.as_slice())
            } else {
                None
            };
            for offset in 0..incoming.size {
                let value = if c.pager == Pager::Allocate {
                    if c.alloc_scan {
                        Word::NIL
                    } else {
                        Word::ZERO
                    }
                } else {
                    let request_index = out.store.len();
                    out.store.push(StoreRequest {
                        op: StoreOp::ReadWord,
                        reference: incoming.reference,
                        offset,
                        ..Default::default()
                    });
                    if faults.store_at == Some(request_index) {
                        return Err(Status::ServiceError);
                    }
                    *body
                        .and_then(|b| b.get(offset as usize))
                        .ok_or(Status::ServiceError)?
                };
                Self::apply_memory_effect(
                    &mut self.memory,
                    MemoryEffect {
                        address: incoming.base + offset,
                        write: true,
                        value,
                    },
                    faults,
                    &mut out,
                )?;
                if offset == 0 {
                    incoming.representation = value;
                }
            }
            self.invalidate_prepared_slot(slot);
            self.entries[slot] = Some(incoming).into();
            self.persistent_roots[slot] = !incoming.new;
            self.state.selected = Some(incoming).into();
            Ok(incoming.reference)
        })();
        out.response = match result {
            Ok(r) => Response::ok(r),
            Err(e) => Response::error(e),
        };
        out
    }
    pub(super) fn memory_effect(
        &mut self,
        effect: MemoryEffect,
        faults: Faults,
        out: &mut Outcome,
    ) -> Result<Word, Status> {
        Self::apply_memory_effect(&mut self.memory, effect, faults, out)
    }
    fn apply_memory_effect(
        memory: &mut [Word],
        effect: MemoryEffect,
        faults: Faults,
        out: &mut Outcome,
    ) -> Result<Word, Status> {
        let n = out.memory.len();
        out.memory.push(effect);
        if faults.memory_at == Some(n) {
            return Err(Status::MemoryError);
        }
        if effect.write {
            memory[effect.address as usize] = effect.value;
        }
        Ok(memory[effect.address as usize])
    }
    pub(super) fn store_effect(
        &mut self,
        request: StoreRequest,
        faults: Faults,
        out: &mut Outcome,
    ) -> Result<StoreReply, Status> {
        let n = out.store.len();
        out.store.push(request);
        let reply = self.store.request(request, faults.store_at == Some(n));
        if reply.status != Status::Ok {
            Err(
                if request.op == StoreOp::Metadata && reply.status == Status::InvalidReference {
                    Status::InvalidReference
                } else {
                    Status::ServiceError
                },
            )
        } else {
            Ok(reply)
        }
    }
}
