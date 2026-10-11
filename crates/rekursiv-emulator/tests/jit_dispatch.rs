use eyre::Result;
use rekursiv_asm::{Index, INDEX_MAX};
use rekursiv_emulator::{boot, Machine};

// Bus transfers deliberately end regions, so the native dispatcher must join
// two-word regions rather than merely unrolling a static microcode loop.
const LOOP: &str =
    "read=Index, idx=Increment\nd=4, seq=Bus\nhalt\nhalt\nread=Index, idx=Increment\nd=0, seq=Bus";
fn pair() -> Result<(Machine, Machine)> {
    let mut a = boot::microcode_with_pager(LOOP, 512, 16)?.machine;
    let mut b = boot::microcode_with_pager(LOOP, 512, 16)?.machine;
    a.enable_guest_counter(0)?;
    b.enable_guest_counter(0)?;
    b.enable_jit_eager()?;
    Ok((a, b))
}
fn same(a: &Machine, b: &Machine) {
    assert_eq!(a.cpu, b.cpu);
    assert_eq!(a.stats, b.stats);
    assert_eq!(a.objekt, b.objekt);
    assert_eq!(a.fault, b.fault);
    assert_eq!(a.devices.idle_budget(), b.devices.idle_budget());
    if let (Some(a), Some(b)) = (&a.devices.clocks, &b.devices.clocks) {
        assert_eq!(a.monotonic_ms, b.monotonic_ms);
        assert_eq!(a.deadline, b.deadline);
    }
    if let (Some(a), Some(b)) = (&a.devices.events, &b.devices.events) {
        assert_eq!(a.counts, b.counts);
        assert_eq!(a.ticks, b.ticks);
    }
}
#[test]
fn chained_regions_preserve_budgets_events_and_wide_counts() -> Result<()> {
    for divider in [0, 1, 17, 10000] {
        let (mut a, mut b) = pair()?;
        if divider != 0 {
            for m in [&mut a, &mut b] {
                m.devices = rekursiv_emulator::presentation::workstation(0, divider);
                m.devices.clocks.as_mut().unwrap().deadline = Some(3);
                m.devices.events.as_mut().unwrap().schedule.insert(71, 4);
            }
        }
        for budget in [1, 2, 3, 31, 32, 255, 1024, 1025, 4097] {
            assert_eq!(a.run_steps(budget)?, b.run_steps(budget)?);
            same(&a, &b);
        }
        let stats = b.jit_statistics().unwrap();
        if divider == 0 {
            assert!(stats.block_calls > stats.native_entries * 8, "{stats:?}");
            assert!(b.stats.guest_bytecodes > 255);
            assert!(b.stats.object_commands > 255);
        }
        if divider == 1 {
            assert_eq!(stats.block_calls, stats.native_entries);
        }
    }
    Ok(())
}
#[test]
fn later_chain_faults_keep_successful_prefix_and_object_effects() -> Result<()> {
    for device_error in [false, true] {
        let (mut a, mut b) = pair()?;
        for m in [&mut a, &mut b] {
            if device_error {
                let mut events = rekursiv_devices::Events::default();
                events.counts[0] = u32::MAX;
                events.schedule.insert(71, 1);
                m.devices.events = Some(events);
            } else {
                m.objekt.state.index = INDEX_MAX - 1;
            }
        }
        let expected = a.run_steps(256).unwrap_err();
        let actual = b.run_steps(256).unwrap_err();
        assert_eq!(actual.to_string(), expected.to_string());
        same(&a, &b);
        let stats = b.jit_statistics().unwrap();
        assert!(stats.block_calls > stats.native_entries, "{stats:?}");
    }
    Ok(())
}
#[test]
fn dispatch_table_invalidation_stops_at_edited_and_missing_targets() -> Result<()> {
    let (mut a, mut b) = pair()?;
    a.run_steps(128)?;
    b.run_steps(128)?;
    same(&a, &b);
    let entries = b.jit_statistics().unwrap().native_entries;
    assert_eq!(entries, 1);
    for m in [&mut a, &mut b] {
        m.image_mut().code[4]
            .as_mut()
            .unwrap()
            .object
            .as_mut()
            .unwrap()
            .index = Index::Decrement;
    }
    a.run_steps(128)?;
    b.run_steps(128)?;
    same(&a, &b);
    // Shrinking the directory must also invalidate native source dependencies.
    for m in [&mut a, &mut b] {
        m.image_mut().code.truncate(4);
    }
    assert!(a.run_steps(128).is_err());
    assert!(b.run_steps(128).is_err());
    same(&a, &b);
    assert_eq!(b.fault.unwrap().code, 2);
    Ok(())
}

#[test]
fn native_roots_preserve_full_words_and_old_operands() -> Result<()> {
    use rekursiv_asm::{processor::*, Command, Read, Word};
    use rekursiv_model::processor::Image;
    let image = Image::program(&[
        Instruction {
            bus: Bus::Symbol,
            write_root: true,
            ra: 0,
            rb: 0,
            r: Source::Bus,
            write_register: true,
            ..Instruction::default()
        },
        Instruction {
            bus: Bus::Root,
            ra: 0,
            write_root: true,
            symbol: true,
            ..Instruction::default()
        },
        Instruction {
            bus: Bus::Root,
            ra: 1,
            symbol: true,
            object: Some(Command {
                read: Read::Vr,
                load_vr: true,
                vr: 2,
                ..Command::default()
            }),
            ..Instruction::default()
        },
        Instruction::halt(),
    ]);
    let mut native = Machine::new(image.clone(), 0, 16, 64)?;
    native.enable_jit_eager()?;
    for initial_roots in [false, true] {
        for index in [0, 7, 31] {
            let mut reference = Machine::new(image.clone(), 0, 16, 64)?;
            reference.cpu.roots = initial_roots.then_some([Word::signed(19); 32]);
            reference.cpu.symbol = Word::from_bits((0xa5 << 32) | 3)?.bits();
            reference.cpu.rf[0] = index;
            reference.cpu.rf[1] = index;
            native.cpu = reference.cpu.clone();
            native.objekt = reference.objekt.clone();
            native.stats = reference.stats;
            reference.run_steps(3)?;
            native.run_steps(3)?;
            same(&reference, &native);
            assert_eq!(
                native.cpu.roots.unwrap()[index as usize].bits(),
                (0xa5 << 32) | 3
            );
        }
    }
    assert!(native.jit_statistics().unwrap().block_instructions > 0);
    Ok(())
}

#[test]
fn root_faults_precede_effects_and_failed_ticks_do_not_publish_roots() -> Result<()> {
    use rekursiv_asm::{processor::*, Command, Word};
    use rekursiv_model::processor::Image;
    for index in [31, 32, u32::MAX] {
        for write in [false, true] {
            let image = Image::program(&[
                Instruction::default(),
                Instruction {
                    bus: if write { Bus::Immediate } else { Bus::Root },
                    data: Word::signed(99),
                    write_root: write,
                    // Invalid root bounds must win even over an invalid PC.
                    seq: if index < 32 { Seq::Continue } else { Seq::Jump },
                    branch: 8192,
                    object: Some(Command {
                        index: Index::Increment,
                        ..Command::default()
                    }),
                    ..Instruction::default()
                },
                Instruction::halt(),
            ]);
            let mut a = Machine::new(image.clone(), 0, 16, 64)?;
            let mut b = Machine::new(image, 0, 16, 64)?;
            b.enable_jit_eager()?;
            for m in [&mut a, &mut b] {
                m.cpu.rf[0] = index;
                let mut events = rekursiv_devices::Events::default();
                events.counts[0] = u32::MAX;
                events.schedule.insert(1, 1);
                m.devices.events = Some(events);
            }
            let expected = a.run_steps(3).unwrap_err();
            let actual = b.run_steps(3).unwrap_err();
            assert_eq!(actual.to_string(), expected.to_string());
            same(&a, &b);
            assert_eq!(b.cpu.roots.unwrap(), [Word::ZERO; 32]);
            assert_eq!(b.objekt.state.index, i64::from(index < 32));
            if index >= 32 {
                assert_eq!(b.fault.unwrap().code, 1);
            }
            assert!(b.jit_statistics().unwrap().native_entries > 0);
        }
    }
    Ok(())
}

#[test]
fn native_interrupt_checks_sample_before_device_edges() -> Result<()> {
    let source = "seq=ConditionalJump, cc=Interrupt, brch=irq\nr=Bus, d=1, ldrb, rb=1\nd=0, seq=Bus\nirq: r=Bus, d=2, ldrb, rb=1\nd=0, seq=Bus";
    let mut b = boot::microcode_with_pager(source, 512, 16)?.machine;
    b.enable_jit_eager()?;
    for event_tick in [1, 2, 31, 32, 33, 1023, 1024] {
        for pending in [false, true] {
            let mut a = boot::microcode_with_pager(source, 512, 16)?.machine;
            b.cpu = a.cpu.clone();
            b.stats = a.stats;
            for m in [&mut a, &mut b] {
                m.devices = rekursiv_devices::Device::default();
                let mut events = rekursiv_devices::Events::default();
                events.counts[0] = u32::from(pending);
                events.schedule.insert(event_tick, 1);
                m.devices.events = Some(events);
            }
            for budget in [1, 2, 29, 32, 1024, 4097] {
                a.run_steps(budget)?;
                b.run_steps(budget)?;
                same(&a, &b);
            }
            // A host mutation between calls must be visible to the next chain.
            a.devices.events.as_mut().unwrap().counts.fill(0);
            b.devices.events.as_mut().unwrap().counts.fill(0);
            a.run_steps(64)?;
            b.run_steps(64)?;
            same(&a, &b);
        }
    }
    assert!(b.jit_statistics().unwrap().block_calls > b.jit_statistics().unwrap().native_entries);
    Ok(())
}
