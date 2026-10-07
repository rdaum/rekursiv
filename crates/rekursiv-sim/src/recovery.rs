//! Stopped-mutator service recovery. Bodies move only through RTL service commands.
use super::*;
use std::collections::{BTreeSet, VecDeque};

pub(super) struct RecoveryJob {
    operations: Vec<Service>,
    next: usize,
    write_number: usize,
    fail_write: Option<usize>,
    live: BTreeSet<u64>,
    expected: Model,
    report: RecoveryReport,
    mode: RecoveryMode,
}
impl Harness<'_> {
    pub fn recovery_pending(&self) -> bool {
        self.recovery_job.is_some()
    }
    pub fn recover(&mut self, mode: RecoveryMode, roots: &Roots) -> Result<RecoveryReport> {
        self.recover_with_fault(mode, roots, None)
    }
    /// Inject one failure at a zero-based memory transaction in the recovery.
    pub fn recover_with_fault(
        &mut self,
        mode: RecoveryMode,
        roots: &Roots,
        fail_at: Option<usize>,
    ) -> Result<RecoveryReport> {
        ensure!(
            self.recovery_job.is_none() && self.rtl.dbg_maintenance_o == 0,
            "recovery is already active"
        );
        ensure!(
            self.rtl.cmd_valid_i == 0 && self.rtl.svc_valid_i == 0,
            "input operation still pending"
        );
        ensure!(
            self.pending.is_none() && self.pending_store.is_none(),
            "recovery requires drained external transactions"
        );
        self.processor_host_access()?;
        let mut retained = roots.clone();
        retained.language.extend(self.processor_roots()?);
        self.rtl.run_i = 0;
        self.rtl.eval();
        ensure!(
            self.rtl.svc_ready_o != 0,
            "recovery requires a completed command boundary"
        );
        ensure!(
            self.service_inner(Service::BeginRecovery, false)?.status == Status::Ok,
            "cannot acquire maintenance lock"
        );
        match self.prepare_recovery(mode, &retained, fail_at) {
            Ok(job) => self.recovery_job = Some(job),
            Err(e) => {
                // Preparation never modifies body memory, pager entries, or store records.
                ensure!(
                    self.service_inner(Service::EndRecovery, false)?.status == Status::Ok,
                    "cannot release maintenance lock"
                );
                return Err(e);
            }
        }
        self.resume_recovery()
    }
    fn prepare_recovery(
        &mut self,
        mode: RecoveryMode,
        roots: &Roots,
        fail_at: Option<usize>,
    ) -> Result<RecoveryJob> {
        let state = self.snapshot()?;
        let mut root_words = roots.values();
        root_words.extend(state.vr);
        if let Some(e) = state.selected {
            root_words.extend([e.reference, e.class]);
        }
        for code in 0..4 {
            self.rtl.dbg_class_code_i = code;
            self.rtl.eval();
            if self.rtl.dbg_class_valid_o != 0 {
                root_words.push(Word::from_bits(self.rtl.dbg_compact_class_o)?);
            }
        }
        let mut entries = Vec::new();
        for slot in 0..PAGER_ENTRIES {
            self.rtl.dbg_slot_i = slot as u8;
            self.rtl.eval();
            if self.rtl.dbg_valid_o != 0 {
                entries.push(
                    Entry {
                        reference: Word::from_bits(self.rtl.dbg_entry_ref_o)?,
                        class: Word::from_bits(self.rtl.dbg_entry_class_o)?,
                        size: self.rtl.dbg_entry_size_o,
                        base: self.rtl.dbg_entry_base_o,
                        representation: Word::from_bits(self.rtl.dbg_entry_repr_o)?,
                        new: self.rtl.dbg_entry_flags_o & 1 != 0,
                        modified: self.rtl.dbg_entry_flags_o & 2 != 0,
                        cond: self.rtl.dbg_entry_flags_o & 4 != 0,
                    }
                    .validate()?,
                );
            }
        }
        entries.sort_by_key(|e| (e.base, e.reference.bits()));
        let mut images = self.store.records.clone();
        for (id, record) in &images {
            ensure!(
                record.reference.identity()? == *id
                    && record.class.is_reference()
                    && record.body.len() < ADDRESS_LIMIT as usize,
                "invalid backing metadata"
            );
        }
        let mut reads = 0;
        let mut end = 0;
        for e in &entries {
            ensure!(
                e.base as u64 + e.size as u64 <= self.backing.len() as u64,
                "resident body exceeds memory"
            );
            if e.size != 0 {
                ensure!(e.base >= end, "overlapping resident bodies");
                end = e.base + e.size;
            }
            let mut body = Vec::new();
            for address in e.base..e.base + e.size {
                let response =
                    self.service_inner(Service::ReadMemory { address }, fail_at == Some(reads))?;
                reads += 1;
                ensure!(
                    response.status == Status::Ok,
                    "recovery snapshot: {}",
                    response.status
                );
                body.push(response.data);
            }
            ensure!(
                body.first().copied().unwrap_or(Word::NIL) == e.representation,
                "incoherent first-field cache"
            );
            let image = Record {
                reference: e.reference,
                class: e.class,
                cond: e.cond,
                body,
            };
            match images.get(&e.reference.identity()?) {
                Some(old) => {
                    ensure!(
                        old.reference == e.reference,
                        "resident/backing scan tags disagree"
                    );
                    ensure!(
                        e.new || e.modified || old == &image,
                        "clean resident differs from backing record"
                    );
                }
                None => ensure!(e.new || e.modified, "clean resident has no backing record"),
            }
            images.insert(e.reference.identity()?, image);
        }
        let mut live = BTreeSet::new();
        if mode == RecoveryMode::Compact {
            live.extend(images.keys().copied());
        } else {
            let mut queue: VecDeque<_> = root_words.into();
            while let Some(value) = queue.pop_front() {
                if value.bits() >> 38 != 2 {
                    continue;
                }
                let id = value.identity()?;
                let image = images
                    .get(&id)
                    .ok_or_else(|| eyre!("dangling root or field: {value}"))?;
                ensure!(
                    image.reference == value,
                    "scan-tag mismatch in root or field: {value}"
                );
                if live.insert(id) {
                    queue.push_back(image.class);
                    if value.bits() & (1 << 37) != 0 {
                        queue.extend(image.body.iter().copied());
                    }
                }
            }
        }
        let mut operations: Vec<_> = entries
            .iter()
            .map(|e| Service::Invalidate(e.reference))
            .collect();
        let mut survivors = Vec::new();
        let mut cursor = 0;
        for mut e in entries.iter().copied() {
            if !live.contains(&e.reference.identity()?) {
                continue;
            }
            let image = &images[&e.reference.identity()?];
            e.base = cursor & (ADDRESS_LIMIT - 1);
            for value in &image.body {
                operations.push(Service::WriteMemory {
                    address: cursor,
                    value: *value,
                });
                cursor += 1;
            }
            survivors.push(e);
        }
        operations.extend(survivors.iter().copied().map(Service::Install));
        operations.push(Service::SetBodyCursor(cursor));
        let report = RecoveryReport {
            objects_before: images.len(),
            objects_after: live.len(),
            resident_before: entries.len(),
            resident_after: survivors.len(),
            cursor_before: self.rtl.dbg_body_cursor_o,
            cursor_after: cursor,
        };
        // The oracle computes graph reachability and the final layout independently.
        let mut expected = self.oracle.clone();
        let expected_report = expected.recover(mode, roots)?;
        ensure!(
            report == expected_report,
            "recovery plan differs from architectural model"
        );
        Ok(RecoveryJob {
            operations,
            next: 0,
            write_number: 0,
            fail_write: fail_at.and_then(|n| n.checked_sub(reads)),
            live,
            expected,
            report,
            mode,
        })
    }
    pub fn resume_recovery(&mut self) -> Result<RecoveryReport> {
        let mut job = self
            .recovery_job
            .take()
            .ok_or_else(|| eyre!("no recovery to resume"))?;
        if self.rtl.dbg_maintenance_o == 0 {
            self.recovery_job = Some(job);
            return Err(eyre!("maintenance lock was lost; recovery copies retained"));
        }
        while job.next < job.operations.len() {
            let operation = job.operations[job.next];
            let is_write = matches!(operation, Service::WriteMemory { .. });
            let fail = is_write && job.fail_write == Some(job.write_number);
            if fail {
                job.fail_write = None;
            }
            let result = self.service_inner(operation, fail);
            match result {
                Ok(response) if response.status == Status::Ok => {}
                other => {
                    self.recovery_job = Some(job);
                    return match other {Ok(r)=>Err(eyre!("recovery paused at {operation:?}: {}; resume_recovery retains the body copies",r.status)),Err(e)=>Err(e)};
                }
            }
            if is_write {
                job.write_number += 1;
            }
            job.next += 1;
        }
        self.store.records.retain(|id, _| job.live.contains(id));
        self.store.discard_staging();
        self.oracle
            .store
            .records
            .retain(|id, _| job.live.contains(id));
        self.oracle.store.discard_staging();
        let finish = (|| {
            let mut actual = self.oracle.clone();
            actual.maintenance = false;
            ensure!(
                actual == job.expected,
                "recovery sequence differs from independent atomic model"
            );
            self.compare_state()?;
            let response = self.service_inner(Service::EndRecovery, false)?;
            ensure!(response.status == Status::Ok, "cannot finish recovery");
            Ok(())
        })();
        if let Err(e) = finish {
            self.recovery_job = Some(job);
            return Err(e);
        }
        match job.mode {
            RecoveryMode::Compact => self.stats.compactions += 1,
            RecoveryMode::Collect => self.stats.collections += 1,
        };
        Ok(job.report)
    }
    /// Retry space exhaustion at command boundaries, retaining all input references as roots.
    pub fn execute_recovering(&mut self, command: Command, roots: &Roots) -> Result<Response> {
        let result = self.execute(command)?;
        if result.status != Status::OutOfSpace {
            return Ok(result);
        }
        let mut retained = roots.clone();
        retained.pending.push(command);
        self.recover(RecoveryMode::Compact, &retained)?;
        let result = self.execute(command)?;
        if result.status != Status::OutOfSpace {
            return Ok(result);
        }
        self.recover(RecoveryMode::Collect, &retained)?;
        self.execute(command)
    }
}
