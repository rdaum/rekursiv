//! Resident OBJEKT execution inside native microcode blocks.
//!
//! Blocks with object barriers run only after an older asynchronous reply has
//! joined. A Fetch miss exits before issue; transfers and LAUNCH retain the
//! general executor. Resident commands cannot enter collection. Local writes
//! remain SSA values until the object response and peripheral tick succeed.
use super::emit::{Emit, Mode};
use cranelift_codegen::ir::{types::I64, InstBuilder};
use rekursiv_asm::{Command, Pager, Word};
use rekursiv_model::{processor::Processor, Model};
use std::mem::offset_of;

pub(super) fn eligible(c: Command) -> bool {
    matches!(
        c.pager,
        Pager::None
            | Pager::Fetch
            | Pager::ProbeBus
            | Pager::ProbeVr
            | Pager::ProbeType
            | Pager::ProbeRepresentation
    )
}

// Distinct from architectural faults and the device-error sentinel (255).
pub(super) const TRANSFER: u8 = 254;
const MISS: u64 = 1 << 63;

/// C ABI adapter for the existing architectural model. The immutable command
/// template belongs to Jit; only the D-bus value varies at execution time.
pub(super) extern "C" fn execute(model: &mut Model, template: &Command, data: u64) -> u64 {
    let mut c = *template;
    c.data = Word::from_bits(data).expect("validated 40-bit D bus");
    // A miss has no effects here. Leave the current PC and its destinations
    // untouched so Machine can run the transfer/collection path exactly once.
    if c.pager == Pager::Fetch && model.resolve(c.data).is_err() {
        return MISS;
    }
    let response = model.execute_response(c, false);
    response.data.bits() | ((response.status as u64) << 40)
}

impl Emit<'_> {
    pub(super) fn resident(&mut self, d: cranelift_codegen::ir::Value) {
        let Mode::Block {
            object,
            model,
            command,
            retired,
            object_commands,
            ..
        } = self.mode
        else {
            unreachable!()
        };
        let template = self.b.ins().iconst(self.pointer, command as i64);
        let call = self.b.ins().call(object, &[model, template, d]);
        let response = self.b.inst_results(call)[0];
        let miss = self.b.ins().ushr_imm_s(response, 63);
        self.reject(miss, i64::from(TRANSFER));
        let status = self.b.ins().ushr_imm_s(response, 40);
        let failed = self.b.create_block();
        let success = self.b.create_block();
        self.b.ins().brif(status, failed, &[], success, &[]);
        self.b.switch_to_block(failed);
        let status = self.b.ins().ishl_imm_s(status, 48);
        let prefix = self.b.ins().iconst(
            I64,
            (4 | (retired << 8) | ((object_commands + 1) << 56)) as i64,
        );
        let result = self.b.ins().bor(status, prefix);
        self.b.ins().return_(&[result]);
        self.b.switch_to_block(success);
        if let Mode::Block {
            object_commands, ..
        } = &mut self.mode
        {
            *object_commands += 1;
        }
        let data = self
            .b
            .ins()
            .band_imm_s(response, rekursiv_asm::WORD_MASK as i64);
        // Published by the caller after the device tick, with the other local
        // destinations. See deferred_object rather than storing CPU here.
        self.deferred_object = Some(data);
    }

    pub(super) fn publish_object(&mut self) {
        if let Some(data) = self.deferred_object.take() {
            self.cpu_store(offset_of!(Processor, object), data);
        }
    }
}
