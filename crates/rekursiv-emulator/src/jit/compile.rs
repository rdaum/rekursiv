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
    pub(crate) fn compile(image: &Image) -> Result<Self> {
        let mut builder =
            JITBuilder::with_flags(&[("opt_level", "speed")], default_libcall_names())?;
        builder.symbol("rekursiv_jit_tick", tick as *const u8);
        builder.symbol("rekursiv_jit_resident", resident::execute as *const u8);
        builder.symbol("rekursiv_jit_nam", stack_fetch::read_nam as *const u8);
        builder.symbol("rekursiv_jit_map", stack_fetch::read_map as *const u8);
        let mut jit = Self {
            module: Some(JITModule::new(builder)),
            entries: Vec::new(),
            blocks: Vec::new(),
            block_sources: Vec::new(),
            stats: Cell::default(),
            commands: image
                .code
                .iter()
                .map(|i| i.and_then(|i| i.object).unwrap_or_default())
                .collect(),
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
        let mut object_signature = module.make_signature();
        object_signature.params.extend([
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(I64),
        ]);
        object_signature.returns.push(AbiParam::new(I64));
        let object_id =
            module.declare_function("rekursiv_jit_resident", Linkage::Import, &object_signature)?;
        let mut nam_signature = module.make_signature();
        nam_signature.params.extend([
            AbiParam::new(pointer),
            AbiParam::new(I32),
            AbiParam::new(pointer),
        ]);
        nam_signature.returns.push(AbiParam::new(I8));
        let nam_id =
            module.declare_function("rekursiv_jit_nam", Linkage::Import, &nam_signature)?;
        let mut map_signature = module.make_signature();
        map_signature
            .params
            .extend([AbiParam::new(pointer), AbiParam::new(pointer)]);
        map_signature.returns.push(AbiParam::new(I32));
        let map_id =
            module.declare_function("rekursiv_jit_map", Linkage::Import, &map_signature)?;
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
                let nam = module.declare_func_in_func(nam_id, &mut context.func);
                let map = module.declare_func_in_func(map_id, &mut context.func);
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
                    image: params[3],
                    pointer,
                    nam,
                    map,
                    deferred_object: None,
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
        block_signature.returns.push(AbiParam::new(I64));
        let mut block_ids = Vec::new();
        for start in 0..image.code.len() {
            let mut code = Vec::new();
            for instruction in image.code[start..].iter().take(32) {
                let Some(i) = *instruction else { break };
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
                code.push(*instruction);
                if i.seq != Seq::Continue {
                    break;
                }
            }
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
            let object_ref = module.declare_func_in_func(object_id, &mut context.func);
            let nam = module.declare_func_in_func(nam_id, &mut context.func);
            let map = module.declare_func_in_func(map_id, &mut context.func);
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
                image: params[3],
                pointer,
                nam,
                map,
                deferred_object: None,
                mode: Mode::Block {
                    tick: tick_ref,
                    device: params[1],
                    error: params[2],
                    retired: 0,
                    object: object_ref,
                    model: params[4],
                    command: (&jit.commands[start]) as *const Command as usize,
                    object_commands: 0,
                },
            };
            for (n, instruction) in code.iter().enumerate() {
                if let Mode::Block {
                    retired, command, ..
                } = &mut emit.mode
                {
                    *retired = n as u64;
                    *command = (&jit.commands[start + n]) as *const Command as usize;
                }
                emit.instruction(&decode(instruction.unwrap()).unwrap());
            }
            let object_commands =
                code.iter().flatten().filter(|i| i.object.is_some()).count() as u64;
            let result = emit.b.ins().iconst(
                I64,
                ((code.len() as u64) << 8 | (object_commands << 56)) as i64,
            );
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
                let object = source.iter().flatten().any(|i| i.object.is_some());
                let barrier = source.iter().flatten().any(|i| i.object_barrier());
                let recovery_safe = source
                    .iter()
                    .flatten()
                    .all(|i| crate::scalar::ScalarInstruction::decode_recovery(*i).is_some());
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
