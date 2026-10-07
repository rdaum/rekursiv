//! Separate interpreter and JIT preparation types keep the interpreter's hot
//! loop free of expanded native records. Both use the same execution body and
//! transaction boundaries in Machine::step_with. Preparation stays out of line
//! so its temporary write records do not inflate the retirement loop stack frame.
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
    type Writes: WriteSet;
    fn prepare(
        m: &Machine,
        i: Instruction,
        irq: bool,
    ) -> Result<(Self::Writes, Option<Command>), u8>;
}
pub(super) struct Interpreted;
impl Execution for Interpreted {
    type Writes = Writes;
    #[inline(never)]
    fn prepare(m: &Machine, i: Instruction, irq: bool) -> Result<(Writes, Option<Command>), u8> {
        if m.recovering() {
            m.cpu
                .prepare_recovery_writes(&m.image, irq)
                .map(|(w, c)| (Writes::General(w), c))
        } else if let Some(decoded) = m
            .scalar_code
            .get(m.cpu.pc as usize)
            .and_then(Option::as_ref)
            .filter(|d| d.matches(&i))
        {
            decoded
                .prepare(&m.cpu, irq)
                .map(|(w, c)| (Writes::Scalar(w), c))
        } else {
            m.cpu
                .prepare_writes(&m.image, irq)
                .map(|(w, c)| (Writes::General(w), c))
        }
    }
}
pub(super) enum NativeWrites {
    Interpreted(Writes),
    StackFetch(super::jit::StackFetchWrites),
}
impl WriteSet for NativeWrites {
    fn object(&mut self, value: u64) {
        match self {
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
            Self::Interpreted(w) => w.commit(cpu, image, i),
            Self::StackFetch(w) => w.commit(cpu, image, i),
        }
    }
}
pub(super) struct Native;
impl Execution for Native {
    type Writes = NativeWrites;
    #[inline(never)]
    fn prepare(
        m: &Machine,
        i: Instruction,
        irq: bool,
    ) -> Result<(NativeWrites, Option<Command>), u8> {
        if !m.recovering() {
            if let Some(result) = m
                .jit
                .as_ref()
                .and_then(|jit| jit.prepare(&m.cpu, i, irq, &m.image))
            {
                return result.map(|(w, c)| (w.into(), c));
            }
        }
        Interpreted::prepare(m, i, irq).map(|(w, c)| (NativeWrites::Interpreted(w), c))
    }
}
