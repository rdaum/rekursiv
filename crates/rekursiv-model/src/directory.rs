//! Read-only key-index oracle. Class filtering remains a microcode operation.
use super::*;
impl Model {
    pub(super) fn execute_directory(&mut self, c: Command, faults: Faults) -> Outcome {
        let mut out = Outcome {
            response: Response::ok(Word::NIL),
            memory: vec![],
            store: vec![],
        };
        let result = (|| {
            let c = c.validate()?;
            let cursor = c.data.bits() & ID_MASK;
            let next = c.pager == Pager::NextObject;
            let mut found = self
                .entries
                .iter()
                .flatten()
                .filter(|e| {
                    let id = e.reference.identity().unwrap();
                    if next {
                        id > cursor
                    } else {
                        id == cursor
                    }
                })
                .min_by_key(|e| e.reference.identity().unwrap())
                .map(|e| (e.reference, e.class));
            if (next && cursor < ID_MASK) || (!next && found.is_none()) {
                let reply = self
                    .store_effect(
                        StoreRequest {
                            op: if next {
                                StoreOp::NextRecord
                            } else {
                                StoreOp::FindRecord
                            },
                            reference: Word::raw(cursor)?,
                            ..Default::default()
                        },
                        faults,
                        &mut out,
                    )
                    .map_err(|_| Status::ServiceError)?;
                if reply.reference != Word::NIL {
                    let id = reply
                        .reference
                        .identity()
                        .map_err(|_| Status::ServiceError)?;
                    if !reply.class.is_reference()
                        || (if next { id <= cursor } else { id != cursor })
                    {
                        return Err(Status::ServiceError);
                    }
                    if found.is_none_or(|(key, _)| id < key.identity().unwrap()) {
                        found = Some((reply.reference, reply.class));
                    }
                }
            }
            let (reference, class) = found.unwrap_or((Word::NIL, Word::NIL));
            self.state.vr[c.vr as usize] = class;
            Ok(reference)
        })();
        out.response = match result {
            Ok(r) => Response::ok(r),
            Err(e) => Response::error(e),
        };
        out
    }
}
