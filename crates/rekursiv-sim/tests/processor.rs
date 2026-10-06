use eyre::Result;
use rekursiv_asm::{processor::*, Command, Index, Memory, Read, Word};
use rekursiv_model::processor::{Image, Processor};
use rekursiv_sim::{runtime, Harness, Timing};
fn lit(value: u64) -> Instruction {
    Instruction::literal(Word::from_bits(value).unwrap())
}
fn jump(target: u16) -> Instruction {
    Instruction {
        seq: Seq::Jump,
        branch: target,
        ..Instruction::default()
    }
}
fn register(value: u32, rb: u8) -> Instruction {
    Instruction {
        data: Word::raw(value as u64).unwrap(),
        r: Source::Bus,
        write_register: true,
        rb,
        ..Instruction::default()
    }
}
fn object(command: Command) -> Instruction {
    Instruction {
        data: command.data,
        object: Some(command),
        ..Instruction::default()
    }
}

#[test]
fn arithmetic_stack_cache_and_full_word_equality() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let image = Image::program(&[
        register(0x7fffffff, 0),
        register(1, 1),
        Instruction {
            alu: Alu::Add,
            ra: 0,
            rb: 1,
            flags: true,
            write_register: true,
            ..Instruction::default()
        },
        Instruction {
            seq: Seq::ConditionalJump,
            condition: Condition::Overflow,
            branch: 5,
            ..Instruction::default()
        },
        Instruction::halt(),
        Instruction {
            sp: Pointer::Increment,
            esp: Address::Sp,
            ..Instruction::default()
        },
        Instruction {
            estk: Estk::Bus,
            symbol: true,
            ..lit(0x8123456789)
        },
        Instruction {
            sp: Pointer::Increment,
            esp: Address::Sp,
            ..Instruction::default()
        },
        Instruction {
            estk: Estk::Bus,
            ..lit(0xc123456789)
        },
        Instruction {
            condition: Condition::Symbol,
            bus: Bus::Estk,
            seq: Seq::ConditionalJump,
            branch: 4,
            ..Instruction::default()
        },
        Instruction {
            sp: Pointer::Decrement,
            esp: Address::Sp,
            ..Instruction::default()
        },
        Instruction {
            estk: Estk::Read,
            ..Instruction::default()
        },
        Instruction {
            condition: Condition::Symbol,
            bus: Bus::Estk,
            seq: Seq::ConditionalJump,
            branch: 14,
            ..Instruction::default()
        },
        Instruction::halt(),
        register(0xfffffff9, 2),
        register(3, 3),
        Instruction {
            alu: Alu::MultiplySigned,
            ra: 2,
            rb: 3,
            ..Instruction::default()
        },
        Instruction {
            alu: Alu::ProductHigh,
            write_register: true,
            rb: 4,
            ..Instruction::default()
        },
        Instruction {
            alu: Alu::ProductLow,
            write_register: true,
            rb: 5,
            ..Instruction::default()
        },
        Instruction {
            alu: Alu::Sub,
            ra: 3,
            rb: 3,
            carry: Carry::One,
            flags: true,
            load_q: true,
            ..Instruction::default()
        },
        Instruction {
            alu: Alu::Pass,
            r: Source::Branch,
            branch: 42,
            estk: Estk::Compact,
            compact_code: 2,
            ..Instruction::default()
        },
        Instruction::halt(),
    ]);
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    h.run_processor(&image, &mut model, 1000)?;
    assert_eq!(model.pc, 21);
    assert_eq!(model.rf[1], 0x80000000);
    assert_eq!(model.rf[4], u32::MAX);
    assert_eq!(model.rf[5], (-21i32) as u32);
    assert_eq!(model.estkr, Word::signed(42).bits());
    assert_eq!(model.q, 0);
    assert_eq!(model.flags, 5);
    Ok(())
}

#[test]
fn nested_calls_explicit_pop_relative_origin_and_service_resume() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let mut image = Image::default();
    for (pc, i) in [
        (0, jump(8)),
        (
            1,
            Instruction {
                seq: Seq::Service,
                branch: 3,
                ..Instruction::default()
            },
        ),
        (2, Instruction::halt()),
        (
            8,
            Instruction {
                csp: Pointer::Increment,
                ..Instruction::default()
            },
        ),
        (
            9,
            Instruction {
                cstk: Cstk::Upcor,
                ..Instruction::default()
            },
        ),
        (10, jump(16)),
        (
            11,
            Instruction {
                cstk: Cstk::Read,
                ..Instruction::default()
            },
        ),
        (
            12,
            Instruction {
                seq: Seq::Return,
                csp: Pointer::Decrement,
                ..Instruction::default()
            },
        ),
        (
            16,
            Instruction {
                csp: Pointer::Increment,
                ..Instruction::default()
            },
        ),
        (
            17,
            Instruction {
                cstk: Cstk::Upcor,
                ..Instruction::default()
            },
        ),
        (
            18,
            Instruction {
                seq: Seq::Relative,
                ..lit(2)
            },
        ),
        (19, Instruction::halt()),
        (
            20,
            Instruction {
                seq: Seq::Return,
                csp: Pointer::Decrement,
                ..Instruction::default()
            },
        ),
    ] {
        image.code[pc] = Some(i);
    }
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    h.run_processor(&image, &mut model, 1000)?;
    assert!(model.service);
    assert_eq!(model.pc, 2);
    assert_eq!(model.csp, 0);
    assert_eq!(model.service_code, 3);
    for _ in 0..10 {
        h.tick()?;
        h.compare_processor(&model)?;
    }
    h.rtl.cpu_resume_i = 1;
    h.tick()?;
    h.rtl.cpu_resume_i = 0;
    model.service = false;
    h.run_processor(&image, &mut model, 100)?;
    assert!(model.halted);
    Ok(())
}

#[test]
fn nam_dispatch_reads_old_opcode_when_fetches_overlap() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let mut image = Image::program(&[
        Instruction {
            fetch: Fetch::Nam,
            apc: Apc::Increment,
            ..Instruction::default()
        },
        Instruction {
            fetch: Fetch::Both,
            ..Instruction::default()
        },
        Instruction {
            seq: Seq::Dispatch,
            ..Instruction::default()
        },
        Instruction::halt(),
        Instruction {
            bus: Bus::Namarg,
            r: Source::Bus,
            write_register: true,
            ..Instruction::default()
        },
        Instruction::halt(),
    ]);
    image.nam[0] = Some(7 << 30 | 123);
    image.nam[1] = Some(8 << 30 | 456);
    image.map[7] = Some(4);
    image.map[8] = Some(3);
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    h.run_processor(&image, &mut model, 100)?;
    assert_eq!(model.pc, 5);
    assert_eq!(model.rf[0], 456);
    assert_eq!(model.ucar, 4);
    Ok(())
}

#[test]
fn autonomous_allocate_evict_refill_and_loop_with_channel_stalls() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 3,
            memory_latency: 7,
            response_stall: 5,
        },
        None,
    )?;
    let class = Word::reference(100, true)?;
    let mut code = vec![register(20, 0)];
    // Keep the first object in the 40-bit evaluation stack across pager collisions.
    code.push(object(Command::allocate(class, 2, true)?));
    code.push(Instruction {
        bus: Bus::Object,
        estk: Estk::Bus,
        ..Instruction::default()
    });
    code.push(object(Command::index(2)?));
    code.push(object(Command::write_field(Word::signed(1234))));
    let loop_pc = code.len() as u16;
    code.push(object(Command::allocate(class, 2, true)?));
    code.push(Instruction {
        alu: Alu::Sub,
        r: Source::Register,
        s: Source::Branch,
        branch: 1,
        ra: 0,
        rb: 0,
        carry: Carry::One,
        flags: true,
        write_register: true,
        ..Instruction::default()
    });
    code.push(Instruction {
        seq: Seq::ConditionalJump,
        condition: Condition::Zero,
        invert: true,
        branch: loop_pc,
        ..Instruction::default()
    });
    code.push(Instruction {
        bus: Bus::Estk,
        object: Some(Command::fetch(Word::NIL)),
        ..Instruction::default()
    });
    code.push(object(Command::index(2)?));
    code.push(object(Command::read_field()));
    code.push(Instruction {
        bus: Bus::Object,
        estk: Estk::Bus,
        ..Instruction::default()
    });
    code.push(Instruction::halt());
    let image = Image::program(&code);
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    h.run_processor(&image, &mut model, 30000)?;
    assert_eq!(model.rf[0], 0);
    assert_eq!(model.estkr, Word::signed(1234).bits());
    assert!(h.stats.saved_objects >= 5);
    assert_eq!(h.stats.commands, 26);
    Ok(())
}

#[test]
fn invalid_local_effect_cannot_issue_object_write_and_bounds_do_not_wrap() -> Result<()> {
    let rt = runtime()?;
    for bad in [
        Instruction {
            csp: Pointer::Decrement,
            ..Instruction::default()
        },
        Instruction {
            sp: Pointer::Bus,
            ..lit(32)
        },
        Instruction {
            seq: Seq::Relative,
            ..lit((1 << 40) - 1)
        },
        Instruction {
            seq: Seq::Bus,
            ..lit(1 << 24)
        },
    ] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let instruction = Instruction {
            object: Some(Command {
                index: Index::Increment,
                ..Command::default()
            }),
            ..bad
        };
        let image = Image::program(&[instruction]);
        h.load_processor(&image)?;
        h.start_processor(0)?;
        let before = h.stats.commands;
        for _ in 0..5 {
            h.tick()?;
        }
        assert_ne!(h.rtl.cpu_fault_o, 0);
        assert_eq!(h.stats.commands, before);
        assert_eq!(h.rtl.cpu_pc_o, 0);
        assert_eq!(h.rtl.dbg_idx_o, 0);
        assert_eq!(h.rtl.cpu_sp_o, 0);
        assert_eq!(h.rtl.cpu_csp_o, 0);
    }
    Ok(())
}

#[test]
fn object_error_and_reset_during_wait_preserve_processor_state() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 5,
            memory_latency: 20,
            response_stall: 0,
        },
        None,
    )?;
    let image = Image::program(&[Instruction {
        sp: Pointer::Increment,
        object: Some(Command {
            memory: Memory::Write,
            ..Command::default()
        }),
        ..lit(5)
    }]);
    h.load_processor(&image)?;
    h.start_processor(0)?;
    for _ in 0..20 {
        h.tick()?;
    }
    assert_eq!(h.rtl.cpu_fault_o, 4);
    assert_eq!(h.rtl.cpu_pc_o, 0);
    assert_eq!(h.rtl.cpu_sp_o, 0);
    h.reset()?;
    let image = Image::program(&[
        object(Command::allocate(Word::reference(1, true)?, 2, true)?),
        Instruction::halt(),
    ]);
    h.load_processor(&image)?;
    h.start_processor(0)?;
    for _ in 0..3 {
        h.tick()?;
    }
    assert_eq!(h.rtl.cpu_pc_o, 0);
    assert_eq!(h.rtl.cpu_rsp_ready_o, 1);
    h.reset()?;
    assert_eq!(h.rtl.cpu_halted_o, 1);
    assert_eq!(h.rtl.cpu_object_o, 0);
    h.start_processor(0)?;
    h.tick()?;
    assert_eq!(h.rtl.cpu_fault_o, 2); // reset invalidates the image
    Ok(())
}

#[test]
fn freeze_is_unconditional_and_relative_zero_is_a_retiring_loop() -> Result<()> {
    let rt = runtime()?;
    for seq in [Seq::Hold, Seq::Relative] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let image = Image::program(&[Instruction {
            seq,
            condition: Condition::Always,
            invert: true,
            object: if seq == Seq::Hold {
                Some(Command::read(Read::Index))
            } else {
                None
            },
            ..Instruction::default()
        }]);
        h.load_processor(&image)?;
        h.start_processor(0)?;
        for _ in 0..10 {
            h.tick()?;
            assert_eq!(h.rtl.cpu_pc_o, 0);
            assert_eq!(h.rtl.cpu_retire_o, (seq == Seq::Relative) as u8);
        }
        assert_eq!(h.stats.commands, 0);
    }
    Ok(())
}

#[test]
fn flags_are_previous_retirement_and_lastcc_preserves_selected_condition() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let image = Image::program(&[
        // Current result is zero, but the branch sees reset Z=0.
        Instruction {
            flags: true,
            condition: Condition::Zero,
            seq: Seq::ConditionalJump,
            branch: 7,
            ..Instruction::default()
        },
        Instruction {
            condition: Condition::Zero,
            seq: Seq::ConditionalJump,
            branch: 3,
            ..Instruction::default()
        },
        Instruction::halt(),
        Instruction {
            condition: Condition::Last,
            seq: Seq::ConditionalJump,
            branch: 5,
            ..Instruction::default()
        },
        Instruction::halt(),
        Instruction {
            condition: Condition::Last,
            invert: true,
            seq: Seq::ConditionalJump,
            branch: 7,
            ..Instruction::default()
        },
        Instruction::halt(),
        Instruction::halt(),
    ]);
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    h.run_processor(&image, &mut model, 100)?;
    assert_eq!(model.pc, 6);
    Ok(())
}

#[test]
fn condition_is_latched_before_command_backpressure() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let image = Image::program(&[
        Instruction {
            seq: Seq::ConditionalJump,
            condition: Condition::Interrupt,
            branch: 2,
            object: Some(Command::index(1)?),
            ..Instruction::default()
        },
        Instruction::halt(),
        Instruction::halt(),
    ]);
    h.load_processor(&image)?;
    h.rtl.cpu_command_enable_i = 0;
    h.rtl.cpu_irq_i = 0;
    h.start_processor(0)?;
    h.tick()?;
    assert_eq!(h.rtl.cpu_cmd_valid_o, 1);
    h.rtl.cpu_irq_i = 1;
    for _ in 0..10 {
        h.tick()?;
        assert_eq!(h.rtl.cpu_cmd_valid_o, 1);
        assert_eq!(h.rtl.cpu_pc_o, 0);
    }
    h.rtl.cpu_command_enable_i = 1;
    for _ in 0..10 {
        h.tick()?;
    }
    assert_eq!(h.rtl.cpu_pc_o, 1);
    assert_eq!(h.stats.commands, 1);
    Ok(())
}

#[test]
fn collector_retains_processor_stack_cache_symbol_and_code_roots() -> Result<()> {
    use rekursiv_asm::{RecoveryMode, Roots};
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let class = Word::reference(1, true)?;
    h.install(class, class, 0, &[])?;
    let literal_root = Word::reference(6, true)?;
    let dead = Word::reference(5, true)?;
    h.install(literal_root, class, 0, &[])?;
    h.install(dead, class, 0, &[])?;
    let allocation = object(Command::allocate(class, 0, true)?);
    let image = Image::program(&[
        allocation,
        Instruction {
            bus: Bus::Object,
            estk: Estk::Bus,
            ..Instruction::default()
        },
        Instruction {
            sp: Pointer::Increment,
            esp: Address::Sp,
            ..Instruction::default()
        },
        allocation,
        Instruction {
            bus: Bus::Object,
            symbol: true,
            ..Instruction::default()
        },
        allocation,
        Instruction {
            bus: Bus::Object,
            estk: Estk::Bus,
            ..Instruction::default()
        },
        allocation,
        Instruction {
            seq: Seq::Service,
            ..Instruction::default()
        },
        Instruction::halt(),
        Instruction::literal(literal_root),
    ]);
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    h.run_processor(&image, &mut model, 1000)?;
    // Erase selection and remove incidental OBJEKT roots.
    h.execute(Command::probe(class))?;
    h.recover(RecoveryMode::Collect, &Roots::default())?;
    for bits in [
        model.estk[0],
        model.symbol,
        model.estkr,
        model.object,
        literal_root.bits(),
    ] {
        let r = Word::from_bits(bits)?;
        assert!(h.store.records.contains_key(&r.identity()?) || h.oracle.resolve(r).is_ok());
    }
    assert!(!h.store.records.contains_key(&dead.identity()?));
    h.compare_processor(&model)?;
    h.resume_processor()?;
    model.service = false;
    h.run_processor(&image, &mut model, 100)?;
    assert!(model.halted);
    Ok(())
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(32))]
    #[test]
    fn arithmetic_matches_independent_integer_model(cases in proptest::collection::vec((proptest::prelude::any::<u32>(),proptest::prelude::any::<u32>(),0u8..13,0u8..3,0u8..4),1..24)) {
        let rt=runtime().unwrap();let mut h=Harness::new(&rt,Timing::default(),None).unwrap();
        let ops=[Alu::Pass,Alu::Add,Alu::Sub,Alu::SubReverse,Alu::And,Alu::Or,Alu::Xor,Alu::Not,Alu::Rotate,Alu::MultiplySigned,Alu::MultiplyUnsigned,Alu::ProductHigh,Alu::ProductLow];
        let mut code=Vec::new();
        for (r,s,op,cin,shift) in cases {
            code.push(register(r,0));code.push(register(s,1));
            code.push(Instruction{alu:ops[op as usize],ra:0,rb:1,carry:[Carry::Zero,Carry::One,Carry::ZeroFlag][cin as usize],shift:[Shift::None,Shift::Left,Shift::Right,Shift::ArithmeticRight][shift as usize],write_register:true,load_q:true,flags:true,..Instruction::default()});
        }
        code.push(Instruction::halt());let image=Image::program(&code);
        h.load_processor(&image).unwrap();h.start_processor(0).unwrap();
        h.run_processor(&image,&mut Processor::default(),1000).unwrap();
    }
}
