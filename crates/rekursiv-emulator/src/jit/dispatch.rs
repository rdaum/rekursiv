//! A bounded native loop over already-compiled regions. Each region publishes
//! CPU state normally; this removes Rust dispatch and accounting between them.
//! The caller guarantees a quiet device interval and no pending object reply.
use super::*;
use cranelift_codegen::ir::{condcodes::IntCC, types::*, AbiParam, InstBuilder, MemFlagsData};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Linkage, Module};
use std::mem::{offset_of, size_of};

pub(super) const MAX_INSTRUCTIONS: u64 = 1024;

#[derive(Default)]
#[repr(C)]
pub(super) struct ChainResult {
    pub retired: u64,
    pub guest_bytecodes: u64,
    pub object_commands: u64,
    pub blocks: u64,
    pub result: u64,
}
pub(super) type RunChain = unsafe extern "C" fn(
    *mut Processor,
    *mut ExternalDevice,
    *mut Option<eyre::Report>,
    *const Tables,
    *mut rekursiv_model::Model,
    *const native_objekt::MemoryView,
    *const Block,
    usize,
    u64,
    *mut ChainResult,
);

impl Jit {
    pub(super) fn compile_dispatch(&mut self) -> Result<()> {
        let module = self.module.as_mut().unwrap();
        let pointer = module.target_config().pointer_type();
        let mut signature = module.make_signature();
        signature
            .params
            .extend((0..8).map(|_| AbiParam::new(pointer)));
        signature
            .params
            .extend([AbiParam::new(I64), AbiParam::new(pointer)]);
        let id = module.declare_function("dispatch", Linkage::Local, &signature)?;
        let mut context = module.make_context();
        context.func.signature = signature;
        let mut frontend = FunctionBuilderContext::new();
        let mut b = FunctionBuilder::new(&mut context.func, &mut frontend);
        let entry = b.create_block();
        let next = b.create_block();
        let lookup = b.create_block();
        let call = b.create_block();
        let done = b.create_block();
        b.append_block_params_for_function_params(entry);
        for block in [next, done] {
            for _ in 0..5 {
                b.append_block_param(block, I64);
            }
        }
        b.switch_to_block(entry);
        let args = b.block_params(entry).to_vec();
        let zero = b.ins().iconst(I64, 0);
        let tickless = b.ins().iconst(I8, 1);
        b.ins().jump(next, &[zero.into(); 5]);

        b.switch_to_block(next);
        let totals = b.block_params(next).to_vec();
        let remaining = b.ins().isub(args[8], totals[0]);
        let pc = b.ins().load(
            I16,
            MemFlagsData::trusted(),
            args[0],
            offset_of!(Processor, pc) as i32,
        );
        let pc = b.ins().uextend(pointer, pc);
        let in_table = b.ins().icmp(IntCC::UnsignedLessThan, pc, args[7]);
        let exit_args: Vec<_> = totals.iter().copied().map(Into::into).collect();
        b.ins().brif(in_table, lookup, &[], done, &exit_args);

        b.switch_to_block(lookup);
        let offset = b.ins().imul_imm_s(pc, size_of::<Block>() as i64);
        let block = b.ins().iadd(args[6], offset);
        // Option<extern fn> uses a null pointer for None. Block has a C layout.
        let target = b.ins().load(
            pointer,
            MemFlagsData::trusted(),
            block,
            offset_of!(Block, run) as i32,
        );
        let length = b.ins().load(
            I8,
            MemFlagsData::trusted(),
            block,
            offset_of!(Block, length) as i32,
        );
        let length = b.ins().uextend(I64, length);
        let available = b.ins().icmp_imm_s(IntCC::NotEqual, target, 0);
        let nonempty = b.ins().icmp_imm_s(IntCC::NotEqual, length, 0);
        let fits = b
            .ins()
            .icmp(IntCC::UnsignedLessThanOrEqual, length, remaining);
        let runnable = b.ins().band(available, nonempty);
        let runnable = b.ins().band(runnable, fits);
        b.ins().brif(runnable, call, &[], done, &exit_args);

        b.switch_to_block(call);
        let mut region_signature = module.make_signature();
        region_signature
            .params
            .extend((0..5).map(|_| AbiParam::new(pointer)));
        region_signature
            .params
            .extend([AbiParam::new(I8), AbiParam::new(pointer)]);
        region_signature.returns.push(AbiParam::new(I64));
        let region_signature = b.import_signature(region_signature);
        let invoked = b.ins().call_indirect(
            region_signature,
            target,
            &[
                args[0], args[1], args[2], args[3], args[4], tickless, args[5],
            ],
        );
        let result = b.inst_results(invoked)[0];
        let retired = b.ins().ushr_imm_s(result, 8);
        let retired = b.ins().band_imm_s(retired, 0xffff);
        let guest = b.ins().ushr_imm_s(result, 24);
        let guest = b.ins().band_imm_s(guest, 0xff);
        let commands = b.ins().ushr_imm_s(result, 56);
        let all_retired = b.ins().iadd(totals[0], retired);
        let all_guest = b.ins().iadd(totals[1], guest);
        let all_commands = b.ins().iadd(totals[2], commands);
        let calls = b.ins().iadd_imm_s(totals[3], 1);
        let updated = [
            all_retired.into(),
            all_guest.into(),
            all_commands.into(),
            calls.into(),
            result.into(),
        ];
        let status = b.ins().band_imm_s(result, 0xff);
        let success = b.ins().icmp_imm_s(IntCC::Equal, status, 0);
        let progress = b.ins().icmp_imm_s(IntCC::NotEqual, retired, 0);
        let more = b.ins().icmp(IntCC::UnsignedLessThan, all_retired, args[8]);
        let again = b.ins().band(success, progress);
        let again = b.ins().band(again, more);
        b.ins().brif(again, next, &updated, done, &updated);

        b.switch_to_block(done);
        let results = b.block_params(done).to_vec();
        for (value, offset) in results.into_iter().zip([
            offset_of!(ChainResult, retired),
            offset_of!(ChainResult, guest_bytecodes),
            offset_of!(ChainResult, object_commands),
            offset_of!(ChainResult, blocks),
            offset_of!(ChainResult, result),
        ]) {
            b.ins()
                .store(MemFlagsData::trusted(), value, args[9], offset as i32);
        }
        b.ins().return_(&[]);
        b.seal_all_blocks();
        b.finalize(module.target_config());
        module.define_function(id, &mut context)?;
        module.finalize_definitions()?;
        // SAFETY: the emitted signature is RunChain; Jit retains code ownership.
        self.dispatch = Some(unsafe {
            std::mem::transmute::<*const u8, RunChain>(module.get_finalized_function(id))
        });
        self.stats.functions += 1;
        Ok(())
    }
}
