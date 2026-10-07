//! Transaction oracle for the generic two-binding exchange engine. The RTL
//! supplies all traffic; this model predicts its effects and failure boundary.
use super::*;
impl Model {
    pub(super) fn execute_exchange(&mut self, c: Command, faults: Faults) -> Outcome {
        let mut out = Outcome {
            response: Response::ok(Word::NIL),
            memory: vec![],
            store: vec![],
        };
        let mut begun = false;
        let result = (|| {
            let keys = [c.data, self.state.vr[c.vr as usize]];
            if keys.iter().any(|k| !k.is_reference()) {
                return Err(Status::InvalidReference);
            }
            if (keys[0].bits() ^ keys[1].bits()) & (1 << 37) != 0 {
                return Err(Status::BadValue);
            }
            let mut sources = Vec::new();
            for key in keys {
                let resident = self.entries[self.slot(key)].filter(|e| e.reference == key);
                let entry = if let Some(entry) = resident {
                    if entry.base as usize + entry.size as usize > self.memory.len() {
                        return Err(Status::BoundsError);
                    }
                    entry
                } else {
                    let reply = self.store_effect(
                        StoreRequest {
                            reference: key,
                            ..Default::default()
                        },
                        faults,
                        &mut out,
                    )?;
                    if reply.reference != key || !reply.class.is_reference() {
                        return Err(Status::InvalidReference);
                    }
                    Entry {
                        reference: key,
                        class: reply.class,
                        size: reply.size,
                        base: 0,
                        representation: Word::NIL,
                        new: false,
                        modified: false,
                        cond: reply.cond,
                    }
                };
                sources.push((entry, resident.is_some()));
                if keys[0] == keys[1] {
                    return Ok(keys[0]);
                }
            }
            self.store_effect(
                StoreRequest {
                    op: StoreOp::BeginBatch,
                    ..Default::default()
                },
                faults,
                &mut out,
            )?;
            begun = true;
            let mut edges = vec![];
            for (which, &(source, resident)) in sources.iter().enumerate() {
                let destination = keys[1 - which];
                self.store_effect(
                    StoreRequest {
                        op: StoreOp::BeginSave,
                        reference: destination,
                        class: source.class,
                        size: source.size,
                        cond: source.cond,
                        ..Default::default()
                    },
                    faults,
                    &mut out,
                )?;
                edges.push(source.class);
                for offset in 0..source.size {
                    let data = if resident {
                        self.memory_effect(
                            MemoryEffect {
                                address: source.base + offset,
                                write: false,
                                value: Word::ZERO,
                            },
                            faults,
                            &mut out,
                        )?
                    } else {
                        self.store_effect(
                            StoreRequest {
                                op: StoreOp::ReadWord,
                                reference: source.reference,
                                offset,
                                ..Default::default()
                            },
                            faults,
                            &mut out,
                        )?
                        .data
                    };
                    self.store_effect(
                        StoreRequest {
                            op: StoreOp::WriteWord,
                            reference: destination,
                            offset,
                            data,
                            ..Default::default()
                        },
                        faults,
                        &mut out,
                    )?;
                    if source.reference.bits() & (1 << 37) != 0 {
                        edges.push(data);
                    }
                }
                self.store_effect(
                    StoreRequest {
                        op: StoreOp::CommitSave,
                        reference: destination,
                        ..Default::default()
                    },
                    faults,
                    &mut out,
                )?;
            }
            self.store_effect(
                StoreRequest {
                    op: StoreOp::CommitBatch,
                    ..Default::default()
                },
                faults,
                &mut out,
            )?;
            for (slot, entry) in self.entries.iter_mut().enumerate() {
                if let Some(e) = entry {
                    self.persistent_roots[slot] |= edges.contains(&e.reference);
                    if keys.contains(&e.reference) {
                        *entry = None;
                    }
                }
            }
            if self
                .state
                .selected
                .is_some_and(|e| keys.contains(&e.reference))
            {
                self.state.selected = None;
            }
            if keys.contains(&self.state.prepared.reference)
                && self.state.prepared.status == Status::Ok
            {
                self.state.prepared.status = Status::NotResident;
            }
            Ok(keys[0])
        })();
        if result.is_err() && begun {
            let _ = self.store_effect(
                StoreRequest {
                    op: StoreOp::AbortBatch,
                    ..Default::default()
                },
                faults,
                &mut out,
            );
        }
        out.response = match result {
            Ok(value) => Response::ok(value),
            Err(status) => Response::error(status),
        };
        out
    }
}
