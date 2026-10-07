//! Atomic object records behind the streaming backing-store interface.
#![deny(missing_docs)]

use rekursiv_asm::*;
use std::collections::BTreeMap;
/// One complete object saved under its stable identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Full tagged reference, including the scan bit.
    pub reference: Word,
    /// Class reference saved with the body.
    pub class: Word,
    /// Opaque condition bit preserved by transfers and identity exchange.
    pub cond: bool,
    /// Object body in field order; the first word supplies cached representation metadata.
    pub body: Vec<Word>,
}
/// In-memory implementation of the streaming object backing-store protocol.
///
/// A save stages body words until `CommitSave`. Within a batch, completed saves
/// stay private until `CommitBatch` publishes them together. Preview never
/// publishes effects, so an executor can hold a response under backpressure.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackingStore {
    /// Committed records indexed by identity. Incomplete saves and batches are separate.
    pub records: BTreeMap<u64, Record>,
    staging: Option<(Record, usize)>,
    batch: Option<BTreeMap<u64, Record>>,
}
impl BackingStore {
    /// Number of complete records awaiting batch publication (device diagnostics).
    pub fn staged_record_count(&self) -> Option<usize> {
        self.batch.as_ref().map(BTreeMap::len)
    }
    /// Abandon incomplete saves and unpublished batches, preserving committed records.
    pub fn discard_staging(&mut self) {
        self.staging = None;
        self.batch = None;
    }
    /// Compute the eventual reply without publication. A device adapter can
    /// hold this reply under backpressure, then apply request at the completion
    /// handshake. No complete-image snapshot is needed for each transferred word.
    pub fn preview(&self, req: StoreRequest, fail: bool) -> StoreReply {
        let mut reply = StoreReply::default();
        let result = (|| {
            if fail {
                return Err(Status::ServiceError);
            }
            // Batch operations provide generic storage publication, not object
            // transformations. The requester supplies every record and word.
            match req.op {
                StoreOp::NextRecord | StoreOp::FindRecord => {
                    let id = req.reference.bits();
                    if id > ID_MASK {
                        return Err(Status::BadValue);
                    }
                    let found = if req.op == StoreOp::NextRecord {
                        self.records
                            .range((std::ops::Bound::Excluded(id), std::ops::Bound::Unbounded))
                            .next()
                            .map(|(_, r)| r)
                    } else {
                        self.records.get(&id)
                    };
                    reply.reference = Word::NIL;
                    reply.class = Word::NIL;
                    if let Some(record) = found {
                        reply.reference = record.reference;
                        reply.class = record.class;
                        reply.size = record.body.len() as u32;
                        reply.cond = record.cond;
                    }
                    return Ok(());
                }
                StoreOp::BeginBatch | StoreOp::AbortBatch => return Ok(()),
                StoreOp::CommitBatch => {
                    if self.staging.is_some() || self.batch.is_none() {
                        return Err(Status::ServiceError);
                    }
                    return Ok(());
                }
                _ => {}
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
                }
                StoreOp::WriteWord => {
                    let (record, next) = self.staging.as_ref().ok_or(Status::ServiceError)?;
                    if record.reference != req.reference
                        || req.offset as usize != *next
                        || *next >= record.body.len()
                    {
                        return Err(Status::ServiceError);
                    }
                }
                StoreOp::CommitSave => {
                    let (record, next) = self.staging.as_ref().ok_or(Status::ServiceError)?;
                    if record.reference != req.reference || *next != record.body.len() {
                        return Err(Status::ServiceError);
                    }
                }
                StoreOp::BeginBatch
                | StoreOp::CommitBatch
                | StoreOp::AbortBatch
                | StoreOp::NextRecord
                | StoreOp::FindRecord => unreachable!(),
            }
            Ok(())
        })();
        if let Err(e) = result {
            reply.status = e;
        }
        reply
    }

    /// Validate a request and publish its effects once if the reply succeeds.
    ///
    /// `fail` injects a service error without mutation. Otherwise this applies
    /// the same checks as [`Self::preview`]. Errors preserve both staging and
    /// published records. A caller that retries a held transaction must use
    /// `preview` until the completion boundary, then call this method once.
    pub fn request(&mut self, req: StoreRequest, fail: bool) -> StoreReply {
        let reply = self.preview(req, fail);
        if reply.status != Status::Ok {
            return reply;
        }
        match req.op {
            StoreOp::BeginBatch => {
                self.discard_staging();
                self.batch = Some(BTreeMap::new());
            }
            StoreOp::AbortBatch => self.discard_staging(),
            StoreOp::CommitBatch => self.records.append(&mut self.batch.take().unwrap()),
            StoreOp::BeginSave => {
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
                let (record, next) = self.staging.as_mut().unwrap();
                record.body[*next] = req.data;
                *next += 1;
            }
            StoreOp::CommitSave => {
                let (record, _) = self.staging.take().unwrap();
                let id = record.reference.identity().unwrap();
                if let Some(batch) = &mut self.batch {
                    batch.insert(id, record);
                } else {
                    self.records.insert(id, record);
                }
            }
            StoreOp::Metadata | StoreOp::ReadWord | StoreOp::NextRecord | StoreOp::FindRecord => {}
        }
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_completion_does_not_publish_or_advance_staging() {
        let reference = Word::reference(7, true).unwrap();
        let mut store = BackingStore::default();
        for op in [
            StoreOp::BeginBatch,
            StoreOp::BeginSave,
            StoreOp::WriteWord,
            StoreOp::CommitSave,
            StoreOp::CommitBatch,
        ] {
            let request = StoreRequest {
                op,
                reference,
                class: reference,
                size: 1,
                data: Word::signed(42),
                ..Default::default()
            };
            let before = store.clone();
            for _ in 0..3 {
                assert_eq!(store.preview(request, false).status, Status::Ok);
                assert_eq!(store, before);
            }
            assert_eq!(store.preview(request, true).status, Status::ServiceError);
            assert_eq!(store, before);
            assert_eq!(store.request(request, false).status, Status::Ok);
            if op != StoreOp::CommitBatch {
                assert!(store.records.is_empty());
            }
        }
        assert_eq!(store.records[&7].body, [Word::signed(42)]);
        let invalid = StoreRequest {
            op: StoreOp::WriteWord,
            reference,
            ..Default::default()
        };
        let before = store.clone();
        assert_eq!(store.preview(invalid, false).status, Status::ServiceError);
        assert_eq!(store.request(invalid, false).status, Status::ServiceError);
        assert_eq!(store, before);
    }
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
