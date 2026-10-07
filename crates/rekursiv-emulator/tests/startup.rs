//! Original-image regression without compiling or executing RTL.
use eyre::{ensure, Result};
use rekursiv_emulator::{boot, presentation, Step};
#[test]
#[ignore = "requires the pinned Xerox distribution; set REKURSIV_ST80_DIR"]
fn native_saved_image_matches_xerox_trace_and_rtl_startup_checkpoint() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("REKURSIV_ST80_DIR")?);
    let reference = std::fs::read_to_string(directory.join("trace2"))?;
    // Published trace checksum from fixtures/xerox-v2.json, so a changed local
    // reference cannot silently bless a different guest execution.
    ensure!(rekursiv_smalltalk::checksum(reference.as_bytes()) == TRACE_SHA256);
    let expected: Vec<u8> = reference
        .lines()
        .filter_map(|line| {
            line.strip_prefix("Bytecode <")?
                .split_once('>')?
                .0
                .parse()
                .ok()
        })
        .collect();
    let mut loaded = boot::smalltalk(&std::fs::read(directory.join("VirtualImage"))?, 131072)?;
    let m = &mut loaded.machine;
    m.devices = presentation::workstation(0, 1000);
    let mut bytecodes = Vec::new();
    let mut boundaries = 0;
    let mut found = false;
    for _ in 0..10_000_000 {
        if m.step()? != Step::Retired || m.recovering() {
            continue;
        }
        if m.cpu.pc == loaded.symbols["cycle"] as u16 {
            boundaries += 1;
        }
        if m.cpu.pc == loaded.symbols["decoded"] as u16 {
            bytecodes.push(m.cpu.rf[0] as u8);
        }
        if m.cpu.pc == loaded.symbols["primitive_dispatch"] as u16 && m.cpu.rf[0] == 96 {
            found = true;
            break;
        }
        ensure!(
            !m.cpu.halted && !m.cpu.service,
            "unexpected stop at {}",
            m.cpu.pc
        );
    }
    ensure!(found, "did not reach BitBlt boundary");
    assert_eq!(expected.len(), 499);
    assert_eq!(bytecodes[..expected.len()], expected);
    assert_eq!(boundaries, 2176);
    assert_eq!(m.stats.collections, 3);
    let display = m.devices.display_bitmap.as_ref().unwrap();
    assert_eq!(display.publications, 2);
    let frame = display.visible.as_ref().unwrap();
    assert_eq!((frame.width, frame.height), (640, 480));
    eprintln!("native startup: {} instructions, {} collector instructions, 499 trace bytecodes, {boundaries} boundaries, {} collections", m.stats.retired, m.stats.collector_retired, m.stats.collections);
    Ok(())
}

const TRACE_SHA256: &str = "b6b42ecdc4e52381e85ef30fde9669656ae1e665604819dd3ab04a4f06c8cd72";

#[test]
#[ignore = "requires the pinned Xerox distribution; set REKURSIV_ST80_DIR"]
fn native_saved_image_reclaims_before_low_space_notification() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("REKURSIV_ST80_DIR")?);
    let mut loaded = boot::smalltalk(&std::fs::read(directory.join("VirtualImage"))?, 1_048_576)?;
    let m = &mut loaded.machine;
    m.devices = presentation::workstation(0, 1000);
    let mut proactive = 0;
    let mut copies = 0;
    for _ in 0..100_000_000 {
        ensure!(
            !m.cpu.halted && !m.cpu.service,
            "unexpected stop at {}",
            m.cpu.pc
        );
        let pc = m.cpu.pc;
        let collecting = m.recovering();
        if m.step()? != Step::Retired || collecting {
            continue;
        }
        ensure!(
            pc != loaded.symbols["low_space_signal"] as u16,
            "premature low-space notification"
        );
        proactive += usize::from(pc == loaded.symbols["low_space_collected"] as u16);
        copies += usize::from(pc == loaded.symbols["bb_success"] as u16);
        if m.stats.retired >= 60_000_000 {
            break;
        }
    }
    // The former notifier was reached at about 35 million mutator instructions.
    assert!(m.stats.retired >= 60_000_000);
    assert!(proactive > 0);
    assert!(copies > 700);
    assert_ne!(
        m.cpu.roots.unwrap()[30],
        rekursiv_smalltalk::layout::reference(2)?
    );
    eprintln!("post-reclamation: {} mutator instructions, {proactive} proactive collections, {copies} copies", m.stats.retired);
    Ok(())
}

/// This is deliberately bounded: successful drawing is not a claim that image
/// startup, storage, snapshotting, or every interactive operation is complete.
#[test]
#[ignore = "requires the pinned Xerox distribution; set REKURSIV_ST80_DIR"]
fn native_saved_image_draws_through_bitblt() -> Result<()> {
    let directory = std::path::PathBuf::from(std::env::var("REKURSIV_ST80_DIR")?);
    let mut loaded = boot::smalltalk(&std::fs::read(directory.join("VirtualImage"))?, 131072)?;
    let m = &mut loaded.machine;
    m.devices = presentation::workstation(0, 1000);
    let started = std::time::Instant::now();
    let mut completed = 0;
    let mut initial_display = None;
    let mut calls = std::collections::BTreeMap::<u32, u64>::new();
    let mut boundaries = 0;
    let mut failures = 0;
    for _ in 0..100_000_000 {
        ensure!(
            !m.cpu.halted && !m.cpu.service,
            "unexpected stop at {}",
            m.cpu.pc
        );
        if m.step()? != Step::Retired || m.recovering() {
            continue;
        }
        if m.cpu.pc == loaded.symbols["cycle"] as u16 {
            boundaries += 1;
        }
        if m.cpu.pc == loaded.symbols["primitive_dispatch"] as u16 {
            *calls.entry(m.cpu.rf[0]).or_default() += 1;
            if m.cpu.rf[0] == 96 && initial_display.is_none() {
                initial_display = Some(
                    m.devices
                        .display_bitmap
                        .as_ref()
                        .unwrap()
                        .visible
                        .as_ref()
                        .unwrap()
                        .words
                        .clone(),
                );
            }
        }
        if m.cpu.pc == loaded.symbols["bb_failed"] as u16 {
            failures += 1;
        }
        if m.cpu.pc == loaded.symbols["bb_success"] as u16 {
            completed += 1;
            if completed == 32 {
                break;
            }
        }
    }
    let display = m.devices.display_bitmap.as_ref().unwrap();
    let frame = display.visible.as_ref().unwrap();
    assert_eq!(completed, 32, "BitBlt progress: {calls:?}");
    assert_eq!(failures, 0);
    assert_eq!(boundaries, 9870);
    assert_eq!(m.stats.collections, 24);
    // Bound executed work, not wall time: these thresholds leave headroom
    // while rejecting the old full-bitmap/full-frame path (22M / 308K).
    assert!(m.stats.retired < 8_000_000);
    assert!(m.stats.device_requests < 40_000);
    // One of these copies clips to an empty rectangle and publishes nothing.
    assert_eq!(display.publications, 33);
    assert_eq!((frame.width, frame.height), (640, 480));
    assert_ne!(frame.words, initial_display.unwrap());
    let checksum = rekursiv_smalltalk::checksum(
        &frame
            .words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        checksum,
        "5c88b1fc4cd078c333f200091c7dab230cbbb9a0ba8a3b35e222a26b43fa9115"
    );
    eprintln!(
        "drawing stats {:?}, elapsed {:?}, frame {}",
        m.stats,
        started.elapsed(),
        checksum
    );
    eprintln!("post-BitBlt: {boundaries} bytecodes, {completed} successful copies, {} display publications, {} collections; primitives {calls:?}",display.publications,m.stats.collections);
    Ok(())
}
