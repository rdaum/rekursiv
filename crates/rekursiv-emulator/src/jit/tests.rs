use super::*;
use crate::execution::WriteSet;

#[test]
fn compiled_datapath_matches_the_processor_oracle() -> Result<()> {
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
    for case in 0..1024 {
        let bits = random();
        let i = Instruction {
            data: Word::from_bits(random() & ((1 << 40) - 1))?,
            bus: buses[case % buses.len()],
            seq: sequences[case % sequences.len()],
            condition: conditions[case % conditions.len()],
            invert: bits & 1 != 0,
            halt: bits & 127 == 0,
            mark: bits & 2 != 0,
            symbol: bits & 4 != 0,
            branch: (random() % 8194) as u16,
            ra: (random() % 16) as u8,
            rb: (random() % 16) as u8,
            alu: alus[case % alus.len()],
            r: sources[case % sources.len()],
            s: sources[(case / 5) % sources.len()],
            carry: carries[case % carries.len()],
            shift: shifts[case % shifts.len()],
            write_register: bits & 8 != 0,
            load_q: bits & 16 != 0,
            flags: bits & 32 != 0,
            allocation_dynamic: bits & 64 != 0,
            object: (case % 3 == 0).then_some(Command::default()),
            ..Instruction::default()
        };
        image.code[case] = Some(i);
    }
    // Compile once, then vary dynamic state without recompilation. Include
    // valid boundaries, stale pointers, and signed arithmetic extremes.
    let jit = Jit::compile(&image)?;
    let mut successes = 0;
    let mut faults = [0; 7];
    for pc in 0..1024 {
        let i = image.code[pc].unwrap();
        for case in 0..48 {
            let mut cpu = Processor {
                pc: pc as u16,
                sp: if case == 1 { 32 } else { 0 },
                csp: if case == 2 { u32::MAX } else { 31 },
                esp: if case == 3 { 32 } else { 7 },
                apc: if case == 4 {
                    0x1000000
                } else {
                    random() as u32 & 0xffffff
                },
                ap: random() as u32,
                namarg: random() as u32,
                upcor: (random() % 8194) as u16,
                mark: (random() % 8194) as u16,
                ucar: (random() % 8194) as u16,
                estkr: random() & ((1 << 40) - 1),
                cstkr: (random() % 8194) as u32,
                object: random() & ((1 << 40) - 1),
                symbol: random() & ((1 << 40) - 1),
                device: random() as u32,
                q: random() as u32,
                product: random(),
                flags: (random() & 31) as u8,
                fp_flags: (random() & 31) as u8,
                lastcc: random() & 1 != 0,
                service_code: 9,
                roots: (case % 2 == 0).then_some([Word::signed(17); 32]),
                ..Processor::default()
            };
            for r in &mut cpu.rf {
                *r = values[random() as usize % values.len()];
            }
            let irq = case % 2 == 0;
            let expected = cpu.prepare_writes(&image, irq);
            let actual = jit
                .prepare(&cpu, irq, &image)
                .expect("eligible instruction");
            match (expected, actual) {
                (Ok((mut expected, command)), Ok((actual, jit_command))) => {
                    let mut actual = crate::execution::NativeWrites::from(actual);
                    assert_eq!(command, jit_command, "PC {pc}, case {case}: {i:?}");
                    if command.is_some() {
                        expected.object = 123;
                        actual.object(123);
                    }
                    let mut oracle = cpu.clone();
                    expected.commit(&mut oracle, &image);
                    actual.commit(&mut cpu, &image, i);
                    assert_eq!(cpu, oracle, "PC {pc}, case {case}: {i:?}");
                    successes += 1;
                }
                (Err(expected), Err(actual)) => {
                    assert_eq!(actual, expected, "PC {pc}, case {case}: {i:?}");
                    faults[actual as usize] += 1;
                }
                _ => panic!("preparation mismatch at PC {pc}, case {case}: {i:?}"),
            }
        }
    }
    assert!(successes > 10_000);
    for code in [1, 2, 3, 5] {
        assert!(faults[code] > 0, "missing fault {code}");
    }
    Ok(())
}

#[test]
fn stack_fetch_preparations_match_the_oracle_for_valid_and_invalid_state() -> Result<()> {
    let estks = [
        Estk::Hold,
        Estk::Read,
        Estk::Bus,
        Estk::Alu,
        Estk::Compact,
        Estk::Wide,
    ];
    let cstks = [
        Cstk::Hold,
        Cstk::Read,
        Cstk::Bus,
        Cstk::Upcor,
        Cstk::Apc,
        Cstk::Ap,
        Cstk::Sp,
        Cstk::Increment,
        Cstk::Decrement,
    ];
    let pointers = [
        Pointer::Hold,
        Pointer::Bus,
        Pointer::Increment,
        Pointer::Decrement,
    ];
    let addresses = [Address::Hold, Address::Bus, Address::Sp, Address::Argument];
    let apcs = [Apc::Hold, Apc::Bus, Apc::Increment, Apc::Step];
    let fetches = [Fetch::Hold, Fetch::Nam, Fetch::Map, Fetch::Both];
    let mut image = Image::default();
    for n in 0..512 {
        let mut i = Instruction {
            data: Word::raw((n % 34) as u64)?,
            r: Source::Bus,
            s: Source::Q,
            alu: Alu::Add,
            write_register: true,
            load_q: true,
            flags: true,
            symbol: true,
            compact_code: (n % 4) as u8,
            branch: (n % 3) as u16,
            ..Instruction::default()
        };
        // Isolated controls supply good coverage; combined controls check
        // simultaneous old-state reads, forwarding, and validation precedence.
        match n % 10 {
            0 => i.estk = estks[(n / 10) % estks.len()],
            1 => i.cstk = cstks[(n / 10) % cstks.len()],
            2 => i.sp = pointers[(n / 10) % pointers.len()],
            3 => i.csp = pointers[(n / 10) % pointers.len()],
            4 => i.esp = addresses[(n / 10) % addresses.len()],
            5 => i.load_ap = true,
            6 => i.apc = apcs[(n / 10) % apcs.len()],
            7 => i.fetch = fetches[(n / 10) % fetches.len()],
            _ => {
                i.estk = estks[n % estks.len()];
                i.cstk = cstks[n % cstks.len()];
                i.sp = pointers[(n / 7) % pointers.len()];
                i.csp = pointers[(n / 11) % pointers.len()];
                i.esp = addresses[(n / 13) % addresses.len()];
                i.apc = apcs[(n / 17) % apcs.len()];
                i.fetch = fetches[(n / 19) % fetches.len()];
                i.load_ap = n % 2 == 0;
            }
        }
        if n % 7 == 0 {
            i.object = Some(Command::default());
        }
        image.code[n] = Some(i);
    }
    for (n, word) in image.nam.iter_mut().enumerate() {
        *word = Some(((n as u64) << 30) | 123);
    }
    for (n, target) in image.map.iter_mut().enumerate() {
        *target = Some((n * 3) as u16);
    }
    let jit = Jit::compile(&image)?;
    // Live table changes must not require retranslation.
    image.nam[1] = None;
    image.map[1] = None;
    let mut successes = 0;
    let mut faults = [0; 7];
    for pc in 0..512 {
        let i = image.code[pc].unwrap();
        for case in 0..32 {
            let mut cpu = Processor {
                pc: pc as u16,
                sp: (case % 33) as u32,
                esp: if case == 3 {
                    u32::MAX
                } else {
                    (case % 32) as u32
                },
                csp: if case == 4 {
                    32
                } else {
                    ((case + 1) % 32) as u32
                },
                ap: (case % 5) as u32,
                apc: [0, 1, 255, 256, 0xffffff, 0x1000000][case % 6],
                opcode: [0, 1, 1023, 1024, usize::MAX][case % 5],
                q: [0, 1, u32::MAX, 0x80000000][case % 4],
                cstkr: [0, 1, 0xffffff, u32::MAX][case % 4],
                estkr: 0x123456789a,
                ucar: 71,
                namarg: 0x12345,
                upcor: 17,
                ..Processor::default()
            };
            for n in 0..32 {
                cpu.estk[n] = 0x4000000000 + n as u64;
                cpu.cstk[n] = n as u32 + 7;
            }
            let before = cpu.clone();
            let expected = cpu.prepare_writes(&image, false);
            let actual = jit.prepare(&cpu, false, &image).unwrap();
            match (expected, actual) {
                (Ok((mut expected, c)), Ok((actual, jc))) => {
                    let mut actual = crate::execution::NativeWrites::from(actual);
                    assert_eq!(c, jc);
                    if c.is_some() {
                        expected.object = 123;
                        actual.object(123);
                    }
                    let mut oracle = cpu.clone();
                    expected.commit(&mut oracle, &image);
                    actual.commit(&mut cpu, &image, i);
                    assert_eq!(cpu, oracle, "PC {pc}, case {case}: {i:?}");
                    successes += 1;
                }
                (Err(a), Err(b)) => {
                    assert_eq!(a, b, "PC {pc}, case {case}: {i:?}");
                    faults[a as usize] += 1;
                    assert_eq!(cpu, before);
                }
                _ => panic!("PC {pc}, case {case}: {i:?}"),
            }
        }
    }
    assert!(successes > 1000);
    for code in [1, 3, 5] {
        assert!(faults[code] > 0, "missing fault {code}");
    }
    Ok(())
}
