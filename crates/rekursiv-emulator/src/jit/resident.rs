//! Resident OBJEKT execution inside native microcode blocks.
//!
//! Blocks with object barriers run only after an older asynchronous reply has
//! joined. A Fetch miss exits before issue; transfers and LAUNCH retain the
//! general executor. Resident commands cannot enter collection. Local writes
//! remain SSA values until the object response and peripheral tick succeed.
use super::emit::{Emit, Mode};
use cranelift_codegen::ir::{condcodes::IntCC, types::I64, InstBuilder, Value};
use rekursiv_asm::{Command, Pager, Status, INDEX_MAX, INDEX_MIN};
use rekursiv_model::processor::Processor;
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
impl Emit<'_> {
    pub(super) fn resident(&mut self, d: Value, c: Command) {
        assert!(self.native_resident(d, c));
    }

    // Status is zero on success. Match the model's attempted-command count and
    // leave all OBJEKT destinations untouched on failure.
    pub(super) fn object_failure(&mut self, status: Value) {
        let Mode::Block {
            retired,
            object_commands,
            guest_bytecodes,
            ..
        } = self.mode
        else {
            unreachable!()
        };
        let failed = self.b.create_block();
        let success = self.b.create_block();
        self.b.ins().brif(status, failed, &[], success, &[]);
        self.b.switch_to_block(failed);
        let status = self.b.ins().ishl_imm_s(status, 48);
        let prefix = self.b.ins().iconst(
            I64,
            super::block_result(4, retired, object_commands + 1, guest_bytecodes) as i64,
        );
        let result = self.b.ins().bor(status, prefix);
        self.flush_cpu();
        self.b.ins().return_(&[result]);
        self.b.switch_to_block(success);
    }

    pub(super) fn object_check(&mut self, bad: Value, status: Status) {
        let yes = self.b.ins().iconst(I64, status as i64);
        let no = self.b.ins().iconst(I64, 0);
        let status = self.b.ins().select(bad, yes, no);
        self.object_failure(status);
    }

    pub(super) fn index_check(&mut self, value: Value) {
        let below = self
            .b
            .ins()
            .icmp_imm_s(IntCC::SignedLessThan, value, INDEX_MIN);
        let above = self
            .b
            .ins()
            .icmp_imm_s(IntCC::SignedGreaterThan, value, INDEX_MAX);
        let bad = self.b.ins().bor(below, above);
        self.object_check(bad, Status::IndexOverflow);
    }

    pub(super) fn publish_object(&mut self) {
        if let Some(data) = self.deferred_object.take() {
            self.cpu_store(offset_of!(Processor, object), data);
        }
    }
}
