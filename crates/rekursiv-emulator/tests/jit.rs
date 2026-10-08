use eyre::Result;
use rekursiv_asm::{processor::*, Word};
use rekursiv_emulator::{boot, Machine, Step};
use rekursiv_model::processor::Image;

#[test]
fn edited_control_store_falls_back_and_can_be_recompiled() -> Result<()> {
    let original = Instruction {
        data: Word::raw(17)?,
        r: Source::Bus,
        write_register: true,
        ..Instruction::default()
    };
    let mut machine = Machine::new(Image::program(&[original, Instruction::halt()]), 0, 16, 512)?;
    assert!(machine.enable_jit()? > 0);
    // A mutable image borrow invalidates changed translations before execution.
    machine.image_mut().code[0].as_mut().unwrap().data = Word::raw(73)?;
    assert_eq!(machine.step()?, Step::Retired);
    assert_eq!(machine.cpu.rf[0], 73);
    machine.cpu.pc = 0;
    machine.enable_jit()?;
    machine.step()?;
    assert_eq!(machine.cpu.rf[0], 73);
    // An invalid replacement must be validated, never run as cached code.
    machine.cpu.pc = 0;
    machine.image_mut().code[0].as_mut().unwrap().ra = 16;
    assert!(machine.step().is_err());
    assert_eq!(machine.fault.unwrap().code, 1);
    assert_eq!(machine.cpu.rf[0], 73);
    Ok(())
}

#[test]
fn jit_and_interpreter_match_through_gc_objects_and_devices() -> Result<()> {
    for source in [
        include_str!("../../../microcode/collection.uc"),
        include_str!("../../../microcode/allocation.uc"),
        include_str!("../../../microcode/pipeline.uc"),
        "d=16, r=Bus, ldrb\nd=73, io=Write\nio=Read\nd=Device, r=Bus, rb=1, ldrb\nseq=Service, brch=3",
        "d=0xa000000064, page=Allocate, size=2, scan=0\nidx=Clear, prepare\nmem=Read, prepared, launch\nd=17, r=Bus, rb=7, ldrb\nd=Object, r=Bus, rb=8, ldrb\nhalt",
    ] {
        let mut interpreter = boot::microcode_with_pager(source, 512, 16)?.machine;
        let mut jit = boot::microcode_with_pager(source, 512, 16)?.machine;
        jit.enable_jit()?;
        interpreter.devices.registers.insert(16, 0);
        jit.devices.registers.insert(16, 0);
        interpreter.objekt_metrics_enabled = true;
        jit.objekt_metrics_enabled = true;
        for n in 0..1_000_000 {
            let expected = interpreter.step();
            let actual = jit.step();
            assert_eq!(jit.cpu, interpreter.cpu, "step {n}");
            assert_eq!(jit.fault, interpreter.fault, "step {n}");
            assert_eq!(jit.recovering(), interpreter.recovering());
            assert_eq!(jit.stats, interpreter.stats);
            assert_eq!(actual.is_err(), expected.is_err());
            if let (Ok(a), Ok(b)) = (actual, expected) {
                assert_eq!(a, b);
                if matches!(a, Step::Halted | Step::Service(_)) { break; }
            } else { break; }
            assert!(n < 999_999, "program did not stop");
        }
        assert_eq!(jit.objekt, interpreter.objekt);
        assert_eq!(jit.devices.requests, interpreter.devices.requests);
        assert_eq!(jit.devices.completions, interpreter.devices.completions);
        assert_eq!(jit.devices.registers, interpreter.devices.registers);
    }
    Ok(())
}

#[test]
fn native_blocks_preserve_ticks_budget_edits_and_fault_boundaries() -> Result<()> {
    let source = "d=0xffffffff, r=Bus, rb=0, ldrb, flags\n\
        ra=0, rb=0, alu=Add, s=Branch, brch=1, ldrb, flags\n\
        ra=0, rb=1, ldrb, ldq, ldsym, cc=!Zero, seq=ConditionalJump, brch=0\n\
        halt";
    for budget in [1, 2, 3, 7, 256] {
        let mut interpreted = boot::microcode_with_pager(source, 512, 16)?.machine;
        let mut compiled = boot::microcode_with_pager(source, 512, 16)?.machine;
        compiled.enable_jit()?;
        interpreted.devices = rekursiv_emulator::presentation::workstation(0, 2);
        compiled.devices = rekursiv_emulator::presentation::workstation(0, 2);
        for _ in 0..budget {
            if matches!(interpreted.step()?, Step::Halted | Step::Service(_)) {
                break;
            }
        }
        assert!(compiled.run_steps(budget)? <= budget);
        assert_eq!(compiled.cpu, interpreted.cpu);
        assert_eq!(compiled.stats, interpreted.stats);
        compare_devices(&compiled, &interpreted);
    }
    // Mutate a word inside the block, rather than just its entry point.
    let mut m =
        boot::microcode_with_pager("d=1, r=Bus, ldrb\nd=2, r=Bus, rb=1, ldrb\nhalt", 512, 16)?
            .machine;
    m.enable_jit()?;
    m.image_mut().code[1].as_mut().unwrap().data = Word::raw(73)?;
    m.run_steps(2)?;
    assert_eq!(m.cpu.rf[..2], [1, 73]);
    // A later invalid branch must preserve the earlier instruction's commit
    // and must not tick peripherals or publish the failed word's writes.
    let source = "d=17, r=Bus, rb=1, ldrb\nd=99, r=Bus, rb=2, ldrb, seq=Bus";
    let mut a = boot::microcode_with_pager(source, 512, 16)?.machine;
    let mut b = boot::microcode_with_pager(source, 512, 16)?.machine;
    // Bus target 99 is in range but absent; the next fetch faults precisely.
    b.enable_jit()?;
    a.step()?;
    a.step()?;
    assert!(a.step().is_err());
    assert!(b.run_steps(3).is_err());
    assert_eq!(a.cpu, b.cpu);
    assert_eq!(a.fault, b.fault);
    assert_eq!(a.stats, b.stats);
    Ok(())
}

fn compare_devices(a: &Machine, b: &Machine) {
    let clock = |m: &Machine| {
        m.devices.clocks.as_ref().map(|c| {
            (
                c.utc_seconds,
                c.monotonic_ms,
                c.deadline,
                c.day_milliseconds(),
            )
        })
    };
    assert_eq!(clock(a), clock(b));
    let events = |m: &Machine| {
        m.devices
            .events
            .as_ref()
            .map(|e| (e.counts, e.ticks, e.acknowledgements, e.schedule.clone()))
    };
    assert_eq!(events(a), events(b));
    assert_eq!(
        a.devices
            .display_bitmap
            .as_ref()
            .and_then(|d| d.visible.as_ref()),
        b.devices
            .display_bitmap
            .as_ref()
            .and_then(|d| d.visible.as_ref())
    );
    assert_eq!(
        a.devices
            .cursor_bitmap
            .as_ref()
            .and_then(|d| d.visible.as_ref()),
        b.devices
            .cursor_bitmap
            .as_ref()
            .and_then(|d| d.visible.as_ref())
    );
}

#[test]
#[ignore = "requires the pinned Xerox distribution; set REKURSIV_ST80_DIR"]
fn saved_image_jit_matches_interpreter_with_batched_execution() -> Result<()> {
    let path = std::path::PathBuf::from(std::env::var("REKURSIV_ST80_DIR")?).join("VirtualImage");
    let bytes = std::fs::read(path)?;
    let mut a = boot::smalltalk_saved_display(&bytes, 131072)?.machine;
    let mut b = boot::smalltalk_saved_display(&bytes, 131072)?.machine;
    a.devices = rekursiv_emulator::presentation::workstation(0, 1000);
    b.devices = rekursiv_emulator::presentation::workstation(0, 1000);
    a.objekt_metrics_enabled = true;
    b.objekt_metrics_enabled = true;
    b.enable_jit()?;
    // Small RAM exercises machine GC; the saved display exercises BitBlt.
    // Each batch compares every CPU field and all architectural counters.
    for batch in 0..20000 {
        if batch == 4000 || batch == 8000 {
            for m in [&mut a, &mut b] {
                rekursiv_emulator::presentation::pointer(&mut m.devices, 200 + batch / 100, 160);
                rekursiv_emulator::presentation::key(&mut m.devices, 130, batch == 4000)?;
            }
        }
        for _ in 0..256 {
            a.step()?;
        }
        assert_eq!(b.run_steps(256)?, 256);
        assert_eq!(a.cpu, b.cpu);
        assert_eq!(a.stats, b.stats);
        assert_eq!(a.fault, b.fault);
    }
    assert!(b.stats.collections > 0);
    assert!(b.devices.display_bitmap.as_ref().unwrap().publications > 2);
    assert_eq!(a.objekt, b.objekt);
    compare_devices(&a, &b);
    Ok(())
}

#[test]
fn a_block_fault_drains_an_older_object_fault_first() -> Result<()> {
    for index in ["Clear", "Two"] {
        let source = format!(
            "d=0xa000000064, page=Allocate, size=2, scan=0\n\
            idx={index}, prepare\nmem=Read, prepared, launch\n\
            d=17, r=Bus, rb=1, ldrb\nd=8192, seq=Bus, r=Bus, rb=2, ldrb"
        );
        let mut a = boot::microcode_with_pager(&source, 512, 16)?.machine;
        let mut b = boot::microcode_with_pager(&source, 512, 16)?.machine;
        b.enable_jit()?;
        for _ in 0..4 {
            a.step()?;
        }
        assert!(a.step().is_err());
        assert!(b.run_steps(5).is_err());
        assert_eq!(a.cpu, b.cpu);
        assert_eq!(a.fault, b.fault);
        assert_eq!(a.stats, b.stats);
        assert_eq!(a.objekt, b.objekt);
        assert_eq!(b.jit_statistics().unwrap().block_instructions, 2);
        assert_eq!(b.fault.unwrap().code, if index == "Clear" { 4 } else { 2 });
    }
    Ok(())
}

#[test]
fn peripheral_errors_leave_the_failing_block_instruction_uncommitted() -> Result<()> {
    let source = "d=17, r=Bus, rb=1, ldrb\nd=73, r=Bus, rb=2, ldrb\nhalt";
    let mut a = boot::microcode_with_pager(source, 512, 16)?.machine;
    let mut b = boot::microcode_with_pager(source, 512, 16)?.machine;
    for m in [&mut a, &mut b] {
        let mut events = rekursiv_devices::Events::default();
        events.counts[0] = u32::MAX;
        events.schedule.insert(1, 1);
        m.devices.events = Some(events);
    }
    b.enable_jit()?;
    a.step()?;
    let expected = a.step().unwrap_err();
    let actual = b.run_steps(2).unwrap_err();
    assert_eq!(actual.to_string(), expected.to_string());
    assert_eq!(a.cpu, b.cpu);
    assert_eq!(a.stats, b.stats);
    assert_eq!(a.fault, b.fault);
    assert!(b.fault.is_none());
    assert_eq!(b.cpu.rf[..3], [0, 17, 0]);
    compare_devices(&a, &b);
    Ok(())
}

#[test]
fn native_block_destinations_use_old_operands_and_flags() -> Result<()> {
    let mut seed = 0x24723374655u64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let alus = [
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
    ];
    let shifts = [
        Shift::None,
        Shift::Left,
        Shift::Right,
        Shift::ArithmeticRight,
    ];
    let carries = [Carry::Zero, Carry::One, Carry::ZeroFlag];
    let mut code = Vec::new();
    for n in 0..256 {
        let bits = random();
        code.push(Instruction {
            alu: alus[n % alus.len()],
            r: Source::Register,
            s: Source::Q,
            ra: (bits & 15) as u8,
            rb: ((bits >> 4) & 15) as u8,
            write_register: true,
            load_q: bits & 256 != 0,
            flags: true,
            symbol: true,
            mark: bits & 512 != 0,
            bus: Bus::Register,
            condition: Condition::Zero,
            invert: bits & 1024 != 0,
            shift: shifts[n % shifts.len()],
            carry: carries[n % carries.len()],
            seq: if n % 16 == 15 {
                Seq::TwoWay
            } else {
                Seq::Continue
            },
            branch: if bits & 2048 != 0 { 8192 } else { 256 },
            ..Instruction::default()
        });
    }
    code.push(Instruction::halt());
    let image = Image::program(&code);
    let mut a = Machine::new(image.clone(), 0, 16, 512)?;
    let mut b = Machine::new(image, 0, 16, 512)?;
    b.enable_jit()?;
    for case in 0..256 {
        let mut cpu = rekursiv_model::processor::Processor {
            pc: ((case % 16) * 16) as u16,
            q: random() as u32,
            flags: (random() & 31) as u8,
            product: random(),
            sp: if case % 31 == 0 { 32 } else { 0 },
            apc: if case % 37 == 0 { 0x1000000 } else { 0 },
            roots: Some(a.image().roots),
            ..Default::default()
        };
        for r in &mut cpu.rf {
            *r = random() as u32;
        }
        a.cpu = cpu.clone();
        b.cpu = cpu;
        a.fault = None;
        b.fault = None;
        a.stats = Default::default();
        b.stats = Default::default();
        let mut expected = Ok(Step::Retired);
        for _ in 0..16 {
            expected = a.step();
            if expected.is_err() {
                break;
            }
        }
        let actual = b.run_steps(16);
        assert_eq!(actual.is_err(), expected.is_err(), "case {case}");
        assert_eq!(a.cpu, b.cpu, "case {case}");
        assert_eq!(a.stats, b.stats, "case {case}");
        assert_eq!(a.fault, b.fault, "case {case}");
    }
    assert!(b.jit_statistics().unwrap().block_instructions > 1000);
    Ok(())
}

#[test]
fn image_edits_invalidate_overlapping_blocks_but_keep_unrelated_code() -> Result<()> {
    let mut m = boot::microcode_with_pager(
        "d=1, r=Bus, ldrb\nd=2, r=Bus, rb=1, ldrb\nd=3, r=Bus, rb=2, ldrb\nhalt",
        512,
        16,
    )?
    .machine;
    let second = m.image().code[..4].to_vec();
    m.image_mut().code[8..12].copy_from_slice(&second);
    m.enable_jit()?;
    // Invalidate blocks beginning at 0 and 1 as well as the edited word at 2.
    m.image_mut().code[2].as_mut().unwrap().data = Word::raw(73)?;
    m.run_steps(3)?;
    assert_eq!(m.cpu.rf[..3], [1, 2, 73]);
    assert_eq!(m.jit_statistics().unwrap().block_instructions, 0);
    m.cpu.pc = 8;
    m.run_steps(3)?;
    assert_eq!(m.cpu.rf[..3], [1, 2, 3]);
    assert_eq!(m.jit_statistics().unwrap().block_instructions, 3);
    // Read access does not invalidate either cache. Table-only edits preserve
    // blocks too, while their new values remain visible to fetch instructions.
    assert!(m.image().code[8].is_some());
    m.image_mut().map[0] = Some(8);
    m.cpu.pc = 8;
    m.run_steps(3)?;
    assert_eq!(m.jit_statistics().unwrap().block_instructions, 6);
    Ok(())
}

#[test]
fn replacing_and_resizing_the_image_cannot_execute_stale_code() -> Result<()> {
    for jit in [false, true] {
        for batched in [false, true] {
            let mut m = boot::microcode_with_pager("d=11, r=Bus, ldrb\nhalt", 512, 16)?.machine;
            if jit {
                m.enable_jit()?;
            }
            let original = m.image().code[0].unwrap();
            *m.image_mut() = Image::program(&[Instruction {
                data: Word::raw(29)?,
                ..original
            }]);
            if batched {
                m.run_steps(1)?;
            } else {
                m.step()?;
            }
            assert_eq!(m.cpu.rf[0], 29);
            // This removes both the next word and all old native block ranges.
            m.image_mut().code.clear();
            let result = if batched {
                m.run_steps(4).map(|_| ())
            } else {
                m.step().map(|_| ())
            };
            assert!(result.is_err());
            assert_eq!(m.fault.unwrap().code, 2);
            assert_eq!(m.cpu.rf[0], 29);
            // New words must be decoded even when no old cache entry exists.
            m.image_mut().code = vec![Some(Instruction {
                data: Word::raw(47)?,
                ..original
            })];
            m.cpu.pc = 0;
            m.cpu.halted = false;
            m.fault = None;
            if batched {
                m.run_steps(1)?;
            } else {
                m.step()?;
            }
            assert_eq!(m.cpu.rf[0], 47);
            if jit {
                m.cpu.pc = 0;
                m.image_mut().code[0].as_mut().unwrap().data = Word::raw(59)?;
                // Recompilation before the next execution must use the new image.
                m.enable_jit()?;
                m.run_steps(1)?;
                assert_eq!(m.cpu.rf[0], 59);
                assert_eq!(m.jit_statistics().unwrap().block_instructions, 1);
            }
        }
    }
    Ok(())
}

#[test]
fn resident_blocks_and_fetch_fallback_preserve_each_committed_prefix() -> Result<()> {
    for source in [
        include_str!("../../../microcode/bench/resident.uc"),
        include_str!("../../../microcode/collection.uc"),
        // The invalid Fetch follows completed resident commands and local writes.
        // Its fallback must retain those effects without issuing them again.
        "d=0xa000000064, page=Allocate, size=2, scan=0\n\
         idx=Two\nd=23, mem=Write\nmem=Read\n\
         d=Object, r=Bus, rb=1, ldrb\n\
         d=0x8000000011, page=Fetch\nhalt",
    ] {
        for budget in [1, 7, 256] {
            let mut expected = boot::microcode_with_pager(source, 512, 16)?.machine;
            let mut actual = boot::microcode_with_pager(source, 512, 16)?.machine;
            actual.enable_jit()?;
            for _ in 0..8 {
                let mut failed = false;
                for _ in 0..budget {
                    match expected.step() {
                        Err(_) => {
                            failed = true;
                            break;
                        }
                        Ok(Step::Halted | Step::Service(_)) => break,
                        Ok(_) => (),
                    }
                }
                assert_eq!(actual.run_steps(budget).is_err(), failed);
                assert_eq!(actual.cpu, expected.cpu);
                assert_eq!(actual.stats, expected.stats);
                assert_eq!(actual.fault, expected.fault);
                assert_eq!(actual.objekt, expected.objekt);
                assert_eq!(actual.recovering(), expected.recovering());
                compare_devices(&actual, &expected);
                if failed || actual.cpu.halted || actual.cpu.service {
                    break;
                }
            }
        }
    }
    Ok(())
}
