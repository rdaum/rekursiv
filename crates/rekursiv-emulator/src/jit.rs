//! Cranelift translation of the local microinstruction datapaths.
//!
//! Single-word translations prepare a small write set for the executor's
//! object-command and retirement path. Native blocks commit local instructions
//! and resident object commands, including stack and fetch controls. Static
//! branches are linked within bounded regions; CPU registers are materialized
//! at exits. Quiet device edges are batched within a conservative event budget. Transfers, devices, and floating-point
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

mod compile;
mod dispatch;
mod emit;
mod emit_stack;
mod native_objekt;
mod resident;
mod stack_fetch;
pub(crate) use stack_fetch::StackFetchWrites;

// Packed result: fault in bits 0..7, retired prefix in 8..23, guest
// dispatches in 24..31, OBJEKT status in 48..55, and attempted object
// commands in 56..63. Codes 254
// and 255 represent transfer fallback and a peripheral error respectively.
// Blocks contain at most 32 instructions, so both counts fit their fields.
const fn block_result(fault: u8, retired: u64, commands: u64, guest_bytecodes: u64) -> u64 {
    fault as u64 | (retired << 8) | (guest_bytecodes << 24) | (commands << 56)
}

type RunBlock = unsafe extern "C" fn(
    *mut Processor,
    *mut ExternalDevice,
    *mut Option<eyre::Report>,
    *const Tables,
    *mut rekursiv_model::Model,
    u8,
    *const native_objekt::MemoryView,
) -> u64;
// Only dispatch data is hot. Keeping source snapshots here inflated each
// lookup to 40 bytes; the compact record fits four entries in a cache line.
#[derive(Clone, Copy, Default)]
#[repr(C)]
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
    pub guest_bytecodes: u64,
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

type Prepare = unsafe extern "C" fn(*const Processor, *mut ScalarWrites, u8, *const Tables) -> u8;
type PrepareStackFetch =
    unsafe extern "C" fn(*const Processor, *mut StackFetchWrites, u8, *const Tables) -> u8;
#[derive(Clone, Copy)]
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
    /// Host time spent compiling and finalizing native functions. With lazy
    /// compilation this is included in execution wall time, not startup time.
    pub compilation_time: std::time::Duration,
    /// A compiler error disables further compilation; execution falls back.
    pub compilation_failures: u64,
    pub preparations: u64,
    pub block_calls: u64,
    /// Rust-to-native entries, each of which may execute several regions.
    pub native_entries: u64,
    pub block_instructions: u64,
    /// Native blocks that stopped before a Fetch miss for the general executor.
    pub transfer_fallbacks: u64,
}

const HOT_REGION: u8 = 16;
const HOT_PREPARATION: u8 = 32;
const DISABLED: u8 = u8::MAX;
#[derive(Clone, Copy, Default)]
struct Heat {
    block: u8,
    preparation: u8,
}

// Flatten optional microcode tables for checked native loads. The source Image
// stays authoritative; Machine refreshes these after every image_mut borrow.
// Separate validity preserves the full 64-bit NAM value domain.
#[repr(C)]
struct NamWord {
    value: u64,
    valid: u64,
}
#[repr(C)]
struct Tables {
    nam: *const NamWord,
    nam_len: usize,
    map: *const u32,
    map_len: usize,
}

pub(crate) struct Jit {
    // Option permits consuming the module in Drop to release executable pages.
    module: Option<JITModule>,
    nam: Vec<NamWord>,
    map: Vec<u32>,
    entries: Vec<Option<Entry>>,
    blocks: Vec<Block>,
    block_sources: Vec<Vec<(usize, Instruction)>>,
    heat: Vec<Heat>,
    preparations: std::collections::HashMap<[u32; 8], Preparation>,
    guest_dispatch_pc: Option<u16>,
    lazy: bool,
    compilation_failed: bool,
    stats: JitStatistics,
    dispatch: Option<dispatch::RunChain>,
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
    fn refresh_tables(&mut self, image: &Image) {
        self.nam = image
            .nam
            .iter()
            .map(|v| NamWord {
                value: v.unwrap_or(0),
                valid: u64::from(v.is_some()),
            })
            .collect();
        self.map = image
            .map
            .iter()
            .map(|v| v.map_or(1 << 16, u32::from))
            .collect();
    }
    fn tables(&self) -> Tables {
        Tables {
            nam: self.nam.as_ptr(),
            nam_len: self.nam.len(),
            map: self.map.as_ptr(),
            map_len: self.map.len(),
        }
    }
    // Called once after a mutable image borrow, before any execution resumes.
    // Regions may follow noncontiguous branches: inspect every source dependency.
    // Missing/shortened stores also invalidate translations. Unchanged tables
    // retain compiled code and its statistics; invalid entries use the interpreter.
    pub(crate) fn invalidate_changed(&mut self, image: &Image) {
        self.refresh_tables(image);
        for (pc, entry) in self.entries.iter_mut().enumerate() {
            if entry
                .as_ref()
                .is_some_and(|e| image.code.get(pc) != Some(&Some(e.source)))
            {
                *entry = None;
            }
            if entry.is_none() {
                self.heat[pc].preparation = 0;
            }
        }
        for (pc, (block, source)) in self
            .blocks
            .iter_mut()
            .zip(&mut self.block_sources)
            .enumerate()
        {
            if source.is_empty()
                || source
                    .iter()
                    .any(|(pc, instruction)| image.code.get(*pc) != Some(&Some(*instruction)))
            {
                *block = Block::default();
                source.clear();
                self.heat[pc].block = 0;
            }
        }
        self.entries.resize_with(image.code.len(), || None);
        self.blocks.resize_with(image.code.len(), Block::default);
        self.block_sources.resize_with(image.code.len(), Vec::new);
        self.heat.resize(image.code.len(), Heat::default());
    }

    fn finish_compilation(&mut self, result: Result<()>, started: std::time::Instant) {
        self.stats.compilation_time += started.elapsed();
        if let Err(error) = result {
            self.stats.compilation_failures += 1;
            self.compilation_failed = true;
            eprintln!("JIT compilation failed; retaining executor fallback: {error:#}");
        }
    }

    #[cold]
    fn heat_region(&mut self, pc: usize, image: &Image, budget: u64, policy: &BlockPolicy) {
        if self.block_sources[pc].is_empty() {
            self.discover_region(image, pc);
            if self.blocks[pc].length == 0 {
                self.heat[pc].block = DISABLED;
                return;
            }
        }
        let block = &self.blocks[pc];
        if u64::from(block.length) > budget
            || (block.object && !policy.allow_object)
            || (block.barrier && policy.pending_object)
            || (policy.recovering && !block.recovery_safe)
        {
            return;
        }
        self.heat[pc].block += 1;
        if self.heat[pc].block == HOT_REGION {
            self.heat[pc].block = DISABLED;
            let started = std::time::Instant::now();
            let result = self.compile_region(pc);
            self.finish_compilation(result, started);
        }
    }

    #[cold]
    fn heat_preparation(&mut self, pc: usize, image: &Image) {
        self.heat[pc].preparation += 1;
        if self.heat[pc].preparation == HOT_PREPARATION {
            self.heat[pc].preparation = DISABLED;
            let started = std::time::Instant::now();
            let result = self.compile_preparation(image, pc);
            self.finish_compilation(result, started);
        }
    }

    pub(crate) fn statistics(&self) -> JitStatistics {
        self.stats
    }

    pub(crate) fn run_block(
        &mut self,
        cpu: &mut Processor,
        devices: &mut ExternalDevice,
        image: &Image,
        budget: u64,
        objekt: &mut rekursiv_model::Model,
        policy: BlockPolicy,
    ) -> Option<BlockResult> {
        let pc = cpu.pc as usize;
        if self.blocks.get(pc)?.run.is_none()
            && cpu.roots.is_some()
            && self.lazy
            && !self.compilation_failed
            && self.heat[pc].block != DISABLED
        {
            self.heat_region(pc, image, budget, &policy);
        }
        let block = self.blocks.get(pc)?;
        let run = block.run?;
        if objekt.entries.len() < 2
            || !objekt.entries.len().is_power_of_two()
            || u64::from(block.length) > budget
            || cpu.roots.is_none()
            || (block.object && !policy.allow_object)
            || (block.barrier && policy.pending_object)
            || (policy.recovering && !block.recovery_safe)
        {
            return None;
        }
        let mut error = None;
        // SAFETY: Machine invalidates changed code before execution. Fields are accessed
        // with build-local offsets, and the pointed-to objects are live
        // and disjoint throughout this synchronous call.
        let quiet = devices.idle_budget();
        let tickless = u8::from(quiet >= u64::from(block.length));
        let view = native_objekt::MemoryView::new(objekt);
        // A chain stays entirely inside a quiet device interval. Resident
        // regions cannot launch asynchronous work, refill, allocate or collect.
        // Pending replies, recovery and instrumentation retain individual calls.
        let chain_budget = budget.min(quiet).min(dispatch::MAX_INSTRUCTIONS);
        let chain = if policy.allow_object
            && !policy.pending_object
            && !policy.recovering
            && chain_budget >= u64::from(block.length)
        {
            let mut result = dispatch::ChainResult::default();
            // SAFETY: the dispatch table and borrowed vectors cannot change
            // while generated code runs. Every indirect call checks its entry
            // and remaining budget; Machine invalidates edits before entry.
            unsafe {
                self.dispatch.unwrap()(
                    cpu,
                    devices,
                    &mut error,
                    &self.tables(),
                    objekt,
                    &view,
                    self.blocks.as_ptr(),
                    self.blocks.len(),
                    chain_budget,
                    &mut result,
                );
            }
            result
        } else {
            let result = unsafe {
                run(
                    cpu,
                    devices,
                    &mut error,
                    &self.tables(),
                    objekt,
                    tickless,
                    &view,
                )
            };
            dispatch::ChainResult {
                retired: (result >> 8) & 0xffff,
                guest_bytecodes: (result >> 24) & 0xff,
                object_commands: result >> 56,
                blocks: 1,
                result,
            }
        };
        let result = chain.result;
        let stats = &mut self.stats;
        stats.block_calls += chain.blocks;
        stats.native_entries += 1;
        let retired = chain.retired;
        if tickless != 0 {
            devices.advance_idle(retired);
        }
        stats.block_instructions += retired;
        let transfer = result as u8 == resident::TRANSFER;
        stats.transfer_fallbacks += u64::from(transfer);
        Some(BlockResult {
            retired,
            guest_bytecodes: chain.guest_bytecodes,
            transfer,
            object_commands: chain.object_commands,
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
        &mut self,
        cpu: &Processor,
        irq: bool,
        image: &Image,
        out: &mut NativeWrites,
    ) -> Option<Result<Option<Command>, u8>> {
        let pc = cpu.pc as usize;
        if self.entries.get(pc)?.is_none()
            && self.lazy
            && !self.compilation_failed
            && self.heat[pc].preparation != DISABLED
        {
            self.heat_preparation(pc, image);
        }
        let entry = self.entries.get(pc)?.as_ref()?;
        let stats = &mut self.stats;
        stats.preparations += 1;
        let d = match entry.prepare {
            Preparation::Scalar(prepare) => {
                *out = NativeWrites::Scalar(ScalarWrites::default());
                let NativeWrites::Scalar(writes) = out else {
                    unreachable!()
                };
                // SAFETY: build-local C ABI with live, disjoint inputs/output.
                let fault = unsafe { prepare(cpu, writes, u8::from(irq), &self.tables()) };
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
                let fault = unsafe { prepare(cpu, writes, u8::from(irq), &self.tables()) };
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
        &mut self,
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
