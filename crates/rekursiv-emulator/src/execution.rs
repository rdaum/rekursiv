//! Separate interpreter and JIT preparation types keep the interpreter's hot
//! loop free of expanded native records. Both use the same execution body and
//! transaction boundaries in Machine::step_with. Native preparation writes into
//! a caller-owned record to avoid copying nested enums before retirement.
use super::{scalar::ScalarWrites, Machine};
use rekursiv_asm::{processor::Instruction, Command};
use rekursiv_model::processor::{Image, Processor};

pub(super) trait WriteSet {
    fn object(&mut self, value: u64);
    fn device(&mut self, value: u32);
    fn commit(self, cpu: &mut Processor, image: &Image, i: Instruction);
}
pub(super) enum Writes {
    Scalar(ScalarWrites),
    General(rekursiv_model::processor::PendingWrites),
}
impl WriteSet for Writes {
    fn object(&mut self, value: u64) {
        match self {
            Self::Scalar(w) => w.object = value,
            Self::General(w) => w.object = value,
        }
    }
    fn device(&mut self, value: u32) {
        let Self::General(w) = self else {
            unreachable!("device words use general preparation")
        };
        w.device = value;
    }
    #[inline(always)]
    fn commit(self, cpu: &mut Processor, image: &Image, i: rekursiv_asm::processor::Instruction) {
        match self {
            Self::Scalar(w) => w.commit(cpu, image, i),
            Self::General(w) => w.commit(cpu, image),
        }
    }
}

pub(super) trait Execution {
    type Writes: WriteSet + Default;
    fn prepare(m: &Machine, irq: bool, out: &mut Self::Writes) -> Result<Option<Command>, u8>;
}
pub(super) struct Interpreted;
impl Execution for Interpreted {
    type Writes = Writes;
    #[inline(never)]
    fn prepare(m: &Machine, irq: bool, out: &mut Writes) -> Result<Option<Command>, u8> {
        let (writes, command) = if m.recovering() {
            if let Some(decoded) = m
                .recovery_code
                .get(m.cpu.pc as usize)
                .and_then(Option::as_ref)
            {
                decoded
                    .prepare(&m.cpu, irq)
                    .map(|(w, c)| (Writes::Scalar(w), c))
            } else {
                m.cpu
                    .prepare_recovery_writes(&m.image, irq)
                    .map(|(w, c)| (Writes::General(w), c))
            }
        } else if let Some(decoded) = m
            .scalar_code
            .get(m.cpu.pc as usize)
            .and_then(Option::as_ref)
        {
            decoded
                .prepare(&m.cpu, irq)
                .map(|(w, c)| (Writes::Scalar(w), c))
        } else {
            m.cpu
                .prepare_writes(&m.image, irq)
                .map(|(w, c)| (Writes::General(w), c))
        }?;
        *out = writes;
        Ok(command)
    }
}
pub(super) enum NativeWrites {
    Scalar(ScalarWrites),
    Interpreted(Writes),
    StackFetch(super::jit::StackFetchWrites),
}
impl WriteSet for NativeWrites {
    fn object(&mut self, value: u64) {
        match self {
            Self::Scalar(w) => w.object = value,
            Self::Interpreted(w) => w.object(value),
            Self::StackFetch(w) => w.scalar.object = value,
        }
    }
    fn device(&mut self, value: u32) {
        let Self::Interpreted(w) = self else {
            unreachable!("device words use the interpreter")
        };
        w.device(value);
    }
    #[inline(always)]
    fn commit(self, cpu: &mut Processor, image: &Image, i: Instruction) {
        match self {
            Self::Scalar(w) => w.commit(cpu, image, i),
            Self::Interpreted(w) => w.commit(cpu, image, i),
            Self::StackFetch(w) => w.commit(cpu, image, i),
        }
    }
}
pub(super) struct Native;
impl Execution for Native {
    type Writes = NativeWrites;
    #[inline(never)]
    fn prepare(m: &Machine, irq: bool, out: &mut NativeWrites) -> Result<Option<Command>, u8> {
        if !m.recovering() {
            if let Some(result) = m
                .jit
                .as_ref()
                .and_then(|jit| jit.prepare_into(&m.cpu, irq, &m.image, out))
            {
                return result;
            }
        }
        let mut writes = Writes::default();
        let command = Interpreted::prepare(m, irq, &mut writes)?;
        *out = NativeWrites::Interpreted(writes);
        Ok(command)
    }
}

impl Default for Writes {
    fn default() -> Self {
        Self::Scalar(ScalarWrites::default())
    }
}
impl Default for NativeWrites {
    fn default() -> Self {
        Self::Scalar(ScalarWrites::default())
    }
}
