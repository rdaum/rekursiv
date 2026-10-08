//! Costs of OBJEKT's resident, transfer, directory, and exchange state machines.
use super::{Estimate, Pending};
use rekursiv_asm::{Command, Pager, Status, StoreOp};
use rekursiv_model::{Model, Outcome};

pub(crate) enum ObjectPath {
    Resident,
    Transfer,
    Directory,
    Exchange,
}
impl ObjectPath {
    pub(crate) fn capture(model: &Model, c: Command) -> Self {
        match c.pager {
            Pager::Allocate => Self::Transfer,
            Pager::Fetch if c.data.is_reference() && model.resolve(c.data).is_err() => {
                Self::Transfer
            }
            Pager::NextObject | Pager::FindObject => Self::Directory,
            Pager::Exchange => Self::Exchange,
            _ => Self::Resident,
        }
    }
}
impl Estimate {
    pub(crate) fn object(&mut self, path: ObjectPath, outcome: &Outcome, asynchronous: bool) {
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
        let status = outcome.response.status;
        let approximate = !matches!(status, Status::Ok | Status::OutOfSpace);
        match path {
            ObjectPath::Resident => (),
            ObjectPath::Transfer => {
                // RESERVE and DONE/publication, plus FILL before each incoming
                // word (once for an empty object). Victim reads are distinct.
                service += 2;
                if status == Status::Ok {
                    service += outcome.memory.iter().filter(|m| m.write).count().max(1) as u64;
                }
            }
            ObjectPath::Directory => service += 1, // DONE/publication
            ObjectPath::Exchange => {
                // SELECT_META for each key, then DONE/publication. Equal keys
                // exit after the first selection, before opening a save batch.
                service += 2 + u64::from(outcome.store.iter().any(|r| r.op == StoreOp::BeginBatch));
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
