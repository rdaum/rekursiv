//! Cycle-independent architectural oracle. No RTL or Verilator dependency.
use rekursiv_asm::*;
mod directory;
mod exchange;
pub mod ram_gc;
mod recovery;
pub mod store;
mod transfer;
use store::BackingStore;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub vr: [Word; 8],
    pub index: i64,
    pub index_reg: i64,
    pub selected: Option<Entry>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            vr: [Word::NIL; 8],
            index: 0,
            index_reg: 0,
            selected: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryEffect {
    pub address: u32,
    pub write: bool,
    pub value: Word,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub response: Response,
    pub memory: Vec<MemoryEffect>,
    pub store: Vec<StoreRequest>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub state: State,
    pub entries: Vec<Option<Entry>>,
    pub classes: [Option<Word>; 64],
    pub memory: Vec<Word>,
    pub store: BackingStore,
    pub next_identity: u64,
    pub body_cursor: u32,
    pub allocation_limit: u32,
    pub persistent_roots: Vec<bool>,
    pub maintenance: bool,
}
impl Outcome {
    fn error(status: Status) -> Self {
        Self {
            response: Response::error(status),
            memory: Vec::new(),
            store: Vec::new(),
        }
    }
}
impl Model {
    pub fn new(pager_entries: usize, memory_words: usize) -> Self {
        assert!(pager_entries.is_power_of_two() && pager_entries >= 2);
        assert!(memory_words > 0 && memory_words <= ADDRESS_LIMIT as usize);
        Self {
            state: State::default(),
            entries: vec![None; pager_entries],
            classes: [None; 64],
            memory: vec![Word::ZERO; memory_words],
            store: BackingStore::default(),
            next_identity: 1,
            body_cursor: 0,
            allocation_limit: memory_words as u32,
            persistent_roots: vec![false; pager_entries],
            maintenance: false,
        }
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.entries.len(), self.memory.len());
    }
    fn slot(&self, r: Word) -> usize {
        r.bits() as usize & (self.entries.len() - 1)
    }
    pub fn resolve(&self, r: Word) -> Result<Entry, Status> {
        if r.is_compact() {
            let (code, payload) = r.compact_parts()?;
            let class = self.classes[code as usize].ok_or(Status::BadValue)?;
            Ok(Entry {
                reference: r,
                class,
                size: 0,
                base: 0,
                representation: Word::raw(payload as u64)?,
                new: false,
                modified: false,
                cond: false,
            })
        } else {
            r.identity()?;
            self.entries[self.slot(r)]
                .filter(|e| e.reference == r)
                .ok_or(Status::NotResident)
        }
    }
    fn selected(&self) -> Result<Entry, Status> {
        self.resolve(self.state.selected.ok_or(Status::NoSelection)?.reference)
    }
    /// Native command boundary: validate once, without constructing wire ports.
    /// Keep validation before the maintenance check, as in encode-then-execute.
    pub fn execute(&mut self, command: Command, memory_error: bool) -> Outcome {
        let command = match command.validate() {
            Ok(command) => command,
            Err(status) => return Outcome::error(status),
        };
        if self.maintenance {
            return Outcome::error(Status::BadCommand);
        }
        self.execute_validated(
            command,
            Faults {
                memory_at: memory_error.then_some(0),
                store_at: None,
            },
        )
    }
    pub fn execute_raw(&mut self, ports: Ports, memory_error: bool) -> Outcome {
        self.execute_faults(
            ports,
            Faults {
                memory_at: memory_error.then_some(0),
                store_at: None,
            },
        )
    }
    /// External wire boundary. Decoding checks numeric fields and validates the
    /// resulting command. Maintenance rejects requests before inspecting ports.
    pub fn execute_faults(&mut self, ports: Ports, faults: Faults) -> Outcome {
        if self.maintenance {
            return Outcome::error(Status::BadCommand);
        }
        match ports.decode() {
            Ok(command) => self.execute_validated(command, faults),
            Err(status) => Outcome::error(status),
        }
    }
    // Both entry points have checked the command and maintenance lock. Keep
    // dispatch and all memory/store effects shared; only wire conversion differs.
    fn execute_validated(&mut self, command: Command, faults: Faults) -> Outcome {
        match command.pager {
            Pager::NextObject | Pager::FindObject => {
                return self.execute_directory(command, faults);
            }
            Pager::Exchange => return self.execute_exchange(command, faults),
            Pager::Fetch | Pager::Allocate => {
                return self.transfer_validated(command, faults);
            }
            _ => {}
        }
        let memory_error = faults.memory_at == Some(0);
        let mut effect = None;
        // transition validates all fallible conditions before publishing a
        // memory write or replacing state. Cloning the complete disk image on
        // every register read adds no rollback protection and makes original
        // image execution prohibitively expensive.
        let result = self.transition(command, memory_error, &mut effect);
        match result {
            Ok(data) => Outcome {
                response: Response::ok(data),
                memory: effect.into_iter().collect(),
                store: Vec::new(),
            },
            Err(e) => Outcome {
                response: Response::error(e),
                memory: effect.into_iter().collect(),
                store: Vec::new(),
            },
        }
    }
    fn transition(
        &mut self,
        c: Command,
        memory_error: bool,
        effect: &mut Option<MemoryEffect>,
    ) -> Result<Word, Status> {
        if matches!(c.pager, Pager::Fetch | Pager::Allocate | Pager::Exchange) {
            return Err(Status::BadCommand);
        }
        let before = self.state.clone();
        let mut next = before.clone();
        // Calculate all proposed state from the old state before any memory effect.
        let idx = match c.index {
            Index::None => before.index,
            Index::Load => c.data.as_index(),
            Index::Clear => 0,
            Index::One => 1,
            Index::Two => 2,
            Index::Increment => before.index + 1,
            Index::Decrement => before.index - 1,
            Index::Step => before.index + c.data.as_index(),
            Index::FromReg => before.index_reg,
            Index::Next => {
                let e = self.selected()?;
                if e.size == 0 || before.index < 0 || before.index > e.size as i64 {
                    return Err(Status::BoundsError);
                }
                next.selected = Some(e);
                if before.index == e.size as i64 {
                    1
                } else {
                    before.index + 1
                }
            }
        };
        if !(INDEX_MIN..=INDEX_MAX).contains(&idx) {
            return Err(Status::IndexOverflow);
        }
        next.index = idx;
        let reg = match c.register {
            Register::None => before.index_reg,
            Register::Load => c.data.as_index(),
            Register::Increment => before.index_reg + 1,
            Register::Decrement => before.index_reg - 1,
            Register::FromIndex => before.index,
        };
        if !(INDEX_MIN..=INDEX_MAX).contains(&reg) {
            return Err(Status::IndexOverflow);
        }
        next.index_reg = reg;
        if c.load_vr {
            next.vr[c.vr as usize] = c.data;
        }
        let mut output = Word::NIL;
        if c.pager != Pager::None {
            let r = match c.pager {
                Pager::ProbeBus => c.data,
                Pager::ProbeVr => before.vr[c.vr as usize],
                Pager::ProbeType => self.selected()?.class,
                Pager::ProbeRepresentation => self.selected()?.representation,
                _ => unreachable!(),
            };
            let e = self.resolve(r)?;
            output = e.reference;
            next.selected = Some(e);
        }
        let needs_metadata = matches!(
            c.read,
            Read::Size | Read::Type | Read::Base | Read::Representation | Read::Flags
        ) || c.expected_type.is_some()
            || c.memory != Memory::None;
        let old_entry = if needs_metadata {
            Some(self.selected()?)
        } else {
            None
        };
        if let Some(e) = old_entry {
            // A simultaneous page retains its newly selected snapshot; reads use old selection.
            if c.pager == Pager::None {
                next.selected = Some(e);
            }
            if c.expected_type.is_some_and(|t| t != e.class) {
                return Err(Status::TypeError);
            }
        }
        output = match c.read {
            Read::None => output,
            Read::Vr => before.vr[c.vr as usize],
            Read::Reference => before.selected.ok_or(Status::NoSelection)?.reference,
            Read::Index => Word::index(before.index)?,
            Read::IndexReg => Word::index(before.index_reg)?,
            Read::Size => Word::raw(old_entry.unwrap().size as u64)?,
            Read::Type => old_entry.unwrap().class,
            Read::Base => Word::raw(old_entry.unwrap().base as u64)?,
            Read::Representation => old_entry.unwrap().representation,
            Read::Flags => Word::raw(old_entry.unwrap().flags() as u64)?,
            Read::FreeWords => {
                Word::raw(self.allocation_limit.saturating_sub(self.body_cursor) as u64)?
            }
            Read::FreeIdentities => Word::raw((ID_MASK + 1).saturating_sub(self.next_identity))?,
        };
        if c.memory != Memory::None {
            let mut e = old_entry.unwrap();
            if before.index <= 0 || before.index > e.size as i64 {
                return Err(Status::BoundsError);
            }
            let address = e.base as u64 + before.index as u64 - 1;
            if address >= ADDRESS_LIMIT as u64 || address >= self.memory.len() as u64 {
                return Err(Status::BoundsError);
            }
            if c.memory == Memory::Read && before.index == 1 {
                output = e.representation;
            } else {
                *effect = Some(MemoryEffect {
                    address: address as u32,
                    write: c.memory == Memory::Write,
                    value: c.data,
                });
                if memory_error {
                    return Err(Status::MemoryError);
                }
                if c.memory == Memory::Read {
                    output = self.memory[address as usize];
                } else {
                    self.memory[address as usize] = c.data;
                    e.modified = true;
                    if before.index == 1 {
                        e.representation = c.data;
                    }
                    let slot = self.slot(e.reference);
                    self.entries[slot] = Some(e);
                    next.selected = Some(e);
                    output = c.data;
                }
            }
        }
        self.state = next;
        Ok(output)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> Model {
        let mut m = Model::new(4, 32);
        let e = Entry {
            reference: Word::reference(1, true).unwrap(),
            class: Word::reference(2, true).unwrap(),
            base: 4,
            size: 2,
            representation: Word::signed(17),
            new: false,
            modified: false,
            cond: false,
        };
        m.memory[4] = e.representation;
        m.memory[5] = Word::signed(18);
        assert_eq!(
            m.service(Service::Install(e), false).response.status,
            Status::Ok
        );
        m.execute(Command::probe(e.reference), false);
        m
    }
    #[test]
    fn native_and_wire_commands_match_effects_and_complete_state() {
        let reference = |id| Word::reference(id, true).unwrap();
        // Two pager slots force dirty eviction and refill, in addition to the
        // resident, allocation, exchange, and directory command paths.
        let mut model = Model::new(2, 64);
        let mut commands = vec![
            Command::allocate(reference(100), 3, true).unwrap(),
            Command::index(1).unwrap(),
            Command::write_field(Word::ZERO),
            Command::index(2).unwrap(),
            Command::write_field(Word::signed(17)),
            Command {
                load_vr: true,
                vr: 7,
                data: reference(1),
                ..Command::default()
            },
            Command::allocate(reference(100), 3, true).unwrap(),
            Command::index(1).unwrap(),
            Command::write_field(Word::ZERO),
            Command::index(2).unwrap(),
            Command::write_field(Word::signed(18)),
            Command::allocate(reference(100), 3, true).unwrap(),
            Command::fetch(reference(1)),
            Command {
                pager: Pager::Exchange,
                data: reference(2),
                vr: 7,
                ..Command::default()
            },
            Command {
                pager: Pager::NextObject,
                ..Command::default()
            },
            Command {
                pager: Pager::FindObject,
                data: reference(1),
                ..Command::default()
            },
            Command::fetch(reference(1)),
            Command::index(2).unwrap(),
            Command::read_field(),
        ];
        for read in [
            Read::Vr,
            Read::Reference,
            Read::Size,
            Read::Type,
            Read::Base,
            Read::Representation,
            Read::Index,
            Read::IndexReg,
            Read::Flags,
            Read::FreeWords,
            Read::FreeIdentities,
        ] {
            commands.push(Command::read(read));
        }
        for command in commands {
            for memory_error in [true, false] {
                let mut native = model.clone();
                let mut wire = model.clone();
                let actual = native.execute(command, memory_error);
                let expected = wire.execute_raw(command.encode().unwrap(), memory_error);
                assert_eq!(actual, expected, "{command:?}, fault={memory_error}");
                assert_eq!(native, wire, "{command:?}, fault={memory_error}");
                if !memory_error {
                    assert_eq!(actual.response.status, Status::Ok, "{command:?}");
                    model = native;
                }
            }
        }
        assert!(!model.store.records.is_empty());
    }

    #[test]
    fn command_boundaries_preserve_validation_and_maintenance_error_priority() {
        for command in [
            Command {
                vr: 8,
                ..Command::default()
            },
            Command {
                expected_type: Some(Word::ZERO),
                ..Command::default()
            },
            Command {
                pager: Pager::Allocate,
                ..Command::default()
            },
            Command {
                alloc_size: ADDRESS_LIMIT,
                ..Command::default()
            },
        ] {
            let expected = command.validate().unwrap_err();
            for maintenance in [false, true] {
                let mut model = setup();
                model.maintenance = maintenance;
                let before = model.clone();
                assert_eq!(model.execute(command, true), Outcome::error(expected));
                assert_eq!(model, before);
                // The public transfer entry point must still validate itself.
                assert_eq!(
                    model.execute_transfer(command, Faults::default()),
                    Outcome::error(expected)
                );
                assert_eq!(model, before);
            }
        }
        let mut model = setup();
        model.maintenance = true;
        let before = model.clone();
        let command = Command::read_field();
        assert_eq!(
            model.execute(command, false),
            Outcome::error(Status::BadCommand)
        );
        assert_eq!(
            model.execute_raw(command.encode().unwrap(), false),
            Outcome::error(Status::BadCommand)
        );
        // A malformed data word normally reports BadValue; maintenance wins at
        // the wire boundary, before any decode or side effect.
        let ports = Ports {
            data: 1 << 40,
            ..Ports::default()
        };
        assert_eq!(
            model.execute_raw(ports, false),
            Outcome::error(Status::BadCommand)
        );
        assert_eq!(model, before);
        model.maintenance = false;
        assert_eq!(
            model.execute_raw(ports, false),
            Outcome::error(Status::BadValue)
        );
    }

    #[test]
    fn old_state_and_atomic_failure() {
        let mut m = setup();
        m.execute(Command::index(1).unwrap(), false);
        let c = Command {
            index: Index::Increment,
            memory: Memory::Read,
            ..Command::default()
        };
        assert_eq!(m.execute(c, false).response.data, Word::signed(17));
        assert_eq!(m.state.index, 2);
        let before = m.state.clone();
        let c = Command {
            index: Index::Increment,
            load_vr: true,
            data: Word::signed(99),
            memory: Memory::Write,
            ..Command::default()
        };
        assert_eq!(m.execute(c, true).response.status, Status::MemoryError);
        assert_eq!(m.state, before);
        assert_eq!(m.memory[5], Word::signed(18));
    }
    #[test]
    fn revalidation_and_compact_types() {
        let mut m = setup();
        let r = m.state.selected.unwrap().reference;
        m.service(Service::Invalidate(r), false);
        assert_eq!(
            m.execute(Command::read(Read::Size), false).response.status,
            Status::NotResident
        );
        assert_eq!(m.state.selected.unwrap().reference, r);
        assert_eq!(
            m.execute(Command::probe(Word::signed(-8)), false)
                .response
                .status,
            Status::BadValue
        );
        m.service(
            Service::CompactClass {
                code: 2,
                class: Word::reference(2, true).unwrap(),
            },
            false,
        );
        assert_eq!(
            m.execute(Command::probe(Word::signed(-8)), false)
                .response
                .status,
            Status::Ok
        );
        assert_eq!(m.state.selected.unwrap().representation.bits(), 0xfffffff8);
    }
}

pub mod float;
pub mod processor;
