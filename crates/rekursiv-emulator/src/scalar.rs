//! Native execution of control words whose stack and fetch units are idle.
//!
//! Decode checks static constraints once. The executor refreshes cached words
//! after image edits, before execution resumes. Dynamic checks and old-state operand
//! reads precede every external operation; commit remains infallible. The full
//! processor model handles all other words and remains the test oracle.
use rekursiv_asm::{processor::*, Command, Word};
use rekursiv_model::processor::{Image, Processor, CODE_WORDS, STACK_WORDS};

pub(crate) struct ScalarInstruction {
    pub(super) instruction: Instruction,
    pub(super) arithmetic: bool,
    pub(super) bus: bool,
}

impl ScalarInstruction {
    pub(crate) fn decode(i: Instruction) -> Option<Self> {
        if i.recovery != Recovery::None
            || i.device != Device::None
            || i.write_root
            || i.bus == Bus::Root
            || i.sp != Pointer::Hold
            || i.esp != Address::Hold
            || i.csp != Pointer::Hold
            || i.estk != Estk::Hold
            || i.cstk != Cstk::Hold
            || i.load_ap
            || i.apc != Apc::Hold
            || i.fetch != Fetch::Hold
            || i.alu == Alu::Float
            || i.seq == Seq::Hold
            || i.validate().is_err()
        {
            None
        } else {
            let arithmetic = i.write_register
                || i.load_q
                || i.flags
                || matches!(i.alu, Alu::MultiplySigned | Alu::MultiplyUnsigned);
            let bus = i.symbol
                || i.object.is_some()
                || i.condition == Condition::Symbol
                || matches!(
                    i.seq,
                    Seq::Relative | Seq::ConditionalRelative | Seq::Bus | Seq::ConditionalBus
                )
                || (arithmetic && (i.r == Source::Bus || i.s == Source::Bus));
            Some(Self {
                instruction: i,
                arithmetic,
                bus,
            })
        }
    }

    pub(crate) fn prepare(
        &self,
        cpu: &Processor,
        irq: bool,
    ) -> Result<(ScalarWrites, Option<Command>), u8> {
        let i = self.instruction;
        let d = if self.bus { cpu.bus(i) } else { 0 };
        let mut product = cpu.product;
        // ALU results are unobservable without destinations, except for the
        // multiply latch (floating-point operations use the general path).
        let (f, carry, overflow) = if self.arithmetic {
            let source = |s: Source, reg: u8| match s {
                Source::Register => cpu.rf[reg as usize],
                Source::Bus => d as u32,
                Source::Estk => cpu.estkr as u32,
                Source::Q => cpu.q,
                Source::Branch => i.branch as i16 as i32 as u32,
            };
            let r = source(i.r, i.ra);
            let s = source(i.s, i.rb);
            let cin = match i.carry {
                Carry::Zero => 0,
                Carry::One => 1,
                Carry::ZeroFlag => (cpu.flags & 1) as u64,
            };
            match i.alu {
                Alu::Add | Alu::Sub | Alu::SubReverse => {
                    let (a, b, subtract) = match i.alu {
                        Alu::Add => (r, s, false),
                        Alu::Sub => (r, s, true),
                        _ => (s, r, true),
                    };
                    let wide = a as u64 + if subtract { (!b) as u64 } else { b as u64 } + cin;
                    let signed = if subtract {
                        (a as i32 as i64) - (b as i32 as i64) - 1 + cin as i64
                    } else {
                        (a as i32 as i64) + (b as i32 as i64) + cin as i64
                    };
                    (
                        wide as u32,
                        wide > u32::MAX as u64,
                        !(i32::MIN as i64..=i32::MAX as i64).contains(&signed),
                    )
                }
                op => {
                    let value = match op {
                        Alu::Pass => r,
                        Alu::And => r & s,
                        Alu::Or => r | s,
                        Alu::Xor => r ^ s,
                        Alu::Not => !r,
                        Alu::Rotate => r.rotate_left(s & 31),
                        Alu::MultiplySigned => {
                            product = ((r as i32 as i64) * (s as i32 as i64)) as u64;
                            product as u32
                        }
                        Alu::MultiplyUnsigned => {
                            product = r as u64 * s as u64;
                            product as u32
                        }
                        Alu::ProductHigh => (cpu.product >> 32) as u32,
                        Alu::ProductLow => cpu.product as u32,
                        Alu::FloatStatus => cpu.fp_flags as u32,
                        _ => unreachable!("decode excludes floating-point arithmetic"),
                    };
                    (value, false, false)
                }
            }
        } else {
            (0, false, false)
        };
        let y = match i.shift {
            Shift::None => f,
            Shift::Left => f << 1,
            Shift::Right => f >> 1,
            Shift::ArithmeticRight => ((f as i32) >> 1) as u32,
        };
        let cc = (match i.condition {
            Condition::Always | Condition::ObjectOk => true,
            Condition::Zero => cpu.flags & 1 != 0,
            Condition::Sign => cpu.flags & 2 != 0,
            Condition::Carry => cpu.flags & 4 != 0,
            Condition::Overflow => cpu.flags & 8 != 0,
            Condition::CorrectedSign => cpu.flags & 16 != 0,
            Condition::Symbol => cpu.symbol == d,
            Condition::Last => cpu.lastcc,
            Condition::ControlZero => cpu.cstkr == 0,
            Condition::Interrupt => irq,
        }) ^ i.invert;
        let sequential = cpu.pc as i64 + 1;
        let taken = match i.seq {
            Seq::Jump | Seq::Relative | Seq::Bus | Seq::Return | Seq::Dispatch | Seq::TwoWay => {
                true
            }
            Seq::ConditionalJump
            | Seq::ConditionalRelative
            | Seq::ConditionalBus
            | Seq::ConditionalReturn
            | Seq::ConditionalDispatch
            | Seq::ConditionalMark
            | Seq::SavedReturn => cc,
            _ => false,
        };
        let mut target = if taken {
            match i.seq {
                Seq::Jump | Seq::ConditionalJump => i.branch as i64,
                Seq::Relative | Seq::ConditionalRelative => {
                    cpu.pc as i64 + (((d << 24) as i64) >> 24)
                }
                Seq::Bus | Seq::ConditionalBus => d as i64,
                Seq::Return | Seq::ConditionalReturn => cpu.cstkr as i64,
                Seq::Dispatch | Seq::ConditionalDispatch => cpu.ucar as i64,
                Seq::ConditionalMark => cpu.mark as i64,
                Seq::TwoWay => {
                    if cc {
                        i.branch as i64
                    } else {
                        cpu.mark as i64
                    }
                }
                Seq::SavedReturn => cpu.upcor as i64,
                _ => unreachable!(),
            }
        } else {
            sequential
        };
        if i.halt {
            target = cpu.pc as i64;
        }
        if !(0..CODE_WORDS as i64).contains(&target) {
            return Err(2);
        }
        // Hold controls still reject invalid existing pointer state, in the
        // same order as the general processor. Nothing has been published yet.
        if cpu.sp >= STACK_WORDS as u32
            || cpu.csp >= STACK_WORDS as u32
            || cpu.esp >= STACK_WORDS as u32
        {
            return Err(3);
        }
        if cpu.apc > 0xffffff {
            return Err(5);
        }
        if i.allocation_dynamic && cpu.rf[i.ra as usize] >= 1 << 24 {
            return Err(1);
        }
        let command = i.object.map(|mut c| {
            if i.allocation_dynamic {
                c.alloc_size = cpu.rf[i.ra as usize];
            }
            c.data = Word::from_bits(d).unwrap();
            c
        });
        let sign = f >> 31 != 0;
        let flags = (f == 0) as u8
            | ((sign as u8) << 1)
            | ((carry as u8) << 2)
            | ((overflow as u8) << 3)
            | (((sign ^ overflow) as u8) << 4);
        Ok((
            ScalarWrites {
                pc: target as u16,
                upcor: if taken && i.seq != Seq::SavedReturn {
                    sequential as u16
                } else {
                    cpu.upcor
                },
                d,
                y,
                flags,
                product,
                cc,
                object: cpu.object,
            },
            command,
        ))
    }
}

// Generated code uses offset_of! from this build, so this record can retain
// Rust's compact field layout. It is passed by pointer, never by value over C.
pub(crate) struct ScalarWrites {
    pub(super) pc: u16,
    pub(super) upcor: u16,
    pub(super) d: u64,
    pub(super) y: u32,
    pub(super) flags: u8,
    pub(super) product: u64,
    pub(super) cc: bool,
    pub(crate) object: u64,
}
impl ScalarWrites {
    #[inline(always)]
    pub(crate) fn commit(self, cpu: &mut Processor, image: &Image, i: Instruction) {
        if cpu.roots.is_none() {
            cpu.roots = Some(image.roots);
        }
        if i.mark {
            cpu.mark = cpu.pc;
        }
        cpu.pc = self.pc;
        cpu.upcor = self.upcor;
        cpu.lastcc = self.cc;
        cpu.object = self.object;
        cpu.product = self.product;
        if i.symbol {
            cpu.symbol = self.d;
        }
        if i.write_register {
            cpu.rf[i.rb as usize] = self.y;
        }
        if i.load_q {
            cpu.q = self.y;
        }
        if i.flags {
            cpu.flags = self.flags;
        }
        cpu.halted = i.halt;
        cpu.service = i.seq == Seq::Service && self.cc && !i.halt;
        if cpu.service {
            cpu.service_code = (i.branch & 15) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_retirements_and_faults_match_the_general_processor() {
        // Fixed seed: varied simultaneous destinations, old-state branches,
        // signed arithmetic, and invalid dynamic state remain reproducible.
        let mut seed = 0x37465524723u64;
        let mut random = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let buses = [
            Bus::Immediate,
            Bus::Estk,
            Bus::Cstk,
            Bus::Object,
            Bus::Register,
            Bus::Apc,
            Bus::Ap,
            Bus::Sp,
            Bus::Namarg,
            Bus::Upcor,
            Bus::Q,
            Bus::Symbol,
            Bus::SymbolHigh,
            Bus::Device,
        ];
        let sequences = [
            Seq::Continue,
            Seq::Jump,
            Seq::ConditionalJump,
            Seq::Relative,
            Seq::ConditionalRelative,
            Seq::Bus,
            Seq::ConditionalBus,
            Seq::Return,
            Seq::ConditionalReturn,
            Seq::Dispatch,
            Seq::ConditionalDispatch,
            Seq::ConditionalMark,
            Seq::TwoWay,
            Seq::SavedReturn,
            Seq::Service,
        ];
        let conditions = [
            Condition::Always,
            Condition::Zero,
            Condition::Sign,
            Condition::Carry,
            Condition::Overflow,
            Condition::CorrectedSign,
            Condition::Symbol,
            Condition::Last,
            Condition::ControlZero,
            Condition::ObjectOk,
            Condition::Interrupt,
        ];
        let alus = [
            Alu::Pass,
            Alu::Add,
            Alu::Sub,
            Alu::SubReverse,
            Alu::And,
            Alu::Or,
            Alu::Xor,
            Alu::Not,
            Alu::Rotate,
            Alu::MultiplySigned,
            Alu::MultiplyUnsigned,
            Alu::ProductHigh,
            Alu::ProductLow,
            Alu::FloatStatus,
        ];
        let sources = [
            Source::Register,
            Source::Bus,
            Source::Estk,
            Source::Q,
            Source::Branch,
        ];
        let carries = [Carry::Zero, Carry::One, Carry::ZeroFlag];
        let shifts = [
            Shift::None,
            Shift::Left,
            Shift::Right,
            Shift::ArithmeticRight,
        ];
        let values = [
            0,
            1,
            u32::MAX,
            i32::MAX as u32,
            0x80000000,
            31,
            32,
            8191,
            0xffffff,
            0x1000000,
        ];
        let mut image = Image::default();
        let mut successes = 0;
        let mut faults = [0; 7];
        for case in 0..30_000 {
            let bits = random();
            let i = Instruction {
                data: Word::from_bits(random() & ((1 << 40) - 1)).unwrap(),
                bus: buses[case % buses.len()],
                seq: sequences[(case / 3) % sequences.len()],
                condition: conditions[(case / 7) % conditions.len()],
                invert: bits & 1 != 0,
                halt: bits & 127 == 0,
                mark: bits & 2 != 0,
                symbol: bits & 4 != 0,
                branch: (random() % 8194) as u16,
                ra: (random() % 16) as u8,
                rb: (random() % 16) as u8,
                alu: alus[(case / 11) % alus.len()],
                r: sources[(case / 13) % sources.len()],
                s: sources[(case / 17) % sources.len()],
                carry: carries[(case / 19) % carries.len()],
                shift: shifts[(case / 23) % shifts.len()],
                write_register: bits & 8 != 0,
                load_q: bits & 16 != 0,
                flags: bits & 32 != 0,
                allocation_dynamic: bits & 64 != 0,
                object: (case % 3 == 0).then_some(Command::default()),
                ..Instruction::default()
            };
            let mut cpu = Processor {
                pc: (random() % 8192) as u16,
                sp: (random() % 33) as u32,
                csp: (random() % 33) as u32,
                esp: (random() % 33) as u32,
                apc: if case % 11 == 0 {
                    0x1000000
                } else {
                    random() as u32 & 0xffffff
                },
                upcor: (random() % 8194) as u16,
                mark: (random() % 8194) as u16,
                ucar: (random() % 8194) as u16,
                estkr: random() & ((1 << 40) - 1),
                cstkr: (random() % 8194) as u32,
                object: random() & ((1 << 40) - 1),
                symbol: random() & ((1 << 40) - 1),
                q: random() as u32,
                product: random(),
                flags: (random() & 31) as u8,
                fp_flags: (random() & 31) as u8,
                lastcc: random() & 1 != 0,
                service_code: 9,
                ..Processor::default()
            };
            for r in &mut cpu.rf {
                *r = values[random() as usize % values.len()];
            }
            if case % 2 == 0 {
                cpu.roots = Some([Word::signed(17); 32]);
            }
            image.code[cpu.pc as usize] = Some(i);
            let irq = bits & 256 != 0;
            let expected = cpu.prepare_writes(&image, irq);
            let actual = ScalarInstruction::decode(i).unwrap().prepare(&cpu, irq);
            match (expected, actual) {
                (Ok((mut expected, command)), Ok((mut actual, fast_command))) => {
                    assert_eq!(command, fast_command, "case {case}");
                    // External replies must substitute identically without
                    // changing the original operands used by local writes.
                    if command.is_some() {
                        expected.object = 123;
                        actual.object = 123;
                    }
                    let mut oracle = cpu.clone();
                    expected.commit(&mut oracle, &image);
                    actual.commit(&mut cpu, &image, i);
                    assert_eq!(cpu, oracle, "case {case}: {i:?}");
                    successes += 1;
                }
                (Err(expected), Err(actual)) => {
                    assert_eq!(actual, expected, "case {case}: {i:?}");
                    faults[actual as usize] += 1;
                }
                _ => panic!("preparation mismatch at case {case}: {i:?}"),
            }
        }
        assert!(successes > 5000);
        for code in [1, 2, 3, 5] {
            assert!(faults[code] > 0, "missing fault {code}");
        }
    }
}
