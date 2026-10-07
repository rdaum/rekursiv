use eyre::{ensure, Result};
use rekursiv_asm::{processor::*, Command, Word};
use rekursiv_emulator::{boot, presentation, Machine, Step};
use rekursiv_model::processor::{Image, Processor};
use rekursiv_sim::{Harness, Timing};

#[test]
fn native_retirements_and_heaps_match_rtl_across_collection_and_paging() -> Result<()> {
    let runtime = rekursiv_sim::runtime()?;
    for source in [
        include_str!("../../../microcode/collection.uc"),
        include_str!("../../../microcode/allocation.uc"),
    ] {
        let mut loaded = boot::microcode(source, 512)?;
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
        h.load_processor(&machine.image)?;
        h.start_processor(machine.cpu.pc)?;
        let mut oracle = Processor::default();
        h.run_processor_observed(
            &machine.image.clone(),
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
        if source.contains("size=120") {
            assert_eq!(machine.stats.collections, 5);
            assert!(machine.stats.collector_retired > 200_000);
        } else {
            assert_eq!(machine.cpu.estkr, Word::signed(1234).bits());
        }
    }
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
