//! Demand-driven translation. Cold words use the existing executor; native
//! regions and single-word preparations are compiled only after repeated use.
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
    pub(crate) fn new(image: &Image, guest_dispatch_pc: Option<u16>) -> Result<Self> {
        let mut builder =
            JITBuilder::with_flags(&[("opt_level", "speed")], default_libcall_names())?;
        builder.symbol("rekursiv_jit_tick", tick as *const u8);
        let mut jit = Self {
            module: Some(JITModule::new(builder)),
            nam: Vec::new(),
            map: Vec::new(),
            entries: (0..image.code.len()).map(|_| None).collect(),
            blocks: (0..image.code.len()).map(|_| Block::default()).collect(),
            block_sources: vec![Vec::new(); image.code.len()],
            heat: vec![Heat::default(); image.code.len()],
            preparations: HashMap::new(),
            guest_dispatch_pc,
            lazy: true,
            compilation_failed: false,
            stats: JitStatistics::default(),
            dispatch: None,
        };
        jit.refresh_tables(image);
        Ok(jit)
    }

    // Explicit eager mode for native-path validation and controlled comparisons.
    pub(crate) fn compile(image: &Image, guest_dispatch_pc: Option<u16>) -> Result<Self> {
        let mut jit = Self::new(image, guest_dispatch_pc)?;
        jit.lazy = false;
        let started = std::time::Instant::now();
        jit.compile_all(image)?;
        jit.stats.compilation_time += started.elapsed();
        Ok(jit)
    }
    fn compile_all(&mut self, image: &Image) -> Result<()> {
        for pc in 0..image.code.len() {
            self.compile_preparation(image, pc)?;
        }
        let mut leaders = std::collections::BTreeSet::from([0usize]);
        leaders.extend(image.collector_entry.map(usize::from));
        leaders.extend(self.guest_dispatch_pc.map(usize::from));
        leaders.extend(image.map.iter().flatten().map(|&pc| pc as usize));
        for (pc, instruction) in image.code.iter().enumerate() {
            let Some(i) = instruction else { continue };
            if pc == 0 || image.code[pc - 1].is_none() {
                leaders.insert(pc);
            }
            if i.seq != Seq::Continue
                || decode_region(*i).is_none()
                || i.halt
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

        for start in 0..image.code.len() {
            if let Some(code) = regions.remove(&start).filter(|code| !code.is_empty()) {
                self.install_region(start, code);
                self.compile_region(start)?;
            }
        }
        Ok(())
    }

    pub(super) fn compile_preparation(&mut self, image: &Image, pc: usize) -> Result<()> {
        let Some(decoded) = image.code[pc].and_then(decode) else {
            return Ok(());
        };
        let key = decoded.instruction.encode()?;
        if let Some(&prepare) = self.preparations.get(&key) {
            self.entries[pc] = Some(Entry {
                source: decoded.instruction,
                prepare,
            });
            return Ok(());
        }
        let module = self.module.as_mut().unwrap();
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
        let id = module.declare_function(
            &format!("micro_{}", self.stats.functions),
            Linkage::Local,
            &signature,
        )?;
        context.func.signature = signature;
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
        module.finalize_definitions()?;

        let code = module.get_finalized_function(id);
        // SAFETY: signatures match the emitted output record, and this owner
        // retains all finalized code until no synchronous call can be active.
        let prepare = if expanded(decoded.instruction) {
            Preparation::StackFetch(unsafe {
                std::mem::transmute::<*const u8, PrepareStackFetch>(code)
            })
        } else {
            Preparation::Scalar(unsafe { std::mem::transmute::<*const u8, Prepare>(code) })
        };
        self.preparations.insert(key, prepare);
        self.entries[pc] = Some(Entry {
            source: decoded.instruction,
            prepare,
        });
        self.stats.functions += 1;
        Ok(())
    }

    pub(super) fn install_region(&mut self, pc: usize, source: Vec<(usize, Instruction)>) {
        self.blocks[pc] = Block {
            run: None,
            length: source.len() as u8,
            object: source.iter().any(|(_, i)| i.object.is_some()),
            barrier: source.iter().any(|(_, i)| i.object_barrier()),
            interrupt: source
                .iter()
                .any(|(_, i)| i.condition == Condition::Interrupt),
            recovery_safe: source
                .iter()
                .all(|(_, i)| crate::scalar::ScalarInstruction::decode_recovery(*i).is_some()),
        };
        self.block_sources[pc] = source;
    }

    pub(super) fn discover_region(&mut self, image: &Image, pc: usize) {
        self.install_region(pc, region(image, pc));
    }

    pub(super) fn compile_region(&mut self, start: usize) -> Result<()> {
        if self.dispatch.is_none() {
            self.compile_dispatch()?;
        }
        let code = &self.block_sources[start];
        let module = self.module.as_mut().unwrap();
        let mut context = module.make_context();
        let mut frontend = FunctionBuilderContext::new();
        let pointer = module.target_config().pointer_type();
        let mut tick_signature = module.make_signature();
        tick_signature
            .params
            .extend([AbiParam::new(pointer), AbiParam::new(pointer)]);
        tick_signature.returns.push(AbiParam::new(I8));
        let tick_id =
            module.declare_function("rekursiv_jit_tick", Linkage::Import, &tick_signature)?;
        let mut block_signature = module.make_signature();
        block_signature.params.extend([
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(pointer),
            AbiParam::new(I8),
            AbiParam::new(pointer),
        ]);
        block_signature.returns.push(AbiParam::new(I64));
        let id = module.declare_function(
            &format!("block_{}", self.stats.functions),
            Linkage::Local,
            &block_signature,
        )?;
        context.func.signature = block_signature;
        let tick_ref = module.declare_func_in_func(tick_id, &mut context.func);
        let mut b = FunctionBuilder::new(&mut context.func, &mut frontend);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let params = b.block_params(entry).to_vec();
        let irq = b.ins().load(
            I8,
            cranelift_codegen::ir::MemFlagsData::trusted(),
            params[3],
            std::mem::offset_of!(Tables, irq) as i32,
        );
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
            emit.instruction(&decode_region(instruction).unwrap());
            // Encode the successful prefix in each exit constant. Native
            // instructions need no PC comparison, callback, or increment.
            if self.guest_dispatch_pc == Some(pc as u16) {
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
        module.finalize_definitions()?;

        // SAFETY: block_signature matches RunBlock; executable pages stay owned.
        let run = unsafe {
            std::mem::transmute::<*const u8, RunBlock>(module.get_finalized_function(id))
        };
        self.blocks[start].run = Some(run);
        self.stats.functions += 1;
        Ok(())
    }
}

// Root accesses are currently emitted only for retiring regions. The generic
// preparation path retains its existing checked write-set implementation.
fn decode_region(i: Instruction) -> Option<crate::scalar::ScalarInstruction> {
    i.validate().ok()?;
    let mut decoded = decode(Instruction {
        write_root: false,
        bus: if i.bus == Bus::Root {
            Bus::Immediate
        } else {
            i.bus
        },
        ..i
    })?;
    decoded.instruction = i;
    decoded.bus |= i.write_root || i.bus == Bus::Root;
    Some(decoded)
}

fn region(image: &Image, start: usize) -> Vec<(usize, Instruction)> {
    let mut code = Vec::new();
    let mut pc = start;
    for _ in 0..32 {
        let Some(Some(i)) = image.code.get(pc).copied() else {
            break;
        };
        if decode_region(i).is_none()
            || i.object
                .is_some_and(|c| !resident::eligible(c) || i.object_async)
            || i.halt
            || i.seq == Seq::Service
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
