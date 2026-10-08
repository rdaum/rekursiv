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
fn control_store_validity_requires_every_lane_and_reset_invalidates_all_words() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let addresses = [0, 255, 256, 4095, 4096, 8191];
    let lanes = Instruction::halt().encode()?;
    // Program out of order, including the highest word and bank boundaries.
    // Repeating a lane must not substitute for a missing lane.
    for lane in [7, 0, 3, 3, 1, 6, 2, 5] {
        for address in addresses {
            h.program_lane(0, address, lane, lanes[lane])?;
            h.rtl.cpu_dbg_addr_i = address as u16;
            h.rtl.eval();
            assert_eq!(h.rtl.cpu_dbg_code_valid_o, 0);
        }
    }
    for address in addresses {
        h.program_lane(0, address, 4, lanes[4])?;
        h.rtl.cpu_dbg_addr_i = address as u16;
        h.rtl.eval();
        assert_eq!(h.rtl.cpu_dbg_code_valid_o, 1);
    }
    h.start_processor(8191)?;
    for _ in 0..8 {
        h.tick()?;
    }
    assert_ne!(h.rtl.cpu_halted_o, 0);
    assert_eq!(h.rtl.cpu_fault_o, 0);
    h.reset()?;
    for address in addresses {
        h.rtl.cpu_dbg_addr_i = address as u16;
        h.rtl.eval();
        assert_eq!(h.rtl.cpu_dbg_code_valid_o, 0);
    }
    h.program_lane(0, 8191, 4, lanes[4])?;
    h.start_processor(8191)?;
    for _ in 0..8 {
        h.tick()?;
    }
    assert_ne!(h.rtl.cpu_halted_o, 0);
    assert_eq!(h.rtl.cpu_fault_o, 2, "partial word must fault after reset");
    Ok(())
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

#[test]
fn floating_point_retires_once_and_preserves_exceptions_across_collection() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 3,
            memory_latency: 5,
            response_stall: 4,
        },
        None,
    )?;
    let class = Word::reference(1, true)?;
    h.install(class, class, 0, &[])?;
    h.install(Word::reference(2, true)?, class, 0, &vec![Word::NIL; 256])?;
    let assembly = rekursiv_asm::text::assemble(
        "d=0x3f800000, r=Bus, rb=2, ldrb\n\
         d=0x40400000, r=Bus, rb=3, ldrb\n\
         ra=2, rb=3, alu=Float, fp=Divide, ldq, flags\n\
         alu=FloatStatus, rb=4, ldrb\n\
         d=CLASS, page=Fetch\n\
         d=CLASS, page=Allocate, size=4, scan=1\n\
         alu=FloatStatus, rb=5, ldrb\n\
         r=Q, alu=Float, fp=ToSigned, round=Up, rb=6, ldrb\n\
         halt",
        0,
        &[("CLASS", class.bits() as i64)],
    )?;
    let image = Image::from_assembly(&assembly)?.with_ram_collector(128, 16)?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    let retired = h.run_processor(&image, &mut model, 500_000)?;
    assert_eq!(retired, 9);
    assert_eq!(model.q, (1.0f32 / 3.0).to_bits());
    assert_eq!((model.rf[4], model.rf[5], model.rf[6]), (1, 1, 1));
    assert_eq!(model.fp_flags, 1);
    assert_eq!(h.stats.collections, 1);
    Ok(())
}

#[test]
fn floating_point_encoding_rejects_ambiguous_parallel_effects() {
    for source in [
        "alu=Float, fp=Divide, page=Fetch, d=0xa000000001",
        "alu=Float, fp=Add, shift=Left",
        "alu=Float, fp=Add, cin=One",
        "alu=Float, estk=Compact, compact=2",
        "alu=Add, fp=Divide",
        "round=TowardZero",
    ] {
        assert!(
            rekursiv_asm::text::assemble(source, 0, &[]).is_err(),
            "{source}"
        );
    }
    let a = rekursiv_asm::text::assemble("alu=Float, fp=ToSigned, round=TowardZero, ldq", 0, &[])
        .unwrap();
    let words = a.code[&0].encode().unwrap();
    assert_eq!((words[2] >> 16) & 15, 13);
    assert_eq!((words[6] >> 25) & 15, 7);
    assert_eq!((words[6] >> 29) & 7, 1);
}

#[test]
fn floating_point_latches_branch_condition_before_variable_latency_wait() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let a = rekursiv_asm::text::assemble(
        "d=2, r=Bus, rb=1, ldrb\n\
         ra=1, alu=Float, fp=FromUnsigned, rb=1, ldrb\n\
         ra=1, alu=Float, fp=Sqrt, rb=1, ldrb, flags, seq=ConditionalJump, cc=Interrupt, brch=4\n\
         halt\n\
         d=99, r=Bus, rb=2, ldrb\n\
         halt",
        0,
        &[],
    )?;
    h.load_processor(&Image::from_assembly(&a)?)?;
    h.rtl.cpu_irq_i = 1;
    h.start_processor(0)?;
    for _ in 0..100 {
        if h.rtl.cpu_pc_o == 2 {
            break;
        }
        h.tick()?;
    }
    assert_eq!(h.rtl.cpu_pc_o, 2);
    // Allow EXECUTE to accept sqrt, then remove IRQ while its result is pending.
    h.tick()?;
    h.tick()?;
    assert_eq!(h.rtl.cpu_pc_o, 2);
    h.rtl.cpu_irq_i = 0;
    for _ in 0..100 {
        if h.rtl.cpu_halted_o != 0 {
            break;
        }
        h.tick()?;
    }
    assert_ne!(h.rtl.cpu_halted_o, 0);
    assert_eq!(h.rtl.cpu_fault_o, 0);
    h.rtl.cpu_dbg_addr_i = 2;
    h.rtl.eval();
    assert_eq!(h.rtl.cpu_dbg_rf_o, 99);
    h.rtl.cpu_dbg_addr_i = 1;
    h.rtl.eval();
    assert_eq!(h.rtl.cpu_dbg_rf_o, 2.0f32.sqrt().to_bits());
    Ok(())
}

#[test]
fn wide_stack_pack_and_symbol_high_preserve_all_forty_bits() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    // D supplies only the high byte; its other bits must not leak into the
    // arithmetic low word. Reading SYMBOL high must zero-extend that byte.
    let assembly = rekursiv_asm::text::assemble(
        "d=0x89abcdef, r=Bus, ldq\n\
         d=0x123456781f, r=Q, estk=Wide\n\
         d=Estk, ldsym\n\
         d=SymbolHigh, r=Bus, rb=2, ldrb\n\
         d=Symbol, r=Bus, rb=3, ldrb\n\
         d=0xff, r=Q, estk=Wide\n\
         d=Estk, ldsym\n\
         d=SymbolHigh, r=Bus, rb=4, ldrb\n\
         halt",
        0,
        &[],
    )?;
    let image = Image::from_assembly(&assembly)?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    h.run_processor(&image, &mut model, 1000)?;
    assert_eq!(
        (model.rf[2], model.rf[3], model.rf[4]),
        (31, 0x89abcdef, 255)
    );
    assert_eq!(model.estkr, 0xff89abcdef);
    Ok(())
}

#[test]
fn device_transactions_retire_once_and_results_survive_collection() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 4,
            memory_latency: 7,
            response_stall: 3,
        },
        None,
    )?;
    h.device.registers.insert(0x200, 0x12345678);
    let class = Word::reference(1, true)?;
    h.install(class, class, 0, &[])?;
    h.install(Word::reference(2, true)?, class, 0, &vec![Word::NIL; 256])?;
    let assembly = rekursiv_asm::text::assemble(
        "d=0x200, r=Bus, rb=1, ldrb\n\
         ra=1, io=Read, d=7, r=Bus, rb=2, ldrb\n\
         d=Device, r=Bus, rb=3, ldrb\n\
         ra=1, d=0xfedcba98, io=Write\n\
         ra=1, io=Read\n\
         d=CLASS, page=Fetch\n\
         d=CLASS, page=Allocate, size=4, scan=1\n\
         d=Device, r=Bus, rb=4, ldrb\n\
         halt",
        0,
        &[("CLASS", class.bits() as i64)],
    )?;
    let image = Image::from_assembly(&assembly)?.with_ram_collector(128, 16)?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    assert_eq!(h.run_processor(&image, &mut model, 500_000)?, 9);
    assert_eq!(
        (model.rf[2], model.rf[3], model.rf[4]),
        (7, 0x12345678, 0xfedcba98)
    );
    assert_eq!(h.device.requests.len(), 3);
    assert_eq!(h.device.completions.len(), 3);
    assert_eq!(h.device.registers[&0x200], 0xfedcba98);
    assert_eq!(h.stats.collections, 1);
    Ok(())
}

#[test]
fn device_errors_and_unaligned_addresses_do_not_retire_local_effects() -> Result<()> {
    let rt = runtime()?;
    for (address, fault, requests) in [(0x204, 6, 2), (0x203, 1, 1)] {
        let mut h = Harness::new(
            &rt,
            Timing {
                request_delay: 3,
                memory_latency: 5,
                response_stall: 0,
            },
            None,
        )?;
        h.device.registers.insert(0x200, 55);
        let assembly = rekursiv_asm::text::assemble(
            "d=0x200, r=Bus, rb=1, ldrb\n\
             ra=1, io=Read\n\
             d=BAD_ADDRESS, r=Bus, rb=1, ldrb\n\
             ra=1, d=99, io=Write, r=Bus, rb=2, ldrb, ldq\n\
             halt",
            0,
            &[("BAD_ADDRESS", address)],
        )?;
        let image = Image::from_assembly(&assembly)?;
        h.load_processor(&image)?;
        h.start_processor(0)?;
        let mut model = Processor::default();
        assert!(h.run_processor(&image, &mut model, 1000).is_err());
        assert_eq!((h.rtl.cpu_fault_o, h.rtl.cpu_pc_o), (fault, 3));
        assert_eq!(h.rtl.cpu_q_o, 0);
        assert_eq!(h.rtl.cpu_device_result_o, 55);
        h.rtl.cpu_dbg_addr_i = 2;
        h.rtl.eval();
        assert_eq!(h.rtl.cpu_dbg_rf_o, 0);
        assert_eq!(h.device.requests.len(), requests);
        assert_eq!(h.device.registers.len(), 1);
        assert_eq!(h.device.registers[&0x200], 55);
    }
    for source in [
        "io=Read, page=Fetch",
        "io=Write, alu=Float",
        "io=Read, gc=Begin",
    ] {
        assert!(rekursiv_asm::text::assemble(source, 0, &[]).is_err());
    }
    Ok(())
}

#[test]
fn device_wait_captures_irq_and_reset_cancels_unfinished_writes() -> Result<()> {
    let rt = runtime()?;
    let assembly = rekursiv_asm::text::assemble(
        "d=0x200, r=Bus, rb=1, ldrb\n\
         ra=1, d=99, io=Write, seq=ConditionalJump, cc=Interrupt, brch=3\n\
         halt\n\
         d=77, r=Bus, rb=2, ldrb\n\
         halt",
        0,
        &[],
    )?;
    let image = Image::from_assembly(&assembly)?;
    for reset in [false, true] {
        let mut h = Harness::new(
            &rt,
            Timing {
                request_delay: 4,
                memory_latency: 20,
                response_stall: 0,
            },
            None,
        )?;
        h.device.registers.insert(0x200, 55);
        h.load_processor(&image)?;
        h.rtl.cpu_irq_i = 1;
        h.start_processor(0)?;
        for _ in 0..100 {
            if !h.device.requests.is_empty() {
                break;
            }
            h.tick()?;
        }
        assert_eq!(h.device.requests.len(), 1);
        assert_eq!(h.rtl.cpu_pc_o, 1);
        assert_eq!(h.device.registers[&0x200], 55);
        h.rtl.cpu_irq_i = 0;
        if reset {
            h.reset()?;
            h.device.registers.insert(0x200, 55);
        }
        for _ in 0..100 {
            h.tick()?;
        }
        assert_ne!(h.rtl.cpu_halted_o, 0);
        assert_eq!(h.rtl.cpu_fault_o, 0);
        h.rtl.cpu_dbg_addr_i = 2;
        h.rtl.eval();
        assert_eq!(h.rtl.cpu_dbg_rf_o, if reset { 0 } else { 77 });
        assert_eq!(h.device.registers[&0x200], if reset { 55 } else { 99 });
        assert_eq!(h.device.completions.len(), if reset { 0 } else { 1 });
    }
    Ok(())
}

#[test]
fn clock_device_latches_high_words_across_rollover_and_delayed_replies() -> Result<()> {
    let rt = runtime()?;
    for address in [0x200, 0x208] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let assembly = rekursiv_asm::text::assemble(
            "d=LOW, r=Bus, rb=1, ldrb\n\
             ra=1, io=Read\n\
             d=Device, r=Bus, rb=3, ldrb\n\
             d=HIGH, r=Bus, rb=1, ldrb\n\
             ra=1, io=Read\n\
             d=Device, r=Bus, rb=4, ldrb\n\
             halt",
            0,
            &[("LOW", address), ("HIGH", address + 4)],
        )?;
        let image = Image::from_assembly(&assembly)?;
        h.load_processor(&image)?;
        h.device.clocks = Some(rekursiv_sim::device::Clocks::new(
            0x2_ffff_ffff,
            0x2_ffff_fff0,
            1,
        ));
        h.device.timing = Timing {
            request_delay: 3,
            memory_latency: 1500,
            response_stall: 0,
        };
        h.start_processor(0)?;
        let mut model = Processor::default();
        h.run_processor(&image, &mut model, 10_000)?;
        assert_eq!(model.rf[4], 2);
        assert!(model.rf[3] >= 0xffff_fff0);
        let clocks = h.device.clocks.as_ref().unwrap();
        assert_eq!(
            (if address == 0x200 {
                clocks.utc_seconds
            } else {
                clocks.monotonic_ms
            }) >> 32,
            3
        );
        assert_eq!(h.device.completions.len(), 2);
    }
    Ok(())
}

#[test]
fn mutable_explicit_roots_retain_dynamic_objects_and_can_release_them() -> Result<()> {
    let rt = runtime()?;
    let class = Word::reference(1, true)?;
    for clear in [false, true] {
        let mut h = Harness::new(
            &rt,
            Timing {
                request_delay: 2,
                memory_latency: 3,
                response_stall: 2,
            },
            None,
        )?;
        h.install(class, class, 0, &[])?;
        h.install(Word::reference(2, true)?, class, 0, &vec![Word::ZERO; 220])?;
        let source = format!(
            "d=27, r=Bus, rb=1, ldrb\n\
             d=CLASS, page=Allocate, size=8, scan=1\n\
             ra=1, d=Object, ldroot\n\
             {}\n\
             d=CLASS, page=Fetch\n\
             d=CLASS, page=Allocate, size=40, scan=1\n\
             ra=1, d=Root, ldsym\n\
             halt",
            if clear { "ra=1, d=0, ldroot" } else { "nop" },
        );
        let assembly = rekursiv_asm::text::assemble(&source, 0, &[("CLASS", class.bits() as i64)])?;
        let mut image = Image::from_assembly(&assembly)?.with_ram_collector(128, 16)?;
        image.roots[27] = Word::signed(-7);
        h.load_processor(&image)?;
        h.start_processor(0)?;
        let mut model = Processor::default();
        h.run_processor(&image, &mut model, 500_000)?;
        let child = Word::reference(3, true)?;
        assert_eq!(model.symbol, if clear { 0 } else { child.bits() });
        assert_eq!(
            model.roots.unwrap()[27],
            if clear { Word::ZERO } else { child }
        );
        assert_eq!(
            h.oracle
                .entries
                .iter()
                .flatten()
                .any(|e| e.reference == child),
            !clear
        );
        assert_eq!(h.stats.collections, 1);
    }
    Ok(())
}

#[test]
fn explicit_root_index_and_failed_object_command_cannot_change_registration() -> Result<()> {
    let rt = runtime()?;
    for index in [27, 32] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let source =
            format!("d={index}, r=Bus, rb=1, ldrb\nra=1, d=0xa00000007b, page=Fetch, ldroot\nhalt");
        let assembly = rekursiv_asm::text::assemble(&source, 0, &[])?;
        let mut image = Image::from_assembly(&assembly)?;
        image.roots[27] = Word::signed(42);
        h.load_processor(&image)?;
        h.start_processor(0)?;
        let mut model = Processor::default();
        assert!(h.run_processor(&image, &mut model, 1000).is_err());
        assert_eq!(h.rtl.cpu_fault_o, if index == 27 { 4 } else { 1 });
        h.rtl.cpu_dbg_addr_i = 27;
        h.rtl.eval();
        assert_eq!(h.rtl.cpu_dbg_root_o, Word::signed(42).bits());
        assert_eq!(h.commands.len(), if index == 27 { 1 } else { 0 });
    }
    Ok(())
}

#[test]
fn pointer_device_latches_coordinates_during_motion_and_publishes_cursor_atomically() -> Result<()>
{
    let rt = runtime()?;
    for fail_publish in [false, true] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let assembly = rekursiv_asm::text::assemble(
            "d=0x300, r=Bus, rb=1, ldrb
             ra=1, io=Read
             d=Device, r=Bus, rb=3, ldrb
             d=0x304, r=Bus, rb=1, ldrb
             ra=1, io=Read
             d=Device, r=Bus, rb=4, ldrb
             d=0x400, r=Bus, rb=1, ldrb
             ra=1, d=111, io=Write
             d=0x404, r=Bus, rb=1, ldrb
             ra=1, d=222, io=Write
             d=0x408, r=Bus, rb=1, ldrb
             seq=Service, brch=1
             ra=1, d=1, io=Write
             halt",
            0,
            &[],
        )?;
        let image = Image::from_assembly(&assembly)?;
        h.load_processor(&image)?;
        let mut pointer = rekursiv_sim::device::Pointer::default();
        pointer.mouse = (-7, 9);
        pointer.cursor = (10, 20);
        // The X read starts before this motion and completes afterwards.
        pointer.schedule.insert(100, (300, 400));
        h.device.pointer = Some(pointer);
        h.device.timing = Timing {
            request_delay: 3,
            memory_latency: 200,
            response_stall: 0,
        };
        h.start_processor(0)?;
        let mut model = Processor::default();
        h.run_processor_observed(&image, &mut model, 10_000, |h, _| {
            assert_eq!(h.device.pointer.as_ref().unwrap().cursor, (10, 20));
            Ok(())
        })?;
        assert!(model.service);
        assert_eq!(h.device.completions.len(), 4);
        h.device.fail_next = fail_publish;
        h.resume_processor()?;
        model.service = false;
        let result = h.run_processor(&image, &mut model, 10_000);
        if fail_publish {
            assert!(result.is_err());
        } else {
            result?;
        }
        let pointer = h.device.pointer.as_ref().unwrap();
        assert_eq!((model.rf[3] as i32, model.rf[4] as i32), (-7, 9));
        assert_eq!(pointer.mouse, (300, 400));
        assert_eq!(
            pointer.cursor,
            if fail_publish { (10, 20) } else { (111, 222) }
        );
        assert_eq!(h.device.completions.len(), 5);
    }
    Ok(())
}

#[test]
fn input_fifo_preserves_packet_fields_under_pressure_and_failed_consume() -> Result<()> {
    use rekursiv_sim::device::{Input, InputKind, InputPacket};
    let rt = runtime()?;
    for fail_pop in [false, true] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let assembly = rekursiv_asm::text::assemble(
            "d=0x314, r=Bus, rb=1, ldrb
             ra=1, io=Read
             d=Device, r=Bus, rb=3, ldrb
             d=0x318, r=Bus, rb=1, ldrb
             ra=1, io=Read
             d=Device, r=Bus, rb=4, ldrb
             d=0x31c, r=Bus, rb=1, ldrb
             ra=1, io=Read
             d=Device, r=Bus, rb=5, ldrb
             d=0x320, r=Bus, rb=1, ldrb
             ra=1, io=Read
             d=Device, r=Bus, rb=6, ldrb
             seq=Service, brch=1
             d=0x324, r=Bus, rb=1, ldrb
             ra=1, d=1, io=Write
             d=0x104, r=Bus, rb=1, ldrb
             ra=1, d=1, io=Write
             d=0x310, r=Bus, rb=1, ldrb
             ra=1, io=Read
             d=Device, r=Bus, rb=7, ldrb
             halt",
            0,
            &[],
        )?;
        let image = Image::from_assembly(&assembly)?;
        h.load_processor(&image)?;
        let packet = InputPacket {
            kind: InputKind::Motion,
            value: -7,
            extra: 9000,
            timestamp_ms: 0xfedcba98,
        };
        let mut input = Input::new(2);
        input.schedule.insert(0, vec![packet]);
        input.schedule.insert(
            100,
            vec![InputPacket {
                value: 100,
                ..packet
            }],
        );
        input.schedule.insert(
            200,
            vec![InputPacket {
                value: 200,
                ..packet
            }],
        );
        h.device.input = Some(input);
        h.device.events = Some(Default::default());
        h.device.timing = Timing {
            request_delay: 3,
            memory_latency: 100,
            response_stall: 0,
        };
        h.start_processor(0)?;
        let mut model = Processor::default();
        h.run_processor(&image, &mut model, 10_000)?;
        assert!(model.service);
        assert_eq!(&model.rf[3..7], &[1, (-7i32) as u32, 9000, 0xfedcba98]);
        assert_eq!(h.device.input.as_ref().unwrap().len(), 2);
        assert_eq!(h.device.input.as_ref().unwrap().overruns, 1);
        assert_eq!(h.device.events.as_ref().unwrap().counts, [2, 0, 0, 0]);
        h.device.fail_next = fail_pop;
        h.resume_processor()?;
        model.service = false;
        let result = h.run_processor(&image, &mut model, 10_000);
        if fail_pop {
            assert!(result.is_err());
            assert_eq!(h.rtl.cpu_fault_o, 6);
        } else {
            result?;
            assert_eq!(model.rf[7], 0x80000001);
        }
        assert_eq!(
            h.device.input.as_ref().unwrap().len(),
            if fail_pop { 2 } else { 1 }
        );
        assert_eq!(
            h.device.events.as_ref().unwrap().counts,
            [if fail_pop { 2 } else { 1 }, 0, 0, 0]
        );
    }
    Ok(())
}

#[test]
fn bitmap_device_publishes_complete_frames_atomically_and_rejects_partial_or_failed_publication(
) -> Result<()> {
    use rekursiv_sim::device::{Bitmap, BitmapFrame};
    let rt = runtime()?;
    for case in 0..3 {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let assembly = rekursiv_asm::text::assemble(
            &format!(
                "d=0x518, r=Bus, rb=1, ldrb
             ra=1, d=33, io=Write
             d=0x51c, r=Bus, rb=1, ldrb
             ra=1, d=1, io=Write
             d=0x520, r=Bus, rb=1, ldrb
             ra=1, d=2, io=Write
             d=0x524, r=Bus, rb=1, ldrb
             ra=1, d=0, io=Write
             d=0x52c, r=Bus, rb=1, ldrb
             ra=1, d=0xfedcba98, io=Write
             {}
             seq=Service, brch=1
             d=0x524, r=Bus, rb=1, ldrb
             ra=1, d=1, io=Write
             halt",
                if case == 2 {
                    ""
                } else {
                    "ra=1, d=0x80000000, io=Write"
                }
            ),
            0,
            &[],
        )?;
        let image = Image::from_assembly(&assembly)?;
        h.load_processor(&image)?;
        let old = BitmapFrame {
            width: 1,
            height: 1,
            stride: 1,
            words: vec![0],
            ..BitmapFrame::default()
        };
        let mut bitmap = Bitmap::new(64, 64);
        bitmap.visible = Some(old.clone());
        h.device.display_bitmap = Some(bitmap);
        h.device.timing = Timing {
            request_delay: 4,
            memory_latency: 7,
            response_stall: 0,
        };
        h.start_processor(0)?;
        let mut model = Processor::default();
        h.run_processor_observed(&image, &mut model, 10_000, |h, _| {
            assert_eq!(
                h.device.display_bitmap.as_ref().unwrap().visible.as_ref(),
                Some(&old)
            );
            Ok(())
        })?;
        assert!(model.service);
        h.device.fail_next = case == 1;
        h.resume_processor()?;
        model.service = false;
        let result = h.run_processor(&image, &mut model, 10_000);
        let bitmap = h.device.display_bitmap.as_ref().unwrap();
        if case == 0 {
            result?;
            assert_eq!(
                bitmap.visible,
                Some(BitmapFrame {
                    width: 33,
                    height: 1,
                    stride: 2,
                    words: vec![0xfedcba98, 0x80000000],
                    ..BitmapFrame::default()
                })
            );
            assert_eq!(bitmap.publications, 1);
        } else {
            assert!(result.is_err());
            assert_eq!(h.rtl.cpu_fault_o, 6);
            assert_eq!(bitmap.visible, Some(old));
            assert_eq!(bitmap.publications, 0);
        }
    }
    Ok(())
}

#[test]
fn sparse_bitmap_upload_is_atomic_across_delayed_and_failed_publish() -> Result<()> {
    use rekursiv_sim::device::{Bitmap, BitmapFrame};
    let rt = runtime()?;
    for fail_publish in [false, true] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let assembly = rekursiv_asm::text::assemble(
            "d=0x518, r=Bus, rb=1, ldrb
             ra=1, d=33, io=Write
             d=0x51c, r=Bus, rb=1, ldrb
             ra=1, d=2, io=Write
             d=0x520, r=Bus, rb=1, ldrb
             ra=1, d=2, io=Write
             d=0x524, r=Bus, rb=1, ldrb
             ra=1, d=2, io=Write
             d=0x528, r=Bus, rb=1, ldrb
             ra=1, d=3, io=Write
             d=0x52c, r=Bus, rb=1, ldrb
             ra=1, d=0x80000000, io=Write
             seq=Service, brch=1
             d=0x524, r=Bus, rb=1, ldrb
             ra=1, d=1, io=Write
             halt",
            0,
            &[],
        )?;
        let image = Image::from_assembly(&assembly)?;
        h.load_processor(&image)?;
        let old = BitmapFrame {
            width: 33,
            height: 2,
            stride: 2,
            words: vec![1, 2, 3, 4],
            ..BitmapFrame::default()
        };
        let mut bitmap = Bitmap::new(64, 64);
        bitmap.visible = Some(old.clone());
        h.device.display_bitmap = Some(bitmap);
        h.device.timing = Timing {
            request_delay: 4,
            memory_latency: 7,
            response_stall: 0,
        };
        h.start_processor(0)?;
        let mut model = Processor::default();
        h.run_processor_observed(&image, &mut model, 10_000, |h, _| {
            assert_eq!(
                h.device.display_bitmap.as_ref().unwrap().visible,
                Some(old.clone())
            );
            Ok(())
        })?;
        assert!(model.service);
        h.device.fail_next = fail_publish;
        h.resume_processor()?;
        model.service = false;
        let result = h.run_processor(&image, &mut model, 10_000);
        let b = h.device.display_bitmap.as_ref().unwrap();
        assert_eq!(b.pixel_writes, 1);
        if fail_publish {
            assert!(result.is_err());
            assert_eq!(b.visible, Some(old));
            assert_eq!(b.publications, 0);
        } else {
            result?;
            assert_eq!(b.visible.as_ref().unwrap().words, vec![1, 2, 3, 0x80000000]);
            assert_eq!(b.publications, 1);
        }
    }
    Ok(())
}

/// Register pairs and PRODUCT must agree at every retirement, including a GC
/// between completing a binary64 operation and transferring its high half.
#[test]
fn binary64_product_and_status_survive_collection() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 3,
            memory_latency: 5,
            response_stall: 4,
        },
        None,
    )?;
    let class = Word::reference(1, true)?;
    h.install(class, class, 0, &[])?;
    h.install(Word::reference(2, true)?, class, 0, &vec![Word::NIL; 256])?;
    let assembly = rekursiv_asm::text::assemble(
        "d=0, r=Bus, rb=2, ldrb\n\
         d=0x3ff00000, r=Bus, rb=3, ldrb\n\
         d=0, r=Bus, rb=4, ldrb\n\
         d=0x40080000, r=Bus, rb=5, ldrb\n\
         ra=2, rb=4, alu=Float, precision=Binary64, fp=Divide, ldq, flags\n\
         alu=FloatStatus, rb=6, ldrb\n\
         d=CLASS, page=Fetch\n\
         d=CLASS, page=Allocate, size=4, scan=1\n\
         alu=ProductHigh, rb=7, ldrb\n\
         alu=ProductLow, rb=8, ldrb\n\
         alu=FloatStatus, rb=9, ldrb\n\
         halt",
        0,
        &[("CLASS", class.bits() as i64)],
    )?;
    let image = Image::from_assembly(&assembly)?.with_ram_collector(128, 16)?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    assert_eq!(h.run_processor(&image, &mut model, 500_000)?, 12);
    assert_eq!(model.product, (1.0f64 / 3.0).to_bits());
    assert_eq!(
        ((model.rf[7] as u64) << 32) | model.rf[8] as u64,
        model.product
    );
    assert_eq!((model.rf[6], model.rf[9]), (1, 1));
    assert_eq!(h.stats.collections, 1);
    Ok(())
}

#[test]
fn binary64_encoding_requires_register_pairs() {
    for source in [
        "precision=Binary64",
        "alu=Float, precision=Binary64, ra=1",
        "alu=Float, precision=Binary64, rb=15",
        "alu=Float, precision=Binary64, r=Q",
        "alu=Float, precision=Binary64, s=Bus",
    ] {
        assert!(
            rekursiv_asm::text::assemble(source, 0, &[]).is_err(),
            "{source}"
        );
    }
    let a =
        rekursiv_asm::text::assemble("alu=Float, precision=Binary64, ra=14, rb=2", 0, &[]).unwrap();
    assert_eq!((a.code[&0].encode().unwrap()[7] >> 6) & 1, 1);
}
