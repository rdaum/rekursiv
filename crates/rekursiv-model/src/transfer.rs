use super::*;
impl Model {
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
                        self.state.selected = Some(e);
                        return Ok(e.reference);
                    }
                    Err(Status::NotResident) => {}
                    Err(e) => return Err(e),
                }
            } else if c.pager != Pager::Allocate {
                return Err(Status::BadCommand);
            }
            let mut incoming = if c.pager == Pager::Allocate {
                if self.next_identity > ID_MASK {
                    return Err(Status::IdentityExhausted);
                }
                Entry {
                    reference: Word::reference(self.next_identity, c.alloc_scan)?,
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
                self.next_identity += 1;
            }
            let slot = self.slot(incoming.reference);
            if let Some(victim) = self.entries[slot] {
                if victim.new || victim.modified {
                    if victim.base as u64 + victim.size as u64 > self.memory.len() as u64 {
                        return Err(Status::BoundsError);
                    }
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
                    let mut edges = vec![victim.class];
                    if victim.reference.bits() & (1 << 37) != 0 {
                        edges.extend(
                            &self.memory
                                [victim.base as usize..(victim.base + victim.size) as usize],
                        );
                    }
                    for edge in edges {
                        let slot = self.slot(edge);
                        if self.entries[slot].is_some_and(|e| e.reference == edge) {
                            self.persistent_roots[slot] = true;
                        }
                    }
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
            self.entries[slot] = Some(incoming);
            self.persistent_roots[slot] = !incoming.new;
            self.state.selected = Some(incoming);
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
