//! Stack and fetch datapaths. Values remain in SSA until all checks and the
//! peripheral tick succeed. Array accesses always use the pre-instruction
//! address, while ESP=SP forwards the newly computed stack pointer.
use super::{
    emit::Emit,
    stack_fetch::{expanded, StackFetchWrites},
};
use cranelift_codegen::ir::{
    condcodes::IntCC, types::*, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, Value,
};
use rekursiv_asm::processor::*;
use rekursiv_model::processor::{Processor, STACK_WORDS};
use std::mem::offset_of;

pub(super) struct StackFetchValues {
    sp: Value,
    csp: Value,
    esp: Value,
    ap: Value,
    apc: Value,
    estkr: Value,
    cstkr: Value,
    opcode: Value,
    namarg: Value,
    ucar: Value,
    estk_index: Value,
    cstk_index: Value,
}
impl Emit<'_> {
    fn stack_bounds(&mut self, value: Value) {
        let bad =
            self.b
                .ins()
                .icmp_imm_s(IntCC::UnsignedGreaterThanOrEqual, value, STACK_WORDS as i64);
        self.reject(bad, 3);
    }
    fn pointer(&mut self, offset: usize, control: Pointer, d: Value) -> Value {
        let old = self.load(I32, offset);
        let old = self.wide(old);
        let value = match control {
            Pointer::Hold => old,
            Pointer::Bus => d,
            Pointer::Increment => self.b.ins().iadd_imm_s(old, 1),
            Pointer::Decrement => self.b.ins().iadd_imm_s(old, -1),
        };
        self.stack_bounds(value);
        self.b.ins().ireduce(I32, value)
    }
    fn stack_address(&mut self, index: Value, offset: usize, shift: i64) -> Value {
        let index = if self.pointer == I32 {
            index
        } else {
            self.b.ins().uextend(self.pointer, index)
        };
        let bytes = self.b.ins().ishl_imm_s(index, shift);
        let base = self.b.ins().iadd_imm_s(self.cpu, offset as i64);
        self.b.ins().iadd(base, bytes)
    }
    pub(super) fn stack_fetch(
        &mut self,
        i: Instruction,
        d: Value,
        y: Value,
    ) -> Option<StackFetchValues> {
        let sp = self.pointer(offset_of!(Processor, sp), i.sp, d);
        let csp = self.pointer(offset_of!(Processor, csp), i.csp, d);
        let old_esp = self.load(I32, offset_of!(Processor, esp));
        let old_csp = self.load(I32, offset_of!(Processor, csp));
        let old_ap = self.load(I32, offset_of!(Processor, ap));
        let esp = match i.esp {
            Address::Hold => self.wide(old_esp),
            Address::Bus => d,
            Address::Sp => self.wide(sp),
            Address::Argument => {
                let ap = self.wide(old_ap);
                self.b.ins().iadd_imm_s(ap, i.branch as i64)
            }
        };
        self.stack_bounds(esp);
        let esp = self.b.ins().ireduce(I32, esp);
        // Public Processor fields can contain invalid old state. A pointer
        // replacement cannot make an OLD out-of-bounds array access safe.
        if i.estk != Estk::Hold {
            self.stack_bounds(old_esp);
        }
        if i.cstk != Cstk::Hold {
            self.stack_bounds(old_csp);
        }
        let ap = if i.load_ap {
            self.stack_bounds(d);
            self.b.ins().ireduce(I32, d)
        } else {
            old_ap
        };
        let old_apc = self.load(I32, offset_of!(Processor, apc));
        let cd = match i.cstk {
            Cstk::Bus => d,
            Cstk::Upcor => {
                let v = self.load(I16, offset_of!(Processor, upcor));
                self.wide(v)
            }
            Cstk::Apc => self.wide(old_apc),
            Cstk::Ap => self.wide(old_ap),
            Cstk::Sp => {
                let v = self.load(I32, offset_of!(Processor, sp));
                self.wide(v)
            }
            Cstk::Increment => {
                let v = self.load(I32, offset_of!(Processor, cstkr));
                let v = self.wide(v);
                self.b.ins().iadd_imm_s(v, 1)
            }
            Cstk::Decrement => {
                let v = self.load(I32, offset_of!(Processor, cstkr));
                let v = self.b.ins().iadd_imm_s(v, -1);
                self.wide(v)
            }
            _ => self.b.ins().iconst(I64, 0),
        };
        let bad_cd = self
            .b
            .ins()
            .icmp_imm_s(IntCC::UnsignedGreaterThan, cd, 0xffffff);
        self.reject(bad_cd, 3);
        let apc = self.wide(old_apc);
        let apc = match i.apc {
            Apc::Hold => apc,
            Apc::Bus => d,
            Apc::Increment => self.b.ins().iadd_imm_s(apc, 1),
            Apc::Step => self.b.ins().iadd(apc, d),
        };
        let bad_apc = self
            .b
            .ins()
            .icmp_imm_s(IntCC::UnsignedGreaterThan, apc, 0xffffff);
        self.reject(bad_apc, 5);
        let apc = self.b.ins().ireduce(I32, apc);
        if !expanded(i) {
            return None;
        }
        let old_opcode = self.load(self.pointer, offset_of!(Processor, opcode));
        let (opcode, namarg) = if matches!(i.fetch, Fetch::Nam | Fetch::Both) {
            let slot = self.b.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let output = self.b.ins().stack_addr(self.pointer, slot, 0);
            let call = self.b.ins().call(self.nam, &[self.image, old_apc, output]);
            let failed = self.b.inst_results(call)[0];
            self.reject(failed, 5);
            let word = self.b.ins().stack_load(self.pointer, I64, slot, 0);
            let opcode = self.b.ins().ushr_imm_s(word, 30);
            let opcode = if self.pointer == I32 {
                self.b.ins().ireduce(I32, opcode)
            } else {
                opcode
            };
            let arg = self.b.ins().ireduce(I32, word);
            (opcode, self.b.ins().band_imm_s(arg, 0x3fffffff))
        } else {
            (old_opcode, self.load(I32, offset_of!(Processor, namarg)))
        };
        let ucar = if matches!(i.fetch, Fetch::Map | Fetch::Both) {
            // Both reads the old opcode. Its NAM result is visible next time.
            let call = self.b.ins().call(self.map, &[self.image, old_opcode]);
            let value = self.b.inst_results(call)[0];
            let bad = self
                .b
                .ins()
                .icmp_imm_s(IntCC::UnsignedGreaterThan, value, 0xffff);
            self.reject(bad, 5);
            self.b.ins().ireduce(I16, value)
        } else {
            self.load(I16, offset_of!(Processor, ucar))
        };
        let estkr = match i.estk {
            Estk::Hold => self.load(I64, offset_of!(Processor, estkr)),
            Estk::Read => {
                let address = self.stack_address(old_esp, offset_of!(Processor, estk), 3);
                self.b.ins().load(I64, MemFlagsData::trusted(), address, 0)
            }
            Estk::Bus => d,
            Estk::Alu => self.wide(y),
            Estk::Wide => {
                let high = self.b.ins().band_imm_s(d, 255);
                let high = self.b.ins().ishl_imm_s(high, 32);
                let low = self.wide(y);
                self.b.ins().bor(high, low)
            }
            Estk::Compact => {
                if i.compact_code < 2 {
                    let bad = self.b.ins().icmp_imm_s(
                        IntCC::UnsignedGreaterThan,
                        y,
                        i.compact_code as i64,
                    );
                    self.reject(bad, 1);
                }
                let payload = self.wide(y);
                self.b
                    .ins()
                    .bor_imm_s(payload, (3i64 << 38) | ((i.compact_code as i64) << 32))
            }
        };
        let cstkr = match i.cstk {
            Cstk::Hold => self.load(I32, offset_of!(Processor, cstkr)),
            Cstk::Read => {
                let address = self.stack_address(old_csp, offset_of!(Processor, cstk), 2);
                self.b.ins().load(I32, MemFlagsData::trusted(), address, 0)
            }
            _ => self.b.ins().ireduce(I32, cd),
        };
        Some(StackFetchValues {
            sp,
            csp,
            esp,
            ap,
            apc,
            estkr,
            cstkr,
            opcode,
            namarg,
            ucar,
            estk_index: old_esp,
            cstk_index: old_csp,
        })
    }
    pub(super) fn publish_stack_fetch(
        &mut self,
        i: Instruction,
        values: StackFetchValues,
        block: bool,
    ) {
        macro_rules! publish {
            ($field:ident, $changed:expr) => {
                if block {
                    if $changed {
                        self.cpu_store(offset_of!(Processor, $field), values.$field);
                    }
                } else {
                    self.b.ins().store(
                        MemFlagsData::trusted(),
                        values.$field,
                        self.out,
                        offset_of!(StackFetchWrites, $field) as i32,
                    );
                }
            };
        }
        publish!(sp, i.sp != Pointer::Hold);
        publish!(csp, i.csp != Pointer::Hold);
        publish!(esp, i.esp != Address::Hold);
        publish!(ap, i.load_ap);
        publish!(apc, i.apc != Apc::Hold);
        publish!(estkr, i.estk != Estk::Hold);
        publish!(cstkr, i.cstk != Cstk::Hold);
        publish!(opcode, matches!(i.fetch, Fetch::Nam | Fetch::Both));
        publish!(namarg, matches!(i.fetch, Fetch::Nam | Fetch::Both));
        publish!(ucar, matches!(i.fetch, Fetch::Map | Fetch::Both));
        if block {
            if !matches!(i.estk, Estk::Hold | Estk::Read) {
                let address = self.stack_address(values.estk_index, offset_of!(Processor, estk), 3);
                self.b
                    .ins()
                    .store(MemFlagsData::trusted(), values.estkr, address, 0);
            }
            if !matches!(i.cstk, Cstk::Hold | Cstk::Read) {
                let address = self.stack_address(values.cstk_index, offset_of!(Processor, cstk), 2);
                self.b
                    .ins()
                    .store(MemFlagsData::trusted(), values.cstkr, address, 0);
            }
        } else {
            for (offset, value) in [
                (offset_of!(StackFetchWrites, estk_index), values.estk_index),
                (offset_of!(StackFetchWrites, cstk_index), values.cstk_index),
            ] {
                self.b
                    .ins()
                    .store(MemFlagsData::trusted(), value, self.out, offset as i32);
            }
        }
    }
}
