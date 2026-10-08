//! Cranelift translation of the local microinstruction datapaths.
//!
//! Single-word translations prepare a small write set for the executor's
//! object-command and retirement path. Native blocks commit local instructions
//! and resident object commands, including stack and fetch controls, with a
//! peripheral tick at each boundary. Transfers, devices, and floating-point
//! words use the general executor. Collector local words use validated scalar
//! paths; its maintenance operations still execute microinstruction by microinstruction.
//! This translates microcode, not Smalltalk bytecodes.
//!
//! All machine-code pointers are private to this owner. Offsets come from this
//! build's Rust layouts, so no stable ABI is assumed for Processor. The function
//! ABIs use C calling conventions; private function types below specify them.
use crate::scalar::ScalarWrites;
use cranelift_jit::JITModule;
use eyre::Result;
use rekursiv_asm::{processor::*, Command, Word};
use rekursiv_devices::Device as ExternalDevice;
use rekursiv_model::processor::{Image, Processor};
use std::cell::Cell;

mod compile;
mod emit;
mod emit_stack;
mod resident;
mod stack_fetch;
pub(crate) use stack_fetch::StackFetchWrites;

// Packed result: fault in bits 0..7, retired prefix in 8..23, OBJEKT
// status in 48..55, and attempted object commands in 56..63. Codes 254
// and 255 represent transfer fallback and a peripheral error respectively.
// Blocks contain at most 32 instructions, so both counts fit their fields.
type RunBlock = unsafe extern "C" fn(
    *mut Processor,
    *mut ExternalDevice,
    *mut Option<eyre::Report>,
    *const Image,
    *mut rekursiv_model::Model,
) -> u64;
// Only dispatch data is hot. Keeping source snapshots here inflated each
// lookup to 40 bytes; the compact record fits four entries in a cache line.
#[derive(Default)]
struct Block {
    run: Option<RunBlock>,
    length: u8,
    object: bool,
    barrier: bool,
    recovery_safe: bool,
}
/// Conditions that restrict which native blocks can execute.
pub(crate) struct BlockPolicy {
    pub allow_object: bool,
    pub pending_object: bool,
    pub recovering: bool,
}

pub(crate) struct BlockResult {
    pub retired: u64,
    pub fault: u8,
    pub error: Option<eyre::Report>,
    pub object_commands: u64,
    pub transfer: bool,
    pub object_status: Option<rekursiv_asm::Status>,
}

// No generated frame unwinds through Rust. Peripheral errors cross the ABI in
// an owned error slot, then the safe caller returns them normally.
extern "C" fn tick(device: &mut ExternalDevice, error: &mut Option<eyre::Report>) -> u8 {
    match device.tick_idle() {
        Ok(()) => 0,
        Err(e) => {
            *error = Some(e);
            1
        }
    }
}

type Prepare = unsafe extern "C" fn(*const Processor, *mut ScalarWrites, u8, *const Image) -> u8;
type PrepareStackFetch =
    unsafe extern "C" fn(*const Processor, *mut StackFetchWrites, u8, *const Image) -> u8;
enum Preparation {
    Scalar(Prepare),
    StackFetch(PrepareStackFetch),
}
use super::execution::NativeWrites;
struct Entry {
    source: Instruction,
    prepare: Preparation,
}

/// Observations of native execution, separate from architectural counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JitStatistics {
    pub functions: usize,
    pub preparations: u64,
    pub block_calls: u64,
    pub block_instructions: u64,
    /// Native blocks that stopped before a Fetch miss for the general executor.
    pub transfer_fallbacks: u64,
}

pub(crate) struct Jit {
    // Option permits consuming the module in Drop to release executable pages.
    module: Option<JITModule>,
    entries: Vec<Option<Entry>>,
    blocks: Vec<Block>,
    block_sources: Vec<Vec<Option<Instruction>>>,
    stats: Cell<JitStatistics>,
    // Stable addresses embedded in native code; never mutate these templates.
    commands: Box<[Command]>,
}
impl Drop for Jit {
    fn drop(&mut self) {
        if let Some(module) = self.module.take() {
            // SAFETY: calls are synchronous and pointers never escape Jit.
            // Dropping this owner excludes any borrow that can execute code.
            unsafe { module.free_memory() };
        }
    }
}
impl Jit {
    // Called once after a mutable image borrow, before any execution resumes.
    // Blocks can start before an edited word: inspect their entire source range.
    // Missing/shortened stores also invalidate translations. Unchanged tables
    // retain compiled code and its statistics; invalid entries use the interpreter.
    pub(crate) fn invalidate_changed(&mut self, image: &Image) {
        for (pc, entry) in self.entries.iter_mut().enumerate() {
            if entry
                .as_ref()
                .is_some_and(|e| image.code.get(pc) != Some(&Some(e.source)))
            {
                *entry = None;
            }
        }
        for (pc, (block, source)) in self.blocks.iter_mut().zip(&self.block_sources).enumerate() {
            if block.run.is_some()
                && image.code.get(pc..pc + source.len()) != Some(source.as_slice())
            {
                block.run = None;
            }
        }
    }

    pub(crate) fn statistics(&self) -> JitStatistics {
        self.stats.get()
    }

    pub(crate) fn run_block(
        &self,
        cpu: &mut Processor,
        devices: &mut ExternalDevice,
        image: &Image,
        budget: u64,
        objekt: &mut rekursiv_model::Model,
        policy: BlockPolicy,
    ) -> Option<BlockResult> {
        let pc = cpu.pc as usize;
        let block = self.blocks.get(pc)?;
        let run = block.run?;
        if u64::from(block.length) > budget
            || cpu.roots.is_none()
            || (block.object && !policy.allow_object)
            || (block.barrier && policy.pending_object)
            || (policy.recovering && !block.recovery_safe)
        {
            return None;
        }
        let mut error = None;
        // SAFETY: Machine invalidates changed code before execution. Fields are accessed
        // with build-local offsets, and the five pointed-to objects are live
        // and disjoint throughout this synchronous call.
        let result = unsafe { run(cpu, devices, &mut error, image, objekt) };
        let mut stats = self.stats.get();
        stats.block_calls += 1;
        let retired = (result >> 8) & 0xffff;
        stats.block_instructions += retired;
        let transfer = result as u8 == resident::TRANSFER;
        stats.transfer_fallbacks += u64::from(transfer);
        self.stats.set(stats);
        Some(BlockResult {
            retired,
            transfer,
            object_commands: result >> 56,
            object_status: (result as u8 == 4)
                .then(|| rekursiv_asm::Status::try_from(((result >> 48) & 255) as u8).unwrap()),
            fault: if transfer { 0 } else { result as u8 },
            error,
        })
    }

    // Write into the caller's retirement record. Returning nested, differently
    // laid-out enums caused repeated overlapping copies and store-forwarding
    // stalls on the instruction hot path. No local effects reach CPU yet.
    #[inline]
    pub(crate) fn prepare_into(
        &self,
        cpu: &Processor,
        irq: bool,
        image: &Image,
        out: &mut NativeWrites,
    ) -> Option<Result<Option<Command>, u8>> {
        let entry = self.entries.get(cpu.pc as usize)?.as_ref()?;
        let mut stats = self.stats.get();
        stats.preparations += 1;
        self.stats.set(stats);
        let d = match entry.prepare {
            Preparation::Scalar(prepare) => {
                *out = NativeWrites::Scalar(ScalarWrites::default());
                let NativeWrites::Scalar(writes) = out else {
                    unreachable!()
                };
                // SAFETY: build-local C ABI with live, disjoint inputs/output.
                let fault = unsafe { prepare(cpu, writes, u8::from(irq), image) };
                if fault != 0 {
                    return Some(Err(fault));
                }
                writes.d
            }
            Preparation::StackFetch(prepare) => {
                *out = NativeWrites::StackFetch(StackFetchWrites::default());
                let NativeWrites::StackFetch(writes) = out else {
                    unreachable!()
                };
                // SAFETY: the expanded output matches this translation's ABI.
                let fault = unsafe { prepare(cpu, writes, u8::from(irq), image) };
                if fault != 0 {
                    return Some(Err(fault));
                }
                writes.scalar.d
            }
        };
        let i = &entry.source;
        let command = i.object.map(|mut c| {
            if i.allocation_dynamic {
                c.alloc_size = cpu.rf[i.ra as usize];
            }
            c.data = Word::from_bits(d).unwrap();
            c
        });
        Some(Ok(command))
    }

    #[cfg(test)]
    pub(crate) fn prepare(
        &self,
        cpu: &Processor,
        irq: bool,
        image: &Image,
    ) -> Option<Result<(NativeWrites, Option<Command>), u8>> {
        let mut writes = NativeWrites::default();
        Some(
            self.prepare_into(cpu, irq, image, &mut writes)?
                .map(|command| (writes, command)),
        )
    }
}

#[cfg(test)]
mod tests;
