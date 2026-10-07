//! Cranelift translation of the scalar microinstruction datapath.
//!
//! Single-word translations prepare a small write set for the executor's
//! object-command and retirement path. Native blocks commit local instructions
//! directly, with a peripheral tick at each boundary. Stack, fetch,
//! floating-point, and collector words use the checked interpreter. This is translation of microcode, not of Smalltalk bytecodes.
//!
//! All machine-code pointers are private to this owner. Offsets come from this
//! build's Rust layouts, so no stable ABI is assumed for Processor. The function
//! ABIs use C calling conventions; private function types below specify them.
use crate::scalar::{ScalarInstruction, ScalarWrites};
use cranelift_codegen::ir::{types::*, AbiParam, InstBuilder};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{default_libcall_names, Linkage, Module};
use eyre::Result;
use rekursiv_asm::{processor::*, Command, Word};
use rekursiv_devices::Device as ExternalDevice;
use rekursiv_model::processor::{Image, Processor};
use std::{cell::Cell, collections::HashMap, mem::MaybeUninit};

mod emit;
use emit::{Emit, Mode};

type RunBlock =
    unsafe extern "C" fn(*mut Processor, *mut ExternalDevice, *mut Option<eyre::Report>) -> u64;
struct Block {
    source: Vec<Option<Instruction>>,
    run: RunBlock,
}
pub(crate) struct BlockResult {
    pub retired: u64,
    pub fault: u8,
    pub error: Option<eyre::Report>,
}

// No generated frame unwinds through Rust. Peripheral errors cross the ABI in
// an owned error slot, then the safe caller returns them normally.
extern "C" fn tick(device: &mut ExternalDevice, error: &mut Option<eyre::Report>) -> u8 {
    match device.tick(None, false) {
        Ok(()) => 0,
        Err(e) => {
            *error = Some(e);
            1
        }
    }
}

type Prepare = unsafe extern "C" fn(*const Processor, *mut ScalarWrites, u8) -> u8;
struct Entry {
    source: Instruction,
    prepare: Prepare,
}

/// Observations of native execution, separate from architectural counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JitStatistics {
    pub functions: usize,
    pub preparations: u64,
    pub block_calls: u64,
    pub block_instructions: u64,
}

pub(crate) struct Jit {
    // Option permits consuming the module in Drop to release executable pages.
    module: Option<JITModule>,
    entries: Vec<Option<Entry>>,
    blocks: Vec<Option<Block>>,
    stats: Cell<JitStatistics>,
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
    pub(crate) fn compile(image: &Image) -> Result<Self> {
        let mut builder =
            JITBuilder::with_flags(&[("opt_level", "speed")], default_libcall_names())?;
        builder.symbol("rekursiv_jit_tick", tick as *const u8);
        let mut jit = Self {
            module: Some(JITModule::new(builder)),
            entries: Vec::new(),
            blocks: Vec::new(),
            stats: Cell::default(),
        };
        let module = jit.module.as_mut().unwrap();
        let mut context = module.make_context();
        let mut frontend = FunctionBuilderContext::new();
        let pointer = module.target_config().pointer_type();
        let mut signature = module.make_signature();
        signature.params.extend([
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(I8),
        ]);
        signature.returns.push(AbiParam::new(I8));
        let mut tick_signature = module.make_signature();
        tick_signature
            .params
            .extend([AbiParam::new(pointer), AbiParam::new(pointer)]);
        tick_signature.returns.push(AbiParam::new(I8));
        let tick_id =
            module.declare_function("rekursiv_jit_tick", Linkage::Import, &tick_signature)?;
        let mut unique = HashMap::new();
        let mut ids = Vec::with_capacity(image.code.len());
        for instruction in &image.code {
            let Some(decoded) = instruction.and_then(ScalarInstruction::decode) else {
                ids.push(None);
                continue;
            };
            let key = decoded.instruction.encode()?;
            let id = if let Some(&id) = unique.get(&key) {
                id
            } else {
                let id = module.declare_function(
                    &format!("micro_{}", unique.len()),
                    Linkage::Local,
                    &signature,
                )?;
                context.func.signature = signature.clone();
                let mut b = FunctionBuilder::new(&mut context.func, &mut frontend);
                let entry = b.create_block();
                b.append_block_params_for_function_params(entry);
                b.switch_to_block(entry);
                let params = b.block_params(entry).to_vec();
                let mut emit = Emit {
                    b,
                    cpu: params[0],
                    out: params[1],
                    irq: params[2],
                    mode: Mode::Prepare,
                };
                emit.instruction(&decoded);
                emit.b.seal_all_blocks();
                emit.b.finalize(module.target_config());
                module.define_function(id, &mut context)?;
                module.clear_context(&mut context);
                unique.insert(key, id);
                id
            };
            ids.push(Some((decoded.instruction, id)));
        }
        // Basic blocks end at control flow or an architectural boundary. Each
        // suffix is an entry point because microcode can jump into a block.
        let mut block_signature = module.make_signature();
        block_signature.params.extend([
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
        ]);
        block_signature.returns.push(AbiParam::new(I64));
        let mut block_ids = Vec::new();
        for start in 0..image.code.len() {
            let mut code = Vec::new();
            for instruction in image.code[start..].iter().take(32) {
                let Some(i) = *instruction else { break };
                if ScalarInstruction::decode(i).is_none()
                    || i.object_barrier()
                    || i.condition == Condition::Interrupt
                    || i.allocation_dynamic
                {
                    break;
                }
                code.push(*instruction);
                if i.seq != Seq::Continue {
                    break;
                }
            }
            if code.len() < 2 {
                block_ids.push(None);
                continue;
            }
            let id = module.declare_function(
                &format!("block_{start}"),
                Linkage::Local,
                &block_signature,
            )?;
            context.func.signature = block_signature.clone();
            let tick_ref = module.declare_func_in_func(tick_id, &mut context.func);
            let mut b = FunctionBuilder::new(&mut context.func, &mut frontend);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let params = b.block_params(entry).to_vec();
            let irq = b.ins().iconst(I8, 0);
            let mut emit = Emit {
                b,
                cpu: params[0],
                out: params[0],
                irq,
                mode: Mode::Block {
                    tick: tick_ref,
                    device: params[1],
                    error: params[2],
                    retired: 0,
                },
            };
            for (n, instruction) in code.iter().enumerate() {
                if let Mode::Block { retired, .. } = &mut emit.mode {
                    *retired = n as u64;
                }
                emit.instruction(&ScalarInstruction::decode(instruction.unwrap()).unwrap());
            }
            let result = emit.b.ins().iconst(I64, (code.len() as i64) << 8);
            emit.b.ins().return_(&[result]);
            emit.b.seal_all_blocks();
            emit.b.finalize(module.target_config());
            module.define_function(id, &mut context)?;
            module.clear_context(&mut context);
            block_ids.push(Some((code, id)));
        }
        module.finalize_definitions()?;
        jit.entries = ids
            .into_iter()
            .map(|entry| {
                entry.map(|(source, id)| {
                    let code = module.get_finalized_function(id);
                    // SAFETY: signature above exactly matches Prepare; finalized code
                    // stays executable until this owner is dropped.
                    let prepare = unsafe { std::mem::transmute::<*const u8, Prepare>(code) };
                    Entry { source, prepare }
                })
            })
            .collect();
        jit.blocks = block_ids
            .into_iter()
            .map(|entry| {
                entry.map(|(source, id)| {
                    // SAFETY: this is the block_signature declared above. The owner
                    // controls its lifetime and all calls use live, disjoint objects.
                    let run = unsafe {
                        std::mem::transmute::<*const u8, RunBlock>(
                            module.get_finalized_function(id),
                        )
                    };
                    Block { source, run }
                })
            })
            .collect();
        jit.stats.get_mut().functions = unique.len() + jit.blocks.iter().flatten().count();
        Ok(jit)
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
    ) -> Option<BlockResult> {
        let pc = cpu.pc as usize;
        let block = self.blocks.get(pc)?.as_ref()?;
        if block.source.len() as u64 > budget
            || cpu.roots.is_none()
            || image.code.get(pc..pc + block.source.len())? != block.source
        {
            return None;
        }
        let mut error = None;
        // SAFETY: guarded code matches the current image, fields are accessed
        // with build-local offsets, and the three pointed-to objects are live
        // and disjoint throughout this synchronous call.
        let result = unsafe { (block.run)(cpu, devices, &mut error) };
        let mut stats = self.stats.get();
        stats.block_calls += 1;
        stats.block_instructions += result >> 8;
        self.stats.set(stats);
        Some(BlockResult {
            retired: result >> 8,
            fault: result as u8,
            error,
        })
    }

    #[inline]
    pub(crate) fn prepare(
        &self,
        cpu: &Processor,
        i: Instruction,
        irq: bool,
    ) -> Option<Result<(ScalarWrites, Option<Command>), u8>> {
        let entry = self.entries.get(cpu.pc as usize)?.as_ref()?;
        // The control store is public and writable. Never execute an old
        // translation after an edit, even if it changes only a command field.
        if entry.source != i {
            return None;
        }
        let mut stats = self.stats.get();
        stats.preparations += 1;
        self.stats.set(stats);
        let mut writes = MaybeUninit::<ScalarWrites>::uninit();
        // SAFETY: valid CPU/output objects, nonoverlapping, correct signature.
        // On success the generated function initializes every output field.
        let fault = unsafe { (entry.prepare)(cpu, writes.as_mut_ptr(), u8::from(irq)) };
        if fault != 0 {
            return Some(Err(fault));
        }
        // SAFETY: zero status means every ScalarWrites field was initialized,
        // including a canonical 0/1 value for the boolean condition field.
        let writes = unsafe { writes.assume_init() };
        let command = i.object.map(|mut c| {
            if i.allocation_dynamic {
                c.alloc_size = cpu.rf[i.ra as usize];
            }
            c.data = Word::from_bits(writes.d).unwrap();
            c
        });
        Some(Ok((writes, command)))
    }
}

#[cfg(test)]
mod tests;
