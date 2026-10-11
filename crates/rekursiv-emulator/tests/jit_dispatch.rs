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
