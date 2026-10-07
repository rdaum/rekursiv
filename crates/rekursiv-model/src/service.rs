//! Bootstrap and maintenance services for the object-memory model.
#![deny(missing_docs)]

use crate::{MemoryEffect, Model, Outcome};
use rekursiv_asm::{Response, Service, Status, Word};

impl Model {
    /// Execute a bootstrap or maintenance service and report attempted RAM traffic.
    ///
    /// Services install or invalidate mappings, configure compact classes,
    /// reserve identities, and read or initialize raw memory. `BeginRecovery`
    /// and `EndRecovery` control the maintenance lock. `SetBodyCursor` requires
    /// that lock; the other operations enforce their own command restrictions.
    /// Raw writes cannot overwrite an installed object's body.
    ///
    /// `memory_error` fails a requested RAM access before publication. On error,
    /// architectural state remains unchanged; an attempted RAM request can still
    /// appear in the outcome. These services do not walk the object graph.
    pub fn service(&mut self, s: Service, memory_error: bool) -> Outcome {
        let mut memory = None;
        let mut result_data = Word::NIL;
        let result = (|| {
            match s {
                Service::ReserveIdentities(next) => {
                    if next == 0 || next > 1 << 37 {
                        return Err(Status::BadValue);
                    }
                    self.next_identity = self.next_identity.max(next);
                }
                Service::BeginRecovery => {
                    if self.maintenance {
                        return Err(Status::BadCommand);
                    }
                    self.maintenance = true;
                }
                Service::EndRecovery => {
                    if !self.maintenance {
                        return Err(Status::BadCommand);
                    }
                    self.maintenance = false;
                }
                Service::SetBodyCursor(cursor) => {
                    if !self.maintenance {
                        return Err(Status::BadCommand);
                    }
                    if cursor as usize > self.memory.len()
                        || self
                            .entries
                            .iter()
                            .flatten()
                            .any(|e| e.base as u64 + e.size as u64 > cursor as u64)
                    {
                        return Err(Status::BoundsError);
                    }
                    self.body_cursor = cursor;
                }
                Service::ReadMemory { address } => {
                    if address as usize >= self.memory.len() {
                        return Err(Status::BoundsError);
                    }
                    memory = Some(MemoryEffect {
                        address,
                        write: false,
                        value: Word::ZERO,
                    });
                    if memory_error {
                        return Err(Status::MemoryError);
                    }
                    result_data = self.memory[address as usize];
                }

                Service::Install(e) => {
                    e.validate()?;
                    self.next_identity = self.next_identity.max(e.reference.identity()? + 1);
                    self.body_cursor = self.body_cursor.max(e.base + e.size);
                    let slot = self.slot(e.reference);
                    self.invalidate_prepared_slot(slot);
                    self.entries[slot] = Some(e);
                    self.persistent_roots[slot] = !e.new;
                    if self
                        .state
                        .selected
                        .is_some_and(|old| old.reference == e.reference)
                    {
                        self.state.selected = Some(e);
                    }
                }
                Service::Invalidate(r) => {
                    r.identity()?;
                    let slot = self.slot(r);
                    if !self.entries[slot].is_some_and(|e| e.reference == r) {
                        return Err(Status::NotResident);
                    }
                    self.invalidate_prepared_slot(slot);
                    self.entries[slot] = None;
                }
                Service::CompactClass { code, class } => {
                    if code > 3 || !class.is_reference() {
                        return Err(Status::BadValue);
                    }
                    self.classes[code as usize] = Some(class);
                    if let Some(mut e) = self.state.selected {
                        if e.reference.is_compact()
                            && ((e.reference.bits() >> 32) & 63) == code as u64
                        {
                            e.class = class;
                            self.state.selected = Some(e);
                        }
                    }
                }
                Service::WriteMemory { address, value } => {
                    if address as usize >= self.memory.len() {
                        return Err(Status::BoundsError);
                    }
                    if self.entries.iter().flatten().any(|e| {
                        address >= e.base && (address as u64) < e.base as u64 + e.size as u64
                    }) {
                        return Err(Status::BadCommand);
                    }
                    memory = Some(MemoryEffect {
                        address,
                        write: true,
                        value,
                    });
                    if memory_error {
                        return Err(Status::MemoryError);
                    }
                    self.memory[address as usize] = value;
                }
            }
            Ok(result_data)
        })();
        Outcome {
            response: match result {
                Ok(w) => Response::ok(w),
                Err(e) => Response::error(e),
            },
            memory: memory.into_iter().collect(),
            store: Vec::new(),
        }
    }
}
