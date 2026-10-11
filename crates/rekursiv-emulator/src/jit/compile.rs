//! Compile instruction preparations and native blocks; retain cold source
//! snapshots separately from the compact runtime dispatch table.
use super::*;
use super::{
    emit::{Emit, Mode},
    stack_fetch::{decode, expanded},
};
use cranelift_codegen::ir::{types::*, AbiParam, InstBuilder};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::JITBuilder;
use cranelift_module::{default_libcall_names, Linkage, Module};
use std::collections::HashMap;

impl Jit {
    pub(crate) fn compile(image: &Image, guest_dispatch_pc: Option<u16>) -> Result<Self> {
        let mut builder =
            JITBuilder::with_flags(&[("opt_level", "speed")], default_libcall_names())?;
        builder.symbol("rekursiv_jit_tick", tick as *const u8);
        let mut jit = Self {
            module: Some(JITModule::new(builder)),
            nam: Vec::new(),
            map: Vec::new(),
            entries: Vec::new(),
            blocks: Vec::new(),
            block_sources: Vec::new(),
            stats: Cell::default(),
        };
        jit.refresh_tables(image);
        let module = jit.module.as_mut().unwrap();
        let mut context = module.make_context();
        let mut frontend = FunctionBuilderContext::new();
        let pointer = module.target_config().pointer_type();
        let mut signature = module.make_signature();
        signature.params.extend([
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(I8),
            AbiParam::new(pointer),
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
            let Some(decoded) = instruction.and_then(decode) else {
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
                    tables: params[3],
                    pointer,
                    deferred_object: None,
                    values: HashMap::new(),
                    dirty: std::collections::BTreeSet::new(),
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
        // Traces follow conditional fallthrough, with a successful-prefix exit
        // when the branch is taken. Static jumps and immediate relative branches
        // remain inside the region; dynamic transfers end it. Every installed
        // statically known control-flow entry gets a region.
        // Resident object operations may share a block. Machine joins older
        // replies before entry; run_block declines a block with a later barrier
        // while a reply is pending, preserving intervening local retirements.
        let mut block_signature = module.make_signature();
        block_signature.params.extend([
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
        ]);
        block_signature
            .params
            .extend([AbiParam::new(pointer), AbiParam::new(pointer)]);
        block_signature.params.push(AbiParam::new(I8));
        block_signature.params.push(AbiParam::new(pointer));
        block_signature.returns.push(AbiParam::new(I64));
        // Compile real control-flow entries rather than every overlapping suffix.
        // Unknown computed entries retain individual preparations until reaching
        // a compiled entry. Closing the set over region ends avoids fallback at
        // the arbitrary 32-word limit.
        let mut leaders = std::collections::BTreeSet::from([0usize]);
        leaders.extend(image.collector_entry.map(usize::from));
        leaders.extend(guest_dispatch_pc.map(usize::from));
        leaders.extend(image.map.iter().flatten().map(|&pc| pc as usize));
        for (pc, instruction) in image.code.iter().enumerate() {
            let Some(i) = instruction else { continue };
            if pc == 0 || image.code[pc - 1].is_none() {
                leaders.insert(pc);
            }
            if i.seq != Seq::Continue
                || decode(*i).is_none()
                || i.halt
                || i.condition == Condition::Interrupt
                || i.allocation_dynamic
                || i.object
                    .is_some_and(|c| !resident::eligible(c) || i.object_async)
            {
                leaders.insert(pc + 1);
            }
            if i.mark {
                leaders.insert(pc);
            }
            if let Some(target) = static_target(pc, *i) {
                leaders.insert(target);
            }
        }
        let mut pending: Vec<_> = leaders.iter().copied().collect();
        let mut regions = HashMap::new();
        while let Some(start) = pending.pop() {
            if start >= image.code.len() {
                continue;
            }
            let code = region(image, start);
            if code.len() == 32 {
                let &(pc, i) = code.last().unwrap();
                let next = if matches!(
                    i.seq,
                    Seq::Continue | Seq::ConditionalJump | Seq::ConditionalRelative
                ) {
                    Some(pc + 1)
                } else {
                    static_target(pc, i)
                };
                if let Some(next) = next {
                    if leaders.insert(next) {
                        pending.push(next);
                    }
                }
            }
            regions.insert(start, code);
        }
        let mut block_ids = Vec::new();
        for start in 0..image.code.len() {
            let code = regions.remove(&start).unwrap_or_default();
            if code.is_empty() {
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
                tables: params[3],
                pointer,
                deferred_object: None,
                values: HashMap::new(),
                dirty: std::collections::BTreeSet::new(),
                mode: Mode::Block {
                    tick: tick_ref,
                    tickless: params[5],
                    device: params[1],
                    error: params[2],
                    retired: 0,
                    model: params[4],
                    memory_view: params[6],
                    object_commands: 0,
                    guest_bytecodes: 0,
                },
            };
            for (n, &(pc, instruction)) in code.iter().enumerate() {
                if let Mode::Block { retired, .. } = &mut emit.mode {
                    *retired = n as u64;
                }
                emit.instruction(&decode(instruction).unwrap());
                // Encode the successful prefix in each exit constant. Native
                // instructions need no PC comparison, callback, or increment.
                if guest_dispatch_pc == Some(pc as u16) {
                    if let Mode::Block {
                        guest_bytecodes, ..
                    } = &mut emit.mode
                    {
                        *guest_bytecodes += 1;
                    }
                }
                if matches!(
                    instruction.seq,
                    Seq::ConditionalJump | Seq::ConditionalRelative
                ) && n + 1 < code.len()
                {
                    // The branch has retired, including its simultaneous
                    // writes. A taken edge must return its exact successful
                    // prefix, without executing or counting the fallthrough.
                    if let Mode::Block { retired, .. } = &mut emit.mode {
                        *retired = (n + 1) as u64;
                    }
                    emit.exit_unless_sequential(code[n + 1].0 as u16);
                }
            }
            let object_commands = code.iter().filter(|(_, i)| i.object.is_some()).count() as u64;
            let Mode::Block {
                guest_bytecodes, ..
            } = emit.mode
            else {
                unreachable!()
            };
            let result = emit.b.ins().iconst(
                I64,
                block_result(0, code.len() as u64, object_commands, guest_bytecodes) as i64,
            );
            emit.flush_cpu();
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
                    let prepare = if expanded(source) {
                        Preparation::StackFetch(unsafe {
                            std::mem::transmute::<*const u8, PrepareStackFetch>(code)
                        })
                    } else {
                        Preparation::Scalar(unsafe {
                            std::mem::transmute::<*const u8, Prepare>(code)
                        })
                    };
                    Entry { source, prepare }
                })
            })
            .collect();
        (jit.blocks, jit.block_sources) = block_ids
            .into_iter()
            .map(|entry| {
                let Some((source, id)) = entry else {
                    return (Block::default(), Vec::new());
                };
                // SAFETY: this is block_signature; Jit owns code and templates.
                let run = unsafe {
                    std::mem::transmute::<*const u8, RunBlock>(module.get_finalized_function(id))
                };
                let object = source.iter().any(|(_, i)| i.object.is_some());
                let barrier = source.iter().any(|(_, i)| i.object_barrier());
                let recovery_safe = source
                    .iter()
                    .all(|(_, i)| crate::scalar::ScalarInstruction::decode_recovery(*i).is_some());
                (
                    Block {
                        run: Some(run),
                        length: source.len() as u8,
                        object,
                        barrier,
                        recovery_safe,
                    },
                    source,
                )
            })
            .unzip();
        jit.stats.get_mut().functions =
            unique.len() + jit.blocks.iter().filter(|b| b.run.is_some()).count();
        Ok(jit)
    }
}

fn region(image: &Image, start: usize) -> Vec<(usize, Instruction)> {
    let mut code = Vec::new();
    let mut pc = start;
    for _ in 0..32 {
        let Some(Some(i)) = image.code.get(pc).copied() else {
            break;
        };
        if decode(i).is_none()
            || i.object
                .is_some_and(|c| !resident::eligible(c) || i.object_async)
            || i.halt
            || i.seq == Seq::Service
            || i.condition == Condition::Interrupt
            || i.allocation_dynamic
        {
            break;
        }
        code.push((pc, i));
        // Follow statically known branches inside the native region.
        // Keep a finite unrolled prefix even for a microcode loop.
        pc = match i.seq {
            Seq::Jump => i.branch as usize,
            Seq::Relative if i.bus == Bus::Immediate => {
                let next = pc as i64 + i.data.as_index();
                if !(0..rekursiv_model::processor::CODE_WORDS as i64).contains(&next) {
                    break;
                }
                next as usize
            }
            Seq::Continue | Seq::ConditionalJump | Seq::ConditionalRelative => pc + 1,
            _ => break,
        };
    }

    code
}

fn static_target(pc: usize, i: Instruction) -> Option<usize> {
    let target = match i.seq {
        Seq::Jump | Seq::ConditionalJump | Seq::TwoWay => i.branch as i64,
        Seq::Relative | Seq::ConditionalRelative if i.bus == Bus::Immediate => {
            pc as i64 + i.data.as_index()
        }
        Seq::Bus | Seq::ConditionalBus if i.bus == Bus::Immediate => i.data.bits() as i64,
        _ => return None,
    };
    (0..rekursiv_model::processor::CODE_WORDS as i64)
        .contains(&target)
        .then_some(target as usize)
}
