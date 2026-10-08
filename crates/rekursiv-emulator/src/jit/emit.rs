//! Lower scalar datapath operations to Cranelift SSA values. Operands and
//! validation precede every publication, preserving simultaneous-write rules.
use crate::scalar::{ScalarInstruction, ScalarWrites};
use cranelift_codegen::ir::{condcodes::IntCC, types::*, InstBuilder, MemFlagsData, Type, Value};
use cranelift_frontend::FunctionBuilder;
use rekursiv_asm::processor::*;
use rekursiv_model::processor::{Processor, CODE_WORDS};
use std::mem::offset_of;

#[derive(Clone, Copy)]
pub(super) enum Mode {
    Prepare,
    Block {
        tick: cranelift_codegen::ir::FuncRef,
        device: Value,
        error: Value,
        retired: u64,
        object: cranelift_codegen::ir::FuncRef,
        model: Value,
        command: usize,
        object_commands: u64,
    },
}
pub(super) struct Emit<'a> {
    pub(super) b: FunctionBuilder<'a>,
    pub(super) cpu: Value,
    pub(super) out: Value,
    pub(super) irq: Value,
    pub(super) mode: Mode,
    pub(super) image: Value,
    pub(super) pointer: Type,
    pub(super) nam: cranelift_codegen::ir::FuncRef,
    pub(super) map: cranelift_codegen::ir::FuncRef,
    pub(super) deferred_object: Option<Value>,
}
macro_rules! load {
    ($e:expr, $ty:expr, $field:ident) => {
        $e.load($ty, offset_of!(Processor, $field))
    };
}
macro_rules! store {
    ($e:expr, $field:ident, $value:expr) => {
        $e.b.ins().store(
            MemFlagsData::trusted(),
            $value,
            $e.out,
            offset_of!(ScalarWrites, $field) as i32,
        )
    };
}
impl Emit<'_> {
    pub(super) fn cpu_store(&mut self, offset: usize, value: Value) {
        self.b
            .ins()
            .store(MemFlagsData::trusted(), value, self.cpu, offset as i32);
    }
    pub(super) fn load(&mut self, ty: Type, offset: usize) -> Value {
        self.b
            .ins()
            .load(ty, MemFlagsData::trusted(), self.cpu, offset as i32)
    }
    pub(super) fn wide(&mut self, value: Value) -> Value {
        self.b.ins().uextend(I64, value)
    }
    fn rf(&mut self, index: u8) -> Value {
        self.load(I32, offset_of!(Processor, rf) + index as usize * 4)
    }
    fn bus(&mut self, i: Instruction) -> Value {
        let narrow = match i.bus {
            Bus::Immediate => return self.b.ins().iconst(I64, i.data.bits() as i64),
            Bus::Estk => return load!(self, I64, estkr),
            Bus::Object => return load!(self, I64, object),
            Bus::Symbol => return load!(self, I64, symbol),
            Bus::SymbolHigh => {
                let v = load!(self, I64, symbol);
                return self.b.ins().ushr_imm_s(v, 32);
            }
            Bus::Cstk => load!(self, I32, cstkr),
            Bus::Register => self.rf(i.ra),
            Bus::Apc => load!(self, I32, apc),
            Bus::Ap => load!(self, I32, ap),
            Bus::Sp => load!(self, I32, sp),
            Bus::Namarg => load!(self, I32, namarg),
            Bus::Upcor => load!(self, I16, upcor),
            Bus::Q => load!(self, I32, q),
            Bus::Device => load!(self, I32, device),
            Bus::Root => unreachable!("decode excludes root reads"),
        };
        self.wide(narrow)
    }
    fn source(&mut self, source: Source, reg: u8, d: Value, i: Instruction) -> Value {
        match source {
            Source::Register => self.rf(reg),
            Source::Bus => self.b.ins().ireduce(I32, d),
            Source::Estk => {
                let v = load!(self, I64, estkr);
                self.b.ins().ireduce(I32, v)
            }
            Source::Q => load!(self, I32, q),
            Source::Branch => self.b.ins().iconst(I32, i.branch as i16 as i64),
        }
    }
    pub(super) fn reject(&mut self, condition: Value, code: i64) {
        let failed = self.b.create_block();
        let good = self.b.create_block();
        self.b.ins().brif(condition, failed, &[], good, &[]);
        self.b.switch_to_block(failed);
        let value = match self.mode {
            Mode::Prepare => self.b.ins().iconst(I8, code),
            Mode::Block {
                retired,
                object_commands,
                ..
            } => self.b.ins().iconst(
                I64,
                ((retired << 8) | code as u64 | (object_commands << 56)) as i64,
            ),
        };
        self.b.ins().return_(&[value]);
        self.b.switch_to_block(good);
    }
    pub(super) fn instruction(&mut self, decoded: &ScalarInstruction) {
        let i = decoded.instruction;
        let d = if decoded.bus {
            self.bus(i)
        } else {
            self.b.ins().iconst(I64, 0)
        };
        let mut product = load!(self, I64, product);
        let false_ = self.b.ins().iconst(I8, 0);
        let mut carry = false_;
        let mut overflow = false_;
        let f = if decoded.arithmetic {
            let r = self.source(i.r, i.ra, d, i);
            let s = self.source(i.s, i.rb, d, i);
            match i.alu {
                Alu::Add | Alu::Sub | Alu::SubReverse => {
                    let cin = match i.carry {
                        Carry::Zero => self.b.ins().iconst(I64, 0),
                        Carry::One => self.b.ins().iconst(I64, 1),
                        Carry::ZeroFlag => {
                            let flags = load!(self, I8, flags);
                            let c = self.b.ins().band_imm_s(flags, 1);
                            self.wide(c)
                        }
                    };
                    let (a, b) = if i.alu == Alu::SubReverse {
                        (s, r)
                    } else {
                        (r, s)
                    };
                    let subtract = i.alu != Alu::Add;
                    let au = self.wide(a);
                    let bu = if subtract {
                        let not = self.b.ins().bnot(b);
                        self.wide(not)
                    } else {
                        self.wide(b)
                    };
                    let sum = self.b.ins().iadd(au, bu);
                    let wide = self.b.ins().iadd(sum, cin);
                    carry =
                        self.b
                            .ins()
                            .icmp_imm_s(IntCC::UnsignedGreaterThan, wide, u32::MAX as i64);
                    let sa = self.b.ins().sextend(I64, a);
                    let sb = self.b.ins().sextend(I64, b);
                    let signed = if subtract {
                        let diff = self.b.ins().isub(sa, sb);
                        self.b.ins().iadd_imm_s(diff, -1)
                    } else {
                        self.b.ins().iadd(sa, sb)
                    };
                    let signed = self.b.ins().iadd(signed, cin);
                    let low =
                        self.b
                            .ins()
                            .icmp_imm_s(IntCC::SignedLessThan, signed, i32::MIN as i64);
                    let high =
                        self.b
                            .ins()
                            .icmp_imm_s(IntCC::SignedGreaterThan, signed, i32::MAX as i64);
                    overflow = self.b.ins().bor(low, high);
                    self.b.ins().ireduce(I32, wide)
                }
                Alu::Pass => r,
                Alu::And => self.b.ins().band(r, s),
                Alu::Or => self.b.ins().bor(r, s),
                Alu::Xor => self.b.ins().bxor(r, s),
                Alu::Not => self.b.ins().bnot(r),
                Alu::Rotate => self.b.ins().rotl(r, s),
                Alu::MultiplySigned | Alu::MultiplyUnsigned => {
                    let (r, s) = if i.alu == Alu::MultiplySigned {
                        (self.b.ins().sextend(I64, r), self.b.ins().sextend(I64, s))
                    } else {
                        (self.wide(r), self.wide(s))
                    };
                    product = self.b.ins().imul(r, s);
                    self.b.ins().ireduce(I32, product)
                }
                Alu::ProductHigh => {
                    let high = self.b.ins().ushr_imm_s(product, 32);
                    self.b.ins().ireduce(I32, high)
                }
                Alu::ProductLow => self.b.ins().ireduce(I32, product),
                Alu::FloatStatus => {
                    let v = load!(self, I8, fp_flags);
                    self.b.ins().uextend(I32, v)
                }
                Alu::Float => unreachable!("decode excludes float operations"),
            }
        } else {
            self.b.ins().iconst(I32, 0)
        };
        let y = match i.shift {
            Shift::None => f,
            Shift::Left => self.b.ins().ishl_imm_s(f, 1),
            Shift::Right => self.b.ins().ushr_imm_s(f, 1),
            Shift::ArithmeticRight => self.b.ins().sshr_imm_s(f, 1),
        };
        // Branch conditions always use OLD flags/symbol, including words that
        // replace those values. All stores below target only the write set.
        let mut cc = match i.condition {
            Condition::Always | Condition::ObjectOk => self.b.ins().iconst(I8, 1),
            Condition::Symbol => {
                let symbol = load!(self, I64, symbol);
                self.b.ins().icmp(IntCC::Equal, symbol, d)
            }
            Condition::Last => load!(self, I8, lastcc),
            Condition::ControlZero => {
                let v = load!(self, I32, cstkr);
                self.b.ins().icmp_imm_s(IntCC::Equal, v, 0)
            }
            Condition::Interrupt => self.irq,
            other => {
                let mask = match other {
                    Condition::Zero => 1,
                    Condition::Sign => 2,
                    Condition::Carry => 4,
                    Condition::Overflow => 8,
                    Condition::CorrectedSign => 16,
                    _ => unreachable!(),
                };
                let flags = load!(self, I8, flags);
                let masked = self.b.ins().band_imm_s(flags, mask);
                self.b.ins().icmp_imm_s(IntCC::NotEqual, masked, 0)
            }
        };
        if i.invert {
            cc = self.b.ins().bxor_imm_s(cc, 1);
        }
        let pc = load!(self, I16, pc);
        let pc = self.wide(pc);
        let sequential = self.b.ins().iadd_imm_s(pc, 1);
        let taken = match i.seq {
            Seq::Jump | Seq::Relative | Seq::Bus | Seq::Return | Seq::Dispatch | Seq::TwoWay => {
                self.b.ins().iconst(I8, 1)
            }
            Seq::ConditionalJump
            | Seq::ConditionalRelative
            | Seq::ConditionalBus
            | Seq::ConditionalReturn
            | Seq::ConditionalDispatch
            | Seq::ConditionalMark
            | Seq::SavedReturn => cc,
            _ => false_,
        };
        let destination = match i.seq {
            Seq::Jump | Seq::ConditionalJump => self.b.ins().iconst(I64, i.branch as i64),
            Seq::Relative | Seq::ConditionalRelative => {
                let shifted = self.b.ins().ishl_imm_s(d, 24);
                let signed = self.b.ins().sshr_imm_s(shifted, 24);
                self.b.ins().iadd(pc, signed)
            }
            Seq::Bus | Seq::ConditionalBus => d,
            Seq::Return | Seq::ConditionalReturn => {
                let v = load!(self, I32, cstkr);
                self.wide(v)
            }
            Seq::Dispatch | Seq::ConditionalDispatch => {
                let v = load!(self, I16, ucar);
                self.wide(v)
            }
            Seq::ConditionalMark => {
                let v = load!(self, I16, mark);
                self.wide(v)
            }
            Seq::SavedReturn => {
                let v = load!(self, I16, upcor);
                self.wide(v)
            }
            Seq::TwoWay => {
                let yes = self.b.ins().iconst(I64, i.branch as i64);
                let no = load!(self, I16, mark);
                let no = self.wide(no);
                self.b.ins().select(cc, yes, no)
            }
            _ => sequential,
        };
        let target = if i.halt {
            pc
        } else {
            self.b.ins().select(taken, destination, sequential)
        };
        // Unsigned comparison also rejects negative relative destinations.
        let bad_pc =
            self.b
                .ins()
                .icmp_imm_s(IntCC::UnsignedGreaterThanOrEqual, target, CODE_WORDS as i64);
        self.reject(bad_pc, 2);
        let stack_fetch = self.stack_fetch(i, d, y);
        if i.allocation_dynamic {
            let size = self.rf(i.ra);
            let bad = self
                .b
                .ins()
                .icmp_imm_s(IntCC::UnsignedGreaterThanOrEqual, size, 1 << 24);
            self.reject(bad, 1);
        }
        let zero = self.b.ins().icmp_imm_s(IntCC::Equal, f, 0);
        let sign = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, f, 0);
        let corrected = self.b.ins().bxor(sign, overflow);
        let mut flags = zero;
        for (value, shift) in [(sign, 1), (carry, 2), (overflow, 3), (corrected, 4)] {
            let bit = self.b.ins().ishl_imm_s(value, shift);
            flags = self.b.ins().bor(flags, bit);
        }
        let next_pc = self.b.ins().ireduce(I16, target);
        let old_upcor = load!(self, I16, upcor);
        let sequential16 = self.b.ins().ireduce(I16, sequential);
        let upcor = if i.seq == Seq::SavedReturn {
            old_upcor
        } else {
            self.b.ins().select(taken, sequential16, old_upcor)
        };
        if let Mode::Block {
            tick,
            device,
            error,
            ..
        } = self.mode
        {
            if i.object.is_some() {
                self.resident(d);
            }
            let call = self.b.ins().call(tick, &[device, error]);
            let failed = self.b.inst_results(call)[0];
            self.reject(failed, 255);
            self.publish_object();
            // Validation and the device tick succeeded. Publish simultaneous
            // local destinations using the already evaluated OLD operands.
            self.cpu_store(offset_of!(Processor, pc), next_pc);
            self.cpu_store(offset_of!(Processor, upcor), upcor);
            self.cpu_store(offset_of!(Processor, lastcc), cc);
            self.cpu_store(offset_of!(Processor, product), product);
            if i.mark {
                let old_pc = self.b.ins().ireduce(I16, pc);
                self.cpu_store(offset_of!(Processor, mark), old_pc);
            }
            if i.symbol {
                self.cpu_store(offset_of!(Processor, symbol), d);
            }
            if i.write_register {
                self.cpu_store(offset_of!(Processor, rf) + i.rb as usize * 4, y);
            }
            if i.load_q {
                self.cpu_store(offset_of!(Processor, q), y);
            }
            if i.flags {
                self.cpu_store(offset_of!(Processor, flags), flags);
            }
            if let Some(values) = stack_fetch {
                self.publish_stack_fetch(i, values, true);
            }
            return;
        }
        if let Some(values) = stack_fetch {
            self.publish_stack_fetch(i, values, false);
        }
        let object = load!(self, I64, object);
        store!(self, pc, next_pc);
        store!(self, upcor, upcor);
        store!(self, d, d);
        store!(self, y, y);
        store!(self, flags, flags);
        store!(self, product, product);
        store!(self, cc, cc);
        store!(self, object, object);
        self.b.ins().return_(&[false_]);
    }
}
