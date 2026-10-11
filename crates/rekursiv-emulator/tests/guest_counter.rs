//! Guest dispatch counts observe successful microcode retirement, independent
//! of execution engine, batching, collection, and native block exits.
use eyre::Result;
use rekursiv_emulator::{boot, Machine, Statistics, Step};
use std::time::Duration;

fn machine(source: &str, pc: u16, jit: bool) -> Result<Machine> {
    let mut m = boot::microcode_with_pager(source, 512, 16)?.machine;
    m.enable_guest_counter(pc)?;
    if jit {
        m.enable_jit()?;
    }
    Ok(m)
}

/// The reference counts observed successful steps, not the counter under test.
fn compare(source: &str, pc: u16, budget: u64, batches: usize) -> Result<Machine> {
    let mut reference = machine(source, pc, false)?;
    let mut native = machine(source, pc, true)?;
    let mut observed = 0;
    for _ in 0..batches {
        let mut failed = false;
        for _ in 0..budget {
            let count = !reference.recovering() && reference.cpu.pc == pc;
            match reference.step() {
                Ok(Step::Retired) => observed += u64::from(count),
                Ok(Step::Halted | Step::Service(_)) => break,
                Err(_) => {
                    failed = true;
                    break;
                }
                _ => (),
            }
        }
        assert_eq!(native.run_steps(budget).is_err(), failed);
        assert_eq!(reference.stats.guest_bytecodes, observed);
        assert_eq!(native.stats.guest_bytecodes, observed);
        assert_eq!(native.stats, reference.stats);
        assert_eq!(native.cpu, reference.cpu);
        assert_eq!(native.fault, reference.fault);
        assert_eq!(native.objekt, reference.objekt);
        if failed || native.cpu.halted || native.cpu.service {
            break;
        }
    }
    Ok(native)
}

#[test]
fn dispatch_counts_match_single_steps_at_every_batch_boundary() -> Result<()> {
    let source = "loop: d=7, r=Bus, ldrb\nd=9, r=Bus, rb=1, ldrb\nseq=Jump, brch=loop";
    for budget in [1, 2, 3, 7, 32, 256] {
        let m = compare(source, 1, budget, 20)?;
        assert_eq!(m.stats.guest_bytecodes, (budget * 20 + 1) / 3);
    }
    Ok(())
}

#[test]
fn conditional_traces_count_only_retired_paths() -> Result<()> {
    // Both taken edges and fallthroughs occur; branches also update registers
    // and flags simultaneously. Compare every possible observation boundary.
    let source = "d=5, r=Bus, rb=1, ldrb\n\
        loop: ra=1, s=Branch, brch=1, alu=Sub, cin=One, rb=1, ldrb, flags\n\
        ra=2, s=Bus, d=1, alu=Add, rb=2, ldrb, seq=ConditionalJump, cc=Zero, brch=done\n\
        ra=1, s=Branch, brch=2, alu=And, flags\n\
        seq=ConditionalJump, cc=Zero, brch=loop\n\
        d=17, r=Bus, rb=3, ldrb\n\
        seq=Jump, brch=loop\n\
        done: halt";
    for pc in 0..7 {
        for budget in [1, 2, 3, 7, 32, 256] {
            let m = compare(source, pc, budget, 50)?;
            assert!(m.cpu.halted);
            assert_eq!(m.cpu.rf[2], 5);
        }
    }
    // A fault beyond a fallthrough must retain the successful branch prefix.
    for pc in 0..4 {
        compare(
            "ra=0, flags\nseq=ConditionalJump, cc=!Zero, brch=0\nd=7, r=Bus, ldrb\nmem=Read",
            pc,
            256,
            1,
        )?;
    }
    Ok(())
}

#[test]
fn faults_and_fetch_fallback_keep_only_the_successful_prefix() -> Result<()> {
    for source in [
        "d=7, r=Bus, ldrb\nd=8192, seq=Bus",
        "d=7, r=Bus, ldrb\nd=0x8000000011, page=Fetch\nhalt",
        "d=7, r=Bus, ldrb\nmem=Read\nhalt",
    ] {
        for pc in [0, 1] {
            for budget in [1, 2, 256] {
                let m = compare(source, pc, budget, 4)?;
                assert!(m.fault.is_some());
                assert_eq!(m.stats.guest_bytecodes, u64::from(pc == 0));
            }
        }
    }
    Ok(())
}

#[test]
fn collection_does_not_count_or_duplicate_guest_dispatches() -> Result<()> {
    let source = "gc=Collect\nd=17, r=Bus, ldrb\nhalt";
    // Even if the configured address names a native collector word, it must
    // never be counted as guest work. Entry + 1 is a local register write.
    for pc in [1, boot::COLLECTOR_ENTRY + 1] {
        let m = compare(source, pc, 256, 10000)?;
        assert!(m.cpu.halted);
        assert_eq!(m.stats.collections, 1);
        assert_eq!(m.stats.guest_bytecodes, u64::from(pc == 1));
    }
    let m = compare(
        include_str!("../../../microcode/collection.uc"),
        1,
        256,
        10000,
    )?;
    assert!(m.cpu.halted);
    assert_eq!(m.stats.collections, 5);
    assert_eq!(
        m.stats.guest_bytecodes, 7,
        "failed allocations must not count twice"
    );
    Ok(())
}

#[test]
fn peripheral_failure_does_not_count_the_failing_dispatch() -> Result<()> {
    for jit in [false, true] {
        let mut m = machine("d=7, r=Bus, ldrb\nd=9, r=Bus, rb=1, ldrb", 1, jit)?;
        let mut events = rekursiv_devices::Events::default();
        events.counts[0] = u32::MAX;
        events.schedule.insert(1, 1);
        m.devices.events = Some(events);
        assert!(m.run_steps(2).is_err());
        assert_eq!(m.stats.guest_bytecodes, 0);
        assert_eq!(m.stats.retired, 1);
    }
    Ok(())
}

#[test]
fn edits_and_engine_changes_preserve_the_counter() -> Result<()> {
    let mut m = machine("d=7, r=Bus, ldrb\nseq=Jump, brch=0", 0, true)?;
    m.run_steps(2)?;
    m.image_mut().code[0].as_mut().unwrap().data = rekursiv_asm::Word::raw(9)?;
    m.run_steps(2)?; // Invalidated block uses the general path.
    m.enable_jit()?;
    m.run_steps(2)?;
    m.disable_jit();
    m.run_steps(2)?;
    assert_eq!(m.stats.guest_bytecodes, 4);
    assert_eq!(m.cpu.rf[0], 9);
    assert!(m.enable_guest_counter(1).is_err());
    let mut fresh = boot::microcode_with_pager("halt", 512, 16)?.machine;
    assert!(!fresh.guest_counter_enabled());
    assert!(fresh.enable_guest_counter(8000).is_err());
    fresh.enable_jit()?;
    assert!(fresh.enable_guest_counter(0).is_err());
    Ok(())
}

#[test]
fn rates_use_guest_counts_and_handle_zero_time() {
    let stats = Statistics {
        retired: 1000,
        guest_bytecodes: 10,
        ..Default::default()
    };
    assert_eq!(stats.bytecodes_per_second(Duration::ZERO), 0.0);
    assert_eq!(stats.bytecodes_per_second(Duration::from_millis(500)), 20.0);
}

#[test]
#[ignore = "requires pinned Xerox and Squeak distributions; set REKURSIV_ST80_DIR and REKURSIV_SQUEAK_DIR"]
fn archived_guests_match_observed_dispatches() -> Result<()> {
    for squeak in [false, true] {
        let (directory, file) = if squeak {
            ("REKURSIV_SQUEAK_DIR", "Squeak1.1.image")
        } else {
            ("REKURSIV_ST80_DIR", "VirtualImage")
        };
        let bytes = std::fs::read(std::path::PathBuf::from(std::env::var(directory)?).join(file))?;
        let load = || {
            if squeak {
                boot::squeak(&bytes, 16_777_216)
            } else {
                boot::smalltalk(&bytes, 16_777_216)
            }
        };
        let loaded = load()?;
        let dispatch = loaded.symbols["decoded"] as u16;
        let mut reference = loaded.machine;
        let mut native = load()?.machine;
        for m in [&mut reference, &mut native] {
            m.devices = rekursiv_emulator::presentation::workstation(0, 1000);
        }
        assert!(native.guest_counter_enabled());
        native.enable_jit()?;
        let mut observed = 0;
        let mut steps = 0;
        // Startup can spend many microinstructions inside one primitive. Stop
        // at a known guest count rather than assuming a bytecode/microcode ratio.
        while observed < 256 && steps < 50_000_000 {
            let count = !reference.recovering() && reference.cpu.pc == dispatch;
            if reference.step()? == Step::Retired && count {
                observed += 1;
            }
            steps += 1;
            assert!(!reference.cpu.halted && !reference.cpu.service);
        }
        assert_eq!(observed, 256, "guest did not reach the dispatch checkpoint");
        native.run_steps(steps)?;
        assert_eq!(native.stats.guest_bytecodes, observed);
        assert_eq!(reference.stats.guest_bytecodes, observed);
        assert_eq!(native.cpu, reference.cpu);
        assert_eq!(native.stats, reference.stats);
        assert_eq!(native.objekt, reference.objekt);
    }
    Ok(())
}
