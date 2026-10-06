//! Atomic object records behind the streaming backing-store interface.
use rekursiv_asm::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub reference: Word,
    pub class: Word,
    pub cond: bool,
    pub body: Vec<Word>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackingStore {
    pub records: BTreeMap<u64, Record>,
    staging: Option<(Record, usize)>,
}
impl BackingStore {
    pub fn discard_staging(&mut self) {
        self.staging = None;
    }
    pub fn request(&mut self, req: StoreRequest, fail: bool) -> StoreReply {
        let mut reply = StoreReply::default();
        let result = (|| {
            if fail {
                return Err(Status::ServiceError);
            }
            let id = req.reference.identity()?;
            match req.op {
                StoreOp::Metadata | StoreOp::ReadWord => {
                    let record = self
                        .records
                        .get(&id)
                        .filter(|r| r.reference == req.reference)
                        .ok_or(Status::InvalidReference)?;
                    if req.op == StoreOp::Metadata {
                        reply.reference = record.reference;
                        reply.class = record.class;
                        reply.size = record.body.len() as u32;
                        reply.cond = record.cond;
                    } else {
                        reply.data = *record
                            .body
                            .get(req.offset as usize)
                            .ok_or(Status::ServiceError)?;
                    }
                }
                StoreOp::BeginSave => {
                    if !req.class.is_reference() || req.size >= ADDRESS_LIMIT {
                        return Err(Status::ServiceError);
                    }
                    // A new transaction discards an abandoned unpublished save.
                    self.staging = Some((
                        Record {
                            reference: req.reference,
                            class: req.class,
                            cond: req.cond,
                            body: vec![Word::ZERO; req.size as usize],
                        },
                        0,
                    ));
                }
                StoreOp::WriteWord => {
                    let (record, next) = self.staging.as_mut().ok_or(Status::ServiceError)?;
                    if record.reference != req.reference
                        || req.offset as usize != *next
                        || *next >= record.body.len()
                    {
                        return Err(Status::ServiceError);
                    }
                    record.body[*next] = req.data;
                    *next += 1;
                }
                StoreOp::CommitSave => {
                    let (record, next) = self.staging.as_ref().ok_or(Status::ServiceError)?;
                    if record.reference != req.reference || *next != record.body.len() {
                        return Err(Status::ServiceError);
                    }
                    let (record, _) = self.staging.take().unwrap();
                    self.records.insert(id, record);
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            reply.status = e;
        }
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_publication_is_atomic_and_ordered() {
        let reference = Word::reference(1, true).unwrap();
        let mut store = BackingStore::default();
        store.records.insert(
            1,
            Record {
                reference,
                class: reference,
                cond: false,
                body: vec![Word::signed(10)],
            },
        );
        let original = store.records.clone();
        let begin = StoreRequest {
            op: StoreOp::BeginSave,
            reference,
            class: reference,
            size: 2,
            ..Default::default()
        };
        let put = StoreRequest {
            op: StoreOp::WriteWord,
            reference,
            data: Word::signed(20),
            ..Default::default()
        };
        let commit = StoreRequest {
            op: StoreOp::CommitSave,
            reference,
            ..Default::default()
        };
        assert_eq!(store.request(begin, false).status, Status::Ok);
        assert_eq!(store.request(commit, false).status, Status::ServiceError);
        assert_eq!(store.request(put, false).status, Status::Ok);
        assert_eq!(store.request(put, false).status, Status::ServiceError);
        assert_eq!(
            store
                .request(StoreRequest { offset: 1, ..put }, true)
                .status,
            Status::ServiceError
        );
        assert_eq!(store.records, original);
        assert_eq!(
            store
                .request(StoreRequest { offset: 1, ..put }, false)
                .status,
            Status::Ok
        );
        assert_eq!(store.request(commit, true).status, Status::ServiceError);
        assert_eq!(store.records, original);
        assert_eq!(store.request(commit, false).status, Status::Ok);
        assert_eq!(store.records[&1].body, vec![Word::signed(20); 2]);
        assert_eq!(store.request(commit, false).status, Status::ServiceError);
    }
}
