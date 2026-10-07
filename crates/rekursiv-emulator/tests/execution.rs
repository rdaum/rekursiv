use eyre::{ensure, Result};
use rekursiv_asm::{processor::*, Command, Word};
use rekursiv_emulator::{boot, presentation, Machine, Step};
use rekursiv_model::processor::{Image, Processor};
use rekursiv_sim::{Harness, Timing};

#[test]
fn native_retirements_and_heaps_match_rtl_across_collection_and_paging() -> Result<()> {
    let runtime = rekursiv_sim::runtime()?;
    // Expected behavior belongs to the fixture, not to substrings that can
    // also appear in microcode comments.
    enum Scenario {
        Pressure,
        Paging,
        Explicit,
    }
    for (scenario, source) in [
        (
            Scenario::Pressure,
            include_str!("../../../microcode/collection.uc"),
        ),
        (
            Scenario::Paging,
            include_str!("../../../microcode/allocation.uc"),
        ),
        // A proactive pass on an empty heap, then repeated passes retaining a
        // 200-word live object. The last transfer's size must not reserve an
        // extra 200 words at Commit: this collection needs zero extra space.
        (
            Scenario::Explicit,
            "gc=Collect
         d=0xa000000064, page=Allocate, size=200, scan=0
         d=Object, ldvr, vr=0
         d=1, idx=Load
         d=73, mem=Write
         d=1234, r=Bus, rb=7, ldrb, ldq, flags
         d=73, estk=Bus, ldsym
         gc=Collect
         gc=Collect
         read=FreeWords
         d=Object, r=Bus, rb=4, ldrb
         halt",
        ),
    ] {
        let mut loaded = boot::microcode_with_pager(source, 512, 16)?;
        let machine = &mut loaded.machine;
        let mut h = Harness::new(
            &runtime,
            Timing {
                request_delay: 2,
                memory_latency: 3,
                response_stall: 2,
            },
            None,
        )?;
        h.load_processor(machine.image())?;
        h.start_processor(machine.cpu.pc)?;
        let mut oracle = Processor::default();
        h.run_processor_observed(
            &machine.image().clone(),
            &mut oracle,
            5_000_000,
            |h, expected| {
                let mut completed = false;
                for _ in 0..200_000 {
                    if machine.step()? == Step::Retired && !machine.recovering() {
                        completed = true;
                        break;
                    }
                }
                ensure!(completed, "emulator failed to reach next retirement");
                ensure!(
                    machine.cpu == *expected,
                    "CPU differs at micro-PC {}",
                    expected.pc
                );
                ensure!(
                    machine.objekt == h.oracle,
                    "object state differs at micro-PC {}",
                    expected.pc
                );
                Ok(())
            },
        )?;
        assert!(machine.cpu.halted);
        assert_eq!(machine.stats.collections, h.stats.collections);
        if matches!(scenario, Scenario::Explicit) {
            assert_eq!(machine.stats.collections, 3);
            assert_eq!(machine.cpu.rf[4], 56);
            assert_eq!(machine.cpu.rf[7], 1234);
            assert_eq!(machine.cpu.estkr, 73);
            assert_eq!(machine.objekt.next_identity, 2);
        } else if matches!(scenario, Scenario::Pressure) {
            assert_eq!(machine.stats.collections, 5);
            assert!(machine.stats.collector_retired > 200_000);
        } else {
            assert_eq!(machine.cpu.estkr, Word::signed(1234).bits());
        }
    }
    Ok(())
}

#[test]
fn collection_requires_a_handler_and_cannot_nest() -> Result<()> {
    let runtime = rekursiv_sim::runtime()?;
    for nested in [false, true] {
        let collect = Instruction {
            recovery: Recovery::Collect,
            ..Default::default()
        };
        let mut image = Image::program(&[collect, Instruction::halt()]);
        if nested {
            image = image.with_ram_collector(128, 16)?;
            image.code[128] = Some(collect);
        }
        let mut m = Machine::new(image.clone(), 0, 16, 512)?;
        if nested {
            assert_eq!(m.step()?, Step::RecoveryEntered);
        }
        assert!(m.step().is_err());
        assert_eq!(m.cpu.pc, 0);
        assert_eq!(m.fault.unwrap().code, 1);
        assert_eq!(m.stats.collections, 0);
        assert_eq!(m.stats.retired, 0);
        let mut h = Harness::new(&runtime, Timing::default(), None)?;
        h.load_processor(&image)?;
        h.start_processor(0)?;
        for _ in 0..20 {
            h.tick()?;
            if h.rtl.cpu_halted_o != 0 {
                break;
            }
        }
        assert_ne!(h.rtl.cpu_halted_o, 0);
        assert_eq!(h.rtl.cpu_fault_o, 1);
        assert_eq!(h.rtl.cpu_pc_o, 0);
        assert_eq!(h.stats.commands, 0);
        assert_eq!(h.stats.store_transactions, 0);
    }
    Ok(())
}

#[test]
fn malformed_collection_request_cannot_enter_recovery() -> Result<()> {
    let collect = Instruction {
        recovery: Recovery::Collect,
        ..Default::default()
    };
    let image = Image::program(&[collect, Instruction::halt()]).with_ram_collector(128, 16)?;
    let mut m = Machine::new(image.clone(), 0, 16, 512)?;
    m.image_mut().code[0].as_mut().unwrap().data = Word::raw(1)?;
    assert!(m.step().is_err());
    assert_eq!(m.fault.unwrap().code, 1);
    assert!(!m.recovering());
    let runtime = rekursiv_sim::runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    h.load_processor(&image)?;
    // Bypass the assembler's rejection to exercise hardware validation.
    h.program_lane(0, 0, 0, 1)?;
    h.start_processor(0)?;
    for _ in 0..10 {
        h.tick()?;
        assert_eq!(h.rtl.cpu_gc_active_o, 0);
        if h.rtl.cpu_halted_o != 0 {
            break;
        }
    }
    assert_eq!(h.rtl.cpu_fault_o, 1);
    assert_eq!(h.rtl.cpu_pc_o, 0);
    assert_eq!(h.stats.commands, 0);
    Ok(())
}

#[test]
fn device_failure_does_not_retire_local_writes() -> Result<()> {
    let image = Image::program(&[Instruction {
        device: Device::Write,
        data: Word::raw(42)?,
        r: Source::Bus,
        write_register: true,
        rb: 2,
        ..Default::default()
    }]);
    let mut m = Machine::new(image, 0, 16, 512)?;
    m.cpu.rf[0] = 0x1000;
    m.devices.registers.insert(0x1000, 3);
    m.devices.fail_next = true;
    let error = m.step().unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<rekursiv_emulator::Fault>()
            .unwrap()
            .code,
        6
    );
    assert_eq!(m.cpu.rf[2], 0);
    assert_eq!(m.cpu.pc, 0);
    assert_eq!(m.devices.registers[&0x1000], 3);
    assert_eq!(m.stats.retired, 0);
    Ok(())
}

#[test]
fn edited_control_words_cannot_use_stale_native_decoding() -> Result<()> {
    let original = Instruction {
        data: Word::raw(11)?,
        r: Source::Bus,
        write_register: true,
        rb: 3,
        ..Instruction::default()
    };
    for replacement in [
        Some(Instruction {
            data: Word::raw(29)?,
            ..original
        }),
        Some(Instruction {
            estk: Estk::Bus,
            ..original
        }),
        Some(Instruction { ra: 16, ..original }),
        None,
    ] {
        let mut m = Machine::new(Image::program(&[original]), 0, 16, 512)?;
        m.image_mut().code[0] = replacement;
        let before = m.cpu.clone();
        match before.prepare(m.image(), false) {
            Ok((expected, _)) => {
                assert_eq!(m.step()?, Step::Retired);
                assert_eq!(m.cpu, expected);
            }
            Err(code) => {
                assert!(m.step().is_err());
                assert_eq!(m.fault.unwrap().code, code);
                let mut expected = before;
                expected.halted = true;
                assert_eq!(m.cpu, expected);
                assert_eq!(m.stats.retired, 0);
            }
        }
    }
    Ok(())
}

#[test]
fn scalar_object_failure_discards_all_local_writes() -> Result<()> {
    let i = Instruction {
        data: Word::raw(42)?,
        r: Source::Bus,
        write_register: true,
        rb: 3,
        load_q: true,
        flags: true,
        symbol: true,
        mark: true,
        seq: Seq::Jump,
        branch: 7,
        object: Some(Command::read_field()),
        ..Instruction::default()
    };
    let mut m = Machine::new(Image::program(&[i]), 0, 16, 512)?;
    let mut expected = m.cpu.clone();
    expected.halted = true;
    assert!(m.step().is_err());
    assert_eq!(m.fault.unwrap().code, 4);
    assert_eq!(m.cpu, expected);
    assert_eq!(m.stats.retired, 0);
    Ok(())
}

// Exercise every indexed destination together: pending writes must neither
// become visible on a failed request nor use a newly written pointer/register.
fn simultaneous_writes() -> Instruction {
    Instruction {
        data: Word::raw(12).unwrap(),
        ra: 0,
        rb: 0,
        r: Source::Bus,
        write_register: true,
        write_root: true,
        load_q: true,
        flags: true,
        symbol: true,
        mark: true,
        sp: Pointer::Increment,
        esp: Address::Sp,
        csp: Pointer::Increment,
        estk: Estk::Bus,
        cstk: Cstk::Ap,
        load_ap: true,
        apc: Apc::Bus,
        ..Default::default()
    }
}

#[test]
fn failed_operations_discard_all_pending_destinations() -> Result<()> {
    for failure in 0..3 {
        let mut instruction = simultaneous_writes();
        match failure {
            0 => instruction.device = Device::Write,
            1 => instruction.object = Some(Command::read_field()),
            _ => instruction.fetch = Fetch::Nam, // Unpopulated NAM: late local fault.
        }
        instruction.validate()?;
        let mut m = Machine::new(Image::program(&[instruction]), 0, 16, 512)?;
        m.cpu.rf[0] = 4;
        m.cpu.esp = 7;
        m.cpu.csp = 9;
        m.cpu.ap = 6;
        m.cpu.flags = 31;
        m.cpu.estk.fill(123);
        m.cpu.cstk.fill(456);
        m.devices.fail_next = true;
        let mut expected = m.cpu.clone();
        expected.halted = true;
        assert!(m.step().is_err());
        assert_eq!(m.cpu, expected, "failure {failure}");
        assert_eq!(m.stats.retired, 0);
    }
    Ok(())
}

#[test]
fn pending_destinations_use_original_operands_and_addresses() -> Result<()> {
    let mut m = Machine::new(Image::program(&[simultaneous_writes()]), 0, 16, 512)?;
    m.cpu.rf[0] = 4;
    m.cpu.esp = 7;
    m.cpu.csp = 9;
    m.cpu.ap = 6;
    let mut expected = m.cpu.clone();
    expected.pc = 1;
    expected.rf[0] = 12;
    expected.roots.as_mut().unwrap()[4] = Word::raw(12)?;
    expected.q = 12;
    expected.symbol = 12;
    expected.lastcc = true;
    expected.sp = 1;
    expected.esp = 1;
    expected.csp = 10;
    expected.estk[7] = 12;
    expected.estkr = 12;
    expected.cstk[9] = 6;
    expected.cstkr = 6;
    expected.ap = 12;
    expected.apc = 12;
    assert_eq!(m.step()?, Step::Retired);
    assert_eq!(m.cpu, expected);
    Ok(())
}

#[test]
fn first_root_read_and_write_are_deferred_until_commit() -> Result<()> {
    let mut image = Image::program(&[Instruction {
        bus: Bus::Root,
        write_root: true,
        write_register: true,
        r: Source::Bus,
        rb: 1,
        ..Default::default()
    }]);
    image.roots[0] = Word::signed(27);
    let mut cpu = Processor::default();
    let (writes, _) = cpu.prepare_writes(&image, false).unwrap();
    assert!(cpu.roots.is_none());
    writes.commit(&mut cpu, &image);
    assert_eq!(cpu.roots, Some(image.roots));
    assert_eq!(cpu.rf[1], 27);
    Ok(())
}

#[test]
fn collector_failure_preserves_source_heap_and_interrupted_registers() -> Result<()> {
    let class = Word::reference(100, true)?;
    let program = Image::program(&[Instruction {
        object: Some(Command::allocate(class, 257, false)?),
        data: class,
        r: Source::Bus,
        write_register: true,
        rb: 9,
        ..Default::default()
    }])
    .with_ram_collector(3968, 16)?;
    let mut m = Machine::new(program, 0, 16, 512)?;
    let memory = m.objekt.memory.clone();
    let result: Result<()> = (|| {
        for _ in 0..100_000 {
            m.step()?;
        }
        Ok(())
    })();
    assert!(result.is_err());
    assert_eq!(m.cpu.pc, 0);
    assert_eq!(m.cpu.rf[9], 0);
    assert_eq!(
        m.fault.unwrap().status,
        Some(rekursiv_asm::Status::OutOfSpace)
    );
    assert_eq!(m.objekt.body_cursor, 0);
    assert_eq!(m.objekt.memory, memory);
    assert_eq!(m.objekt.next_identity, 1);
    Ok(())
}

#[test]
fn window_demo_publishes_pixels_and_consumes_physical_input_via_microcode() -> Result<()> {
    let mut m = boot::microcode(include_str!("../../../microcode/workstation.uc"), 512)?.machine;
    m.devices = presentation::workstation(0, 1000);
    for _ in 0..20_000 {
        m.step()?;
        if m.devices.display_bitmap.as_ref().unwrap().publications == 1 {
            break;
        }
    }
    let initial = m
        .devices
        .display_bitmap
        .as_ref()
        .unwrap()
        .visible
        .clone()
        .unwrap();
    assert_eq!((initial.width, initial.height), (320, 240));
    assert!(initial.words.iter().all(|&w| w == 0xaaaa5555));
    presentation::pointer(&mut m.devices, 71, 83);
    assert!(presentation::key(&mut m.devices, 97, true)?);
    assert!(presentation::key(&mut m.devices, 97, false)?);
    for _ in 0..30_000 {
        m.step()?;
        if m.devices.display_bitmap.as_ref().unwrap().publications == 2
            && m.devices.input.as_ref().unwrap().is_empty()
            && m.devices.events.as_ref().unwrap().counts[0] == 0
        {
            break;
        }
    }
    let display = m.devices.display_bitmap.as_ref().unwrap();
    assert_eq!(display.publications, 2);
    assert!(display
        .visible
        .as_ref()
        .unwrap()
        .words
        .iter()
        .zip(&initial.words)
        .all(|(new, old)| *new == !old));
    assert_eq!(m.devices.pointer.as_ref().unwrap().cursor, (71, 83));
    assert!(m.devices.input.as_ref().unwrap().is_empty());
    assert_eq!(m.devices.events.as_ref().unwrap().counts[0], 0);
    assert!(
        m.devices.requests.is_empty(),
        "interactive history must be bounded"
    );
    Ok(())
}

#[test]
fn framebuffer_padding_and_cursor_edges_are_clipped() {
    use rekursiv_devices::BitmapFrame;
    let frame = BitmapFrame {
        width: 3,
        height: 2,
        stride: 1,
        words: vec![0xa0000000, 0x40000000],
    };
    let cursor = BitmapFrame {
        width: 2,
        height: 2,
        stride: 1,
        words: vec![0xc0000000; 2],
    };
    assert_eq!(
        presentation::pixels(&frame, None),
        vec![0, 0xffffff, 0, 0xffffff, 0, 0xffffff]
    );
    assert_eq!(
        presentation::pixels(&frame, Some((&cursor, (-1, -1)))),
        vec![0xffffff, 0xffffff, 0, 0xffffff, 0, 0xffffff]
    );
    assert_eq!(
        presentation::pixels(&frame, Some((&cursor, (2, 1)))),
        vec![0, 0xffffff, 0, 0xffffff, 0, 0]
    );
}
