//! Stack/fetch eligibility and pending destinations. The interpreter keeps its
//! smaller scalar fast path; only expanded JIT words need this larger record.
use crate::scalar::{ScalarInstruction, ScalarWrites};
use rekursiv_asm::processor::*;
use rekursiv_model::processor::{Image, Processor};

pub(super) fn expanded(i: Instruction) -> bool {
    i.sp != Pointer::Hold
        || i.esp != Address::Hold
        || i.csp != Pointer::Hold
        || i.estk != Estk::Hold
        || i.cstk != Cstk::Hold
        || i.load_ap
        || i.apc != Apc::Hold
        || i.fetch != Fetch::Hold
}

pub(super) fn decode(i: Instruction) -> Option<ScalarInstruction> {
    i.validate().ok()?;
    let mut decoded = ScalarInstruction::decode(Instruction {
        sp: Pointer::Hold,
        esp: Address::Hold,
        csp: Pointer::Hold,
        estk: Estk::Hold,
        cstk: Cstk::Hold,
        load_ap: false,
        apc: Apc::Hold,
        fetch: Fetch::Hold,
        ..i
    })?;
    decoded.instruction = i;
    decoded.arithmetic |= matches!(i.estk, Estk::Alu | Estk::Wide | Estk::Compact);
    decoded.bus |= i.sp == Pointer::Bus
        || i.csp == Pointer::Bus
        || i.esp == Address::Bus
        || i.load_ap
        || i.cstk == Cstk::Bus
        || matches!(i.estk, Estk::Bus | Estk::Wide)
        || matches!(i.apc, Apc::Bus | Apc::Step)
        || (decoded.arithmetic && (i.r == Source::Bus || i.s == Source::Bus));
    Some(decoded)
}

#[repr(C)]
pub(crate) struct StackFetchWrites {
    pub scalar: ScalarWrites,
    pub sp: u32,
    pub csp: u32,
    pub esp: u32,
    pub ap: u32,
    pub apc: u32,
    pub estkr: u64,
    pub cstkr: u32,
    pub opcode: usize,
    pub namarg: u32,
    pub ucar: u16,
    pub estk_index: u32,
    pub cstk_index: u32,
}
impl StackFetchWrites {
    pub(crate) fn commit(self, cpu: &mut Processor, image: &Image, i: Instruction) {
        self.scalar.commit(cpu, image, i);
        cpu.sp = self.sp;
        cpu.csp = self.csp;
        cpu.esp = self.esp;
        cpu.ap = self.ap;
        cpu.apc = self.apc;
        cpu.estkr = self.estkr;
        cpu.cstkr = self.cstkr;
        cpu.opcode = self.opcode;
        cpu.namarg = self.namarg;
        cpu.ucar = self.ucar;
        if !matches!(i.estk, Estk::Hold | Estk::Read) {
            cpu.estk[self.estk_index as usize] = self.estkr;
        }
        if !matches!(i.cstk, Cstk::Hold | Cstk::Read) {
            cpu.cstk[self.cstk_index as usize] = self.cstkr;
        }
    }
}

// These ABI adapters read Rust's Option-backed tables without assuming an
// enum or Vec memory layout in generated code. They do no instruction decode,
// dispatch, or mutation. The JIT extracts fields and updates CPU state itself.
// SAFETY: generated callers supply a live, aligned eight-byte output slot.
pub(super) unsafe extern "C" fn read_nam(image: &Image, index: u32, word: *mut u64) -> u8 {
    match image.nam.get(index as usize).copied().flatten() {
        Some(value) => {
            unsafe { word.write(value) };
            0
        }
        None => 5,
    }
}
pub(super) extern "C" fn read_map(image: &Image, opcode: usize) -> u32 {
    image
        .map
        .get(opcode)
        .copied()
        .flatten()
        .map_or(1 << 16, u32::from)
}
