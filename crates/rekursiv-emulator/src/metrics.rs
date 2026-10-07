//! Passive observations at OBJEKT command boundaries. These counters never
//! participate in architectural state and require no per-object history.
use rekursiv_asm::{Command, Entry, Memory, Pager, Status, StoreOp};
use rekursiv_model::{Model, Outcome};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LookupStatistics {
    pub hits: u64,
    pub empty_misses: u64,
    pub collision_misses: u64,
    pub compacts: u64,
    pub invalid: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjectStatistics {
    /// Explicit Fetch and Probe* selections, not internal metadata revalidation.
    pub fetch: LookupStatistics,
    pub probe: LookupStatistics,
    pub errors: u64,
    pub out_of_space: u64,
    pub allocation_attempts: u64,
    pub allocations: u64,
    pub allocated_words: u64,
    pub refills: u64,
    pub refilled_words: u64,
    /// Completed replacements, split into mutually exclusive victim states.
    pub evicted_clean: u64,
    pub evicted_new: u64,
    pub evicted_modified: u64,
    pub field_reads: u64,
    pub cached_field_reads: u64,
    pub field_writes: u64,
    pub ram_reads: u64,
    pub ram_writes: u64,
    pub store_requests: u64,
    pub store_metadata: u64,
    pub store_reads: u64,
    pub store_writes: u64,
    pub store_save_commits: u64,
    pub gc_explicit: u64,
    pub gc_allocation: u64,
    pub gc_refill: u64,
    pub gc_reads: u64,
    pub gc_writes: u64,
    pub gc_reclaimed_words: u64,
    pub gc_discarded_entries: u64,
}

#[derive(Clone, Copy)]
enum Lookup {
    Hit,
    Empty,
    Collision,
    Compact,
    Invalid,
}
pub(crate) struct Observation {
    lookup: Option<Lookup>,
    victim: Option<Entry>,
}
impl Observation {
    pub fn capture(model: &Model, c: Command) -> Self {
        let key = match c.pager {
            Pager::Fetch | Pager::ProbeBus => Some(Ok(c.data)),
            Pager::ProbeVr => Some(Ok(model.state.vr[c.vr as usize])),
            Pager::ProbeType | Pager::ProbeRepresentation => Some(
                model
                    .state
                    .selected
                    .ok_or(Status::NoSelection)
                    .and_then(|e| model.resolve(e.reference))
                    .map(|e| {
                        if c.pager == Pager::ProbeType {
                            e.class
                        } else {
                            e.representation
                        }
                    }),
            ),
            _ => None,
        };
        let lookup = key.map(|key| match key {
            Ok(r) if r.is_reference() => {
                match model.entries[r.bits() as usize & (model.entries.len() - 1)] {
                    Some(e) if e.reference == r => Lookup::Hit,
                    Some(_) => Lookup::Collision,
                    None => Lookup::Empty,
                }
            }
            Ok(r) if r.is_compact() && model.resolve(r).is_ok() => Lookup::Compact,
            _ => Lookup::Invalid,
        });
        let slot = if c.pager == Pager::Allocate {
            Some(model.next_identity as usize & (model.entries.len() - 1))
        } else if c.pager == Pager::Fetch
            && matches!(lookup, Some(Lookup::Empty | Lookup::Collision))
        {
            Some(c.data.bits() as usize & (model.entries.len() - 1))
        } else {
            None
        };
        Self {
            lookup,
            victim: slot.and_then(|s| model.entries[s]),
        }
    }
}
impl LookupStatistics {
    fn observe(&mut self, lookup: Lookup) {
        match lookup {
            Lookup::Hit => self.hits += 1,
            Lookup::Empty => self.empty_misses += 1,
            Lookup::Collision => self.collision_misses += 1,
            Lookup::Compact => self.compacts += 1,
            Lookup::Invalid => self.invalid += 1,
        }
    }
    fn report(&self, out: &mut String, name: &str) {
        let stored = self.hits + self.empty_misses + self.collision_misses;
        let hit_rate = if stored == 0 {
            0.0
        } else {
            100.0 * self.hits as f64 / stored as f64
        };
        writeln!(out, "  {name}: {} hits, {} empty misses, {} collision misses ({hit_rate:.2}% stored-reference hit rate); {} compacts, {} invalid", self.hits, self.empty_misses, self.collision_misses, self.compacts, self.invalid).unwrap();
    }
}
impl ObjectStatistics {
    pub(crate) fn observe(
        &mut self,
        c: Command,
        before: Observation,
        out: &Outcome,
        model: &Model,
    ) {
        if let Some(lookup) = before.lookup {
            if c.pager == Pager::Fetch {
                self.fetch.observe(lookup);
            } else {
                self.probe.observe(lookup);
            }
        }
        self.allocation_attempts += u64::from(c.pager == Pager::Allocate);
        // Effects are issued requests, including any request which failed.
        // Store traffic also includes Exchange/directory work, not just refills.
        for effect in &out.memory {
            if effect.write {
                self.ram_writes += 1;
            } else {
                self.ram_reads += 1;
            }
        }
        self.store_requests += out.store.len() as u64;
        for request in &out.store {
            match request.op {
                StoreOp::Metadata => self.store_metadata += 1,
                StoreOp::ReadWord => self.store_reads += 1,
                StoreOp::WriteWord => self.store_writes += 1,
                StoreOp::CommitSave => self.store_save_commits += 1,
                _ => {}
            }
        }
        if out.response.status != Status::Ok {
            if out.response.status == Status::OutOfSpace {
                self.out_of_space += 1;
            } else {
                self.errors += 1;
            }
            return;
        }
        let refill = c.pager == Pager::Fetch
            && matches!(before.lookup, Some(Lookup::Empty | Lookup::Collision));
        if refill {
            self.refills += 1;
            self.refilled_words += u64::from(model.state.selected.unwrap().size);
        }
        if c.pager == Pager::Allocate {
            self.allocations += 1;
            self.allocated_words += u64::from(c.alloc_size);
        }
        if refill || c.pager == Pager::Allocate {
            if let Some(victim) = before.victim {
                if victim.new {
                    self.evicted_new += 1;
                } else if victim.modified {
                    self.evicted_modified += 1;
                } else {
                    self.evicted_clean += 1;
                }
            }
        }
        match c.memory {
            Memory::Read => {
                self.field_reads += 1;
                self.cached_field_reads += u64::from(out.memory.is_empty());
            }
            Memory::Write => self.field_writes += 1,
            Memory::None => {}
        }
    }
    /// Logical traffic uses five bytes per 40-bit word, not Rust's storage size
    /// or a board-specific DRAM/bus packing. Bootstrap traffic is excluded.
    pub fn report(&self, model: &Model) -> String {
        let mut out = String::new();
        let occupied = model.entries.iter().flatten().count();
        let resident_words: u64 = model
            .entries
            .iter()
            .flatten()
            .map(|e| u64::from(e.size))
            .sum();
        writeln!(
            out,
            "OBJEKT (command attempts, including retries; traffic excludes bootstrap):"
        )
        .unwrap();
        writeln!(out, "  pager: {} entries, {occupied} occupied; {resident_words} resident words; {} backing records", model.entries.len(), model.store.records.len()).unwrap();
        self.fetch.report(&mut out, "fetch");
        self.probe.report(&mut out, "probe");
        writeln!(
            out,
            "  completed evictions: {} clean, {} new, {} modified",
            self.evicted_clean, self.evicted_new, self.evicted_modified
        )
        .unwrap();
        writeln!(
            out,
            "  allocations: {} completed / {} attempts, {} words; refills: {}, {} words",
            self.allocations,
            self.allocation_attempts,
            self.allocated_words,
            self.refills,
            self.refilled_words
        )
        .unwrap();
        writeln!(
            out,
            "  fields: {} reads ({} cached first-word), {} writes",
            self.field_reads, self.cached_field_reads, self.field_writes
        )
        .unwrap();
        writeln!(out, "  backing requests: {} total, {} metadata, {} word reads, {} word writes, {} save commits", self.store_requests, self.store_metadata, self.store_reads, self.store_writes, self.store_save_commits).unwrap();
        writeln!(
            out,
            "  backing payload: {:.3} MiB read, {:.3} MiB written (5 bytes/word)",
            self.store_reads as f64 * 5.0 / 1048576.0,
            self.store_writes as f64 * 5.0 / 1048576.0
        )
        .unwrap();
        writeln!(
            out,
            "  RAM word requests: {} reads, {} writes; collector: {} reads, {} writes",
            self.ram_reads, self.ram_writes, self.gc_reads, self.gc_writes
        )
        .unwrap();
        writeln!(out, "  collection entries: {} explicit, {} allocation pressure, {} refill pressure; {} words reclaimed, {} mappings discarded at commit", self.gc_explicit, self.gc_allocation, self.gc_refill, self.gc_reclaimed_words, self.gc_discarded_entries).unwrap();
        write!(
            out,
            "  command outcomes: {} out-of-space, {} other errors",
            self.out_of_space, self.errors
        )
        .unwrap();
        out
    }
}
