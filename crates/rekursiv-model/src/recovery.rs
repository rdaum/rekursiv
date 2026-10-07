//! Atomic recovery specification, independent of the service command sequence.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use store::Record;
impl Model {
    pub fn recover(&mut self, mode: RecoveryMode, roots: &Roots) -> Result<RecoveryReport, Status> {
        let mut candidate = self.clone();
        let report = candidate.recover_inner(mode, roots)?;
        *self = candidate;
        Ok(report)
    }
    fn recover_inner(
        &mut self,
        mode: RecoveryMode,
        roots: &Roots,
    ) -> Result<RecoveryReport, Status> {
        let mut images: BTreeMap<u64, Record> = self.store.records.clone();
        for (id, record) in &images {
            if record.reference.identity()? != *id
                || !record.class.is_reference()
                || record.body.len() >= ADDRESS_LIMIT as usize
            {
                return Err(Status::InvalidReference);
            }
        }
        let mut resident: Vec<_> = self.entries.iter().flatten().copied().collect();
        resident.sort_by_key(|e| (e.base, e.reference.bits()));
        let mut previous_end = 0;
        for e in &resident {
            e.validate()?;
            let end = e.base as u64 + e.size as u64;
            if end > self.memory.len() as u64 {
                return Err(Status::BoundsError);
            }
            if e.size > 0 {
                if (e.base as u64) < previous_end {
                    return Err(Status::BadValue);
                }
                previous_end = end;
            }
            let body = self.memory[e.base as usize..end as usize].to_vec();
            if body.first().copied().unwrap_or(Word::NIL) != e.representation {
                return Err(Status::BadValue);
            }
            let id = e.reference.identity()?;
            let current = Record {
                reference: e.reference,
                class: e.class,
                cond: e.cond,
                body,
            };
            if let Some(old) = images.get(&id) {
                if old.reference != e.reference {
                    return Err(Status::InvalidReference);
                }
                if !e.new && !e.modified && *old != current {
                    return Err(Status::BadValue);
                }
            } else if !e.new && !e.modified {
                return Err(Status::InvalidReference);
            }
            images.insert(id, current);
        }
        let objects_before = images.len();
        let mut live = BTreeSet::new();
        if mode == RecoveryMode::Compact {
            live.extend(images.keys().copied());
        } else {
            let mut work = roots.values();
            work.extend(self.state.vr);
            if self.state.prepared.status == Status::Ok {
                work.push(self.state.prepared.reference);
            }
            work.extend(self.classes.iter().flatten().copied());
            if let Some(e) = self.state.selected {
                work.extend([e.reference, e.class]);
            }
            while let Some(word) = work.pop() {
                if word.bits() >> 38 != 2 {
                    continue;
                }
                let id = word.identity()?;
                let object = images
                    .get(&id)
                    .filter(|r| r.reference == word)
                    .ok_or(Status::InvalidReference)?;
                if !live.insert(id) {
                    continue;
                }
                work.push(object.class);
                if word.bits() & (1 << 37) != 0 {
                    work.extend(object.body.iter().copied());
                }
            }
        }
        let report = RecoveryReport {
            objects_before,
            objects_after: live.len(),
            resident_before: resident.len(),
            resident_after: resident
                .iter()
                .filter(|e| live.contains(&e.reference.identity().unwrap()))
                .count(),
            cursor_before: self.body_cursor,
            cursor_after: resident
                .iter()
                .filter(|e| live.contains(&e.reference.identity().unwrap()))
                .map(|e| e.size)
                .sum(),
        };
        self.entries.fill(None);
        let mut base = 0;
        for mut e in resident {
            if !live.contains(&e.reference.identity()?) {
                continue;
            }
            let body = &images[&e.reference.identity()?].body;
            self.memory[base as usize..base as usize + body.len()].copy_from_slice(body);
            e.base = base & (ADDRESS_LIMIT - 1);
            base += e.size;
            let slot = self.slot(e.reference);
            self.entries[slot] = Some(e);
            if self
                .state
                .selected
                .is_some_and(|s| s.reference == e.reference)
            {
                self.state.selected = Some(e);
            }
        }
        // The halted service recovery utility republishes mappings with
        // Install. Unlike machine GC, that interface invalidates derived
        // addresses and requires a new PREPARE after maintenance.
        if self.state.prepared.status == Status::Ok {
            self.state.prepared.status = Status::NotResident;
        }
        self.body_cursor = base;
        self.store.records.retain(|id, _| live.contains(id));
        self.store.discard_staging();
        self.maintenance = false;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_graph_is_atomic_but_compaction_needs_no_reachability() {
        let mut model = Model::new(4, 32);
        let class = Word::reference(1, true).unwrap();
        let base = Entry {
            reference: class,
            class,
            size: 0,
            base: 0,
            representation: Word::NIL,
            new: true,
            modified: false,
            cond: false,
        };
        model.service(Service::Install(base), false);
        let dangling = Word::reference(3, true).unwrap();
        model.memory[10] = dangling;
        let object = Entry {
            reference: Word::reference(2, true).unwrap(),
            size: 1,
            base: 10,
            representation: dangling,
            ..base
        };
        model.service(Service::Install(object), false);
        model.state.selected = Some(object);
        let before = model.clone();
        assert_eq!(
            model.recover(RecoveryMode::Collect, &Roots::default()),
            Err(Status::InvalidReference)
        );
        assert_eq!(model, before);
        let report = model
            .recover(RecoveryMode::Compact, &Roots::default())
            .unwrap();
        assert_eq!(report.cursor_after, 1);
        assert_eq!(model.memory[0], dangling);
        assert_eq!(model.state.selected.unwrap().base, 0);
        assert_eq!(model.next_identity, 3);
    }
}
