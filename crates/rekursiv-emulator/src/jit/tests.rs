use super::*;

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
            let actual = jit.prepare(&cpu, i, irq).expect("eligible instruction");
            match (expected, actual) {
                (Ok((mut expected, command)), Ok((mut actual, jit_command))) => {
                    assert_eq!(command, jit_command, "PC {pc}, case {case}: {i:?}");
                    if command.is_some() {
                        expected.object = 123;
                        actual.object = 123;
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
