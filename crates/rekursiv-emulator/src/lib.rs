//! Native instruction-level executor for the same microcode used by LOGIK.
//!
//! OBJEKT commands use architectural semantics; external device requests use
//! the shared peripheral model. No guest bytecode, primitive, or class dispatch
//! occurs here. Recovery executes ordinary control-store instructions against
//! a maintenance datapath, independently of the model's collect_ram oracle.
use eyre::{ensure, Result};
use rekursiv_asm::{
    processor::{Device as Io, Recovery, Seq},
    Command, Pager, Status, Word,
};
use rekursiv_devices::{Device, Request};
use rekursiv_model::{
    processor::{Image, Processor, CODE_WORDS, STACK_WORDS},
    Model,
};
pub mod boot;
pub mod presentation;
mod recovery;
use recovery::RecoveryState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Retired,
    RecoveryEntered,
    RecoveryReturned,
    Held,
    Halted,
    Service(u8),
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Statistics {
    pub retired: u64,
    pub collector_retired: u64,
    pub collections: u64,
    pub object_commands: u64,
    pub device_requests: u64,
}
impl Statistics {
    /// Retired mutator plus collector instructions per elapsed second. Hold
    /// steps and entry/return transitions do not retire a microinstruction.
    pub fn instructions_per_second(&self, elapsed: std::time::Duration) -> f64 {
        if elapsed.is_zero() {
            0.0
        } else {
            (self.retired as f64 + self.collector_retired as f64) / elapsed.as_secs_f64()
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fault {
    pub pc: u16,
    pub code: u8,
    pub status: Option<Status>,
}
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "processor fault {} at micro-PC {} (object status {:?})",
            self.code, self.pc, self.status
        )
    }
}
impl std::error::Error for Fault {}

pub struct Machine {
    pub image: Image,
    pub cpu: Processor,
    pub objekt: Model,
    pub devices: Device,
    pub stats: Statistics,
    pub fault: Option<Fault>,
    recovery: Option<RecoveryState>,
    retry_irq: Option<bool>,
}
impl Machine {
    pub fn new(
        image: Image,
        entry: u16,
        pager_entries: usize,
        memory_words: usize,
    ) -> Result<Self> {
        ensure!(
            pager_entries.is_power_of_two() && (2..=65536).contains(&pager_entries),
            "invalid pager capacity"
        );
        ensure!(
            (2..=1 << 24).contains(&memory_words) && memory_words.is_multiple_of(2),
            "RAM must contain an even number of words, between 2 and 2^24"
        );
        ensure!(
            image.code.get(entry as usize).is_some_and(Option::is_some),
            "entry is not populated"
        );
        let mut objekt = Model::new(pager_entries, memory_words);
        if image.collector_entry.is_some() {
            objekt.allocation_limit = memory_words as u32 / 2;
        }
        let cpu = Processor {
            pc: entry,
            roots: Some(image.roots),
            ..Default::default()
        };
        Ok(Self {
            image,
            cpu,
            objekt,
            devices: Device::default(),
            stats: Statistics::default(),
            fault: None,
            recovery: None,
            retry_irq: None,
        })
    }
    pub fn recovering(&self) -> bool {
        self.recovery.is_some()
    }
    pub fn resume(&mut self) -> Result<()> {
        ensure!(
            self.cpu.service && self.fault.is_none(),
            "resume requires a service break without a fault"
        );
        self.cpu.service = false;
        Ok(())
    }
    fn fail(&mut self, code: u8, status: Option<Status>) -> Fault {
        // Failed recovery restores the interrupted context. It does not publish
        // an incomplete relocation plan or retire the failed instruction.
        if let Some(gc) = self.recovery.take() {
            self.cpu = gc.saved;
        }
        let fault = Fault {
            pc: self.cpu.pc,
            code,
            status,
        };
        self.cpu.halted = true;
        self.fault = Some(fault);
        fault
    }
    pub fn step(&mut self) -> Result<Step> {
        if let Some(fault) = self.fault {
            return Err(fault.into());
        }
        if self.cpu.halted {
            return Ok(Step::Halted);
        }
        if self.cpu.service {
            return Ok(Step::Service(self.cpu.service_code));
        }
        let instruction = self
            .image
            .code
            .get(self.cpu.pc as usize)
            .copied()
            .flatten()
            .ok_or_else(|| self.fail(2, None))?;
        if self.recovery.is_some() && instruction.recovery == Recovery::Return {
            // Validate even the return word; it cannot smuggle a stack write or
            // a device operation into the frozen mutator state.
            self.cpu
                .prepare_recovery(&self.image, false)
                .map_err(|code| self.fail(code, None))?;
            if !self.recovery.as_ref().unwrap().committed {
                return Err(self.fail(1, None).into());
            }
            let gc = self.recovery.take().unwrap();
            self.cpu = gc.saved;
            self.retry_irq = Some(gc.irq);
            self.stats.collections += 1;
            self.devices.tick(None, false)?;
            return Ok(Step::RecoveryReturned);
        }
        let irq = self.retry_irq.unwrap_or_else(|| {
            self.devices
                .events
                .as_ref()
                .is_some_and(|e| e.status() != 0)
        });
        let prepared = if self.recovering() {
            self.cpu.prepare_recovery(&self.image, irq)
        } else {
            self.cpu.prepare(&self.image, irq)
        };
        let (mut next, command) = prepared.map_err(|code| self.fail(code, None))?;
        if instruction.recovery == Recovery::Collect && self.retry_irq.is_none() {
            self.enter_recovery(None, irq)?;
            self.devices.tick(None, false)?;
            return Ok(Step::RecoveryEntered);
        }
        if instruction.seq == Seq::Hold {
            self.devices.tick(None, false)?;
            return Ok(Step::Held);
        }
        if !matches!(instruction.recovery, Recovery::None | Recovery::Collect) {
            let data = Word::from_bits(self.cpu.bus(instruction))?;
            let result = self
                .recovery
                .as_mut()
                .unwrap()
                .execute(instruction.recovery, data, &mut self.objekt)
                .map_err(|status| self.fail(4, Some(status)))?;
            next.object = result.bits();
        }
        if let Some(command) = command {
            self.stats.object_commands += 1;
            let response = self.objekt.execute(command, false).response;
            if response.status == Status::OutOfSpace
                && self.image.collector_entry.is_some()
                && self.retry_irq.is_none()
                && matches!(command.pager, Pager::Allocate | Pager::Fetch)
            {
                self.enter_recovery(Some(command), irq)?;
                self.devices.tick(None, false)?;
                return Ok(Step::RecoveryEntered);
            }
            if response.status != Status::Ok {
                return Err(self.fail(4, Some(response.status)).into());
            }
            next.object = response.data.bits();
        }
        if instruction.device != Io::None {
            let request = Request {
                address: self.cpu.rf[instruction.ra as usize],
                write: instruction.device == Io::Write,
                data: if instruction.device == Io::Write {
                    self.cpu.bus(instruction) as u32
                } else {
                    0
                },
            };
            // Device delays advance external time while local retirement stays
            // frozen. IRQ is sampled above and does not change a pending branch.
            loop {
                let ready = self.devices.ready();
                self.devices.tick(Some(request), false)?;
                if ready {
                    break;
                }
            }
            loop {
                let reply = self.devices.response();
                self.devices.tick(None, true)?;
                if let Some(reply) = reply {
                    if reply.error {
                        return Err(self.fail(6, None).into());
                    }
                    next.device = reply.data;
                    break;
                }
            }
            self.stats.device_requests += 1;
        } else {
            self.devices.tick(None, false)?;
        }
        self.cpu = next;
        if self.recovering() {
            self.stats.collector_retired += 1;
        } else {
            self.stats.retired += 1;
            self.retry_irq = None;
        }
        Ok(Step::Retired)
    }
    fn enter_recovery(&mut self, command: Option<Command>, irq: bool) -> Result<()> {
        // Proactive collection has no pending transfer, required allocation,
        // or extra class root. It uses the same collector instructions.
        let (needed, needed_class) = if let Some(command) = command {
            if command.pager == Pager::Fetch {
                let r = self
                    .objekt
                    .store
                    .records
                    .get(&command.data.identity()?)
                    .ok_or(Status::InvalidReference)?;
                (r.body.len() as u32, r.class)
            } else {
                (command.alloc_size, command.data)
            }
        } else {
            (0, Word::ZERO)
        };
        let mut roots = vec![Word::ZERO; 20 + STACK_WORDS + 2 * CODE_WORDS + 32];
        roots[..8].copy_from_slice(&self.objekt.state.vr);
        if let Some(e) = self.objekt.state.selected {
            roots[8] = e.reference;
            roots[9] = e.class;
        }
        for n in 0..4 {
            roots[10 + n] = self.objekt.classes[n].unwrap_or(Word::ZERO);
        }
        roots[14] = command.map_or(Word::ZERO, |c| c.data);
        roots[15] = command.and_then(|c| c.expected_type).unwrap_or(Word::ZERO);
        roots[16] = Word::from_bits(self.cpu.estkr)?;
        roots[17] = Word::from_bits(self.cpu.symbol)?;
        roots[18] = Word::from_bits(self.cpu.object)?;
        roots[19] = needed_class;
        for n in 0..=self.cpu.sp as usize {
            roots[20 + n] = Word::from_bits(self.cpu.estk[n])?;
        }
        for (n, instruction) in self.image.code.iter().enumerate() {
            if let Some(i) = instruction {
                roots[20 + STACK_WORDS + 2 * n] = i.data;
                roots[20 + STACK_WORDS + 2 * n + 1] =
                    i.object.and_then(|c| c.expected_type).unwrap_or(Word::ZERO);
            }
        }
        roots[20 + STACK_WORDS + 2 * CODE_WORDS..]
            .copy_from_slice(&self.cpu.roots.unwrap_or(self.image.roots));
        self.recovery = Some(RecoveryState::new(
            self.cpu.clone(),
            irq,
            roots,
            needed,
            &self.objekt,
        ));
        self.cpu.pc = self.image.collector_entry.unwrap();
        Ok(())
    }
}
