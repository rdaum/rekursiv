//! Costs of OBJEKT's resident, transfer, directory, and exchange state machines.
use super::{Estimate, Pending};
use rekursiv_asm::{Command, Pager, Status, StoreOp, ID_MASK};
use rekursiv_model::{Model, Outcome};

pub(crate) enum ObjectPath {
    Resident,
    /// Carries the cycles OBJEKT spends finding a free identity.
    Transfer(u64),
    /// Carries the cycles OBJEKT spends scanning resident metadata.
    Directory(u64),
    Exchange,
}
/// FindObject reads the one slot its identity selects. NextObject probes the
/// slots of the following identities and stops at the first exact match.
fn directory_scan(model: &Model, c: Command) -> u64 {
    if c.pager == Pager::FindObject {
        return 1;
    }
    let slots = model.entries.len() as u64;
    let key = c.data.bits() & ID_MASK;
    (1..=slots)
        .find(|step| {
            let probe = key + step;
            model.entries[(probe % slots) as usize]
                .get()
                .is_some_and(|e| e.reference.bits() & ID_MASK == probe)
        })
        .unwrap_or(slots)
}
impl ObjectPath {
    pub(crate) fn capture(model: &Model, c: Command) -> Self {
        match c.pager {
            Pager::Allocate => Self::Transfer(model.allocation_skips()),
            Pager::Fetch if c.data.is_reference() && model.resolve(c.data).is_err() => {
                Self::Transfer(0)
            }
            Pager::NextObject | Pager::FindObject => Self::Directory(directory_scan(model, c)),
            Pager::Exchange => Self::Exchange,
            _ => Self::Resident,
        }
    }
}
impl Estimate {
    pub(crate) fn object(
        &mut self,
        path: ObjectPath,
        paged: bool,
        outcome: &Outcome,
        asynchronous: bool,
    ) {
        let ram = outcome.memory.len() as u64;
        let backing = outcome.store.len() as u64;
        let ram_cycles = ram * self.config.ram.cycles();
        let backing_cycles = backing * self.config.backing.cycles();
        self.work.ram_requests += ram;
        self.work.ram_cycles += ram_cycles;
        self.work.backing_requests += backing;
        self.work.backing_cycles += backing_cycles;
        // Each engine serializes its RAM and backing requests. Their sum is
        // sufficient here; Outcome does not promise cross-channel ordering.
        let mut service = 1 + ram_cycles + backing_cycles; // final response edge
                                                           // A command that pages a key waits one cycle for the pager table.
                                                           // Commands on the selected or prepared object read registers only.
        service += u64::from(paged);
        let status = outcome.response.status;
        let approximate = !matches!(status, Status::Ok | Status::OutOfSpace);
        match path {
            // Resident RAM completion is forwarded directly to LOGIK. A
            // stalled consumer retains the reply in RESPONSE in the RTL;
            // this estimator assumes no response-consumption backpressure.
            ObjectPath::Resident if ram != 0 => service -= 1,
            ObjectPath::Resident => (),
            ObjectPath::Transfer(skips) => {
                service += skips;
                // RESERVE and DONE/publication, plus one initial FILL.
                // Subsequent words go directly to WRITE (allocation) or
                // GET (refill). Victim reads are distinct.
                service += 2;
                if status == Status::Ok {
                    service += 1;
                }
            }
            ObjectPath::Directory(scan) => {
                service += 1; // DONE/publication
                if !matches!(status, Status::BadCommand | Status::BadValue) {
                    service += scan + 1; // resident scan, then engine start
                }
            }
            ObjectPath::Exchange => {
                // SELECT_META for each key, then DONE/publication. Equal keys
                // exit after the first selection, before opening a save batch.
                // One more to drop the second resident entry after the batch.
                service += 3 + u64::from(outcome.store.iter().any(|r| r.op == StoreOp::BeginBatch));
            }
        }
        self.work.approximate_objects += u64::from(approximate);
        self.cycles.object_issue += 1;
        if asynchronous {
            debug_assert!(self.pending.is_none());
            self.pending = Some(Pending {
                start: self.cycles.total(),
                service,
            });
            self.work.async_service += service;
        } else {
            self.cycles.object_wait += service;
        }
    }
}
