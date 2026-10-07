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
