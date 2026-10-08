//! Pinned archive execution, including display restoration and physical input.
use super::*;
use rekursiv_devices::{Bitmap, Files};
use rekursiv_emulator::presentation;

fn directory() -> Result<std::path::PathBuf> {
    Ok(std::path::PathBuf::from(std::env::var(
        "REKURSIV_SQUEAK_DIR",
    )?))
}

#[test]
#[ignore = "requires pinned Squeak distribution; set REKURSIV_SQUEAK_DIR"]
fn saved_context_keeps_the_original_resumption_prefix() -> Result<()> {
    let mut l = boot::squeak(
        &std::fs::read(directory()?.join("Squeak1.1.image"))?,
        1_048_576,
    )?;
    let mut bytecodes = vec![];
    let mut primitives = vec![];
    for _ in 0..1_000_000 {
        if l.machine.cpu.pc == l.symbols["decoded"] as u16 {
            bytecodes.push(l.machine.cpu.rf[0]);
        }
        if l.machine.cpu.pc == l.symbols["primitive_dispatch"] as u16 {
            primitives.push(l.machine.cpu.rf[0]);
            if l.machine.cpu.rf[0] == 101 {
                break;
            }
        }
        ensure!(l.machine.step()? == Step::Retired, "early startup stop");
    }
    // Project regression checkpoint, not a trace captured from the original VM.
    assert_eq!(
        bytecodes,
        [
            129, 146, 135, 17, 18, 131, 237, 16, 124, 155, 82, 131, 64, 124, 131, 65, 112, 224, 16,
            130, 66, 64, 225, 16, 199, 112, 198, 157, 16, 130, 16, 211
        ]
    );
    assert_eq!(primitives, [258, 0, 0, 0, 0, 0, 101]);
    assert_eq!(l.machine.objekt.state.vr[7], target::reference(8_451_312)?);
    assert!(!l.machine.cpu.halted);
    Ok(())
}

fn advance(m: &mut Machine, instructions: u64) -> Result<()> {
    m.run_steps(instructions)?;
    ensure!(
        !m.cpu.halted && !m.cpu.service,
        "Squeak stopped: PC={} R15={} primitive={}",
        m.cpu.pc,
        m.cpu.rf[15],
        m.cpu.rf[0]
    );
    Ok(())
}
fn screenshot(m: &Machine, name: &str) -> Result<()> {
    if let Ok(dir) = std::env::var("REKURSIV_SQUEAK_FRAMES") {
        presentation::save_ppm(
            m.devices
                .display_bitmap
                .as_ref()
                .unwrap()
                .visible
                .as_ref()
                .unwrap(),
            &std::path::Path::new(&dir).join(name),
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires pinned Squeak distribution; set REKURSIV_SQUEAK_DIR"]
fn saved_image_restores_colour_desktop_and_opens_a_menu() -> Result<()> {
    let dir = directory()?;
    let mut l = boot::squeak(&std::fs::read(dir.join("Squeak1.1.image"))?, 16_777_216)?;
    let m = &mut l.machine;
    m.devices = presentation::workstation(0, 1000);
    m.devices.display_bitmap = Some(Bitmap::new(1024, 768));
    let mut files = Files::new(b"C:\\", b"C:\\Squeak1.1.image", b'\\');
    for name in ["Squeak1.1.image", "SqueakV1.sources", "Squeak1.1.changes"] {
        files.contents.insert(
            format!("/{name}").into_bytes(),
            std::fs::read(dir.join(name))?,
        );
    }
    m.devices.files = Some(files);
    m.enable_jit()?;
    advance(m, 600_000_000)?;
    screenshot(m, "desktop.ppm")?;
    let desktop = m
        .devices
        .display_bitmap
        .as_ref()
        .unwrap()
        .visible
        .clone()
        .unwrap();
    assert_eq!(
        (desktop.width, desktop.height, desktop.depth),
        (1024, 768, 8)
    );
    let pixels = presentation::pixels(&desktop, None);
    assert_eq!(pixels[500 * 1024 + 600], 0x808080); // uncovered desktop
    assert_eq!(pixels[90 * 1024 + 60], 0xffcc00); // welcome window
    assert!(pixels.contains(&0)); // glyphs and window borders
    assert!(m.devices.cursor_bitmap.as_ref().unwrap().visible.is_some());
    let cursor = m
        .devices
        .cursor_bitmap
        .as_ref()
        .unwrap()
        .visible
        .as_ref()
        .unwrap();
    assert_eq!(cursor.cursor_mode, rekursiv_devices::CursorMode::Paint);
    let with_cursor = presentation::pixels(&desktop, Some((cursor, (600, 500))));
    assert!(
        with_cursor
            .iter()
            .zip(&pixels)
            .any(|(&shown, &background)| background == 0x808080 && shown == 0),
        "cursor must contrast with grey desktop"
    );
    presentation::pointer(&mut m.devices, 600, 500);
    presentation::key(&mut m.devices, 129, true)?; // yellow/middle button
    advance(m, 150_000_000)?;
    screenshot(m, "menu.ppm")?;
    let menu = m
        .devices
        .display_bitmap
        .as_ref()
        .unwrap()
        .visible
        .as_ref()
        .unwrap();
    assert_ne!(
        menu.words, desktop.words,
        "button press must draw a guest menu"
    );
    presentation::pointer(&mut m.devices, 900, 700); // cancel outside menu
    presentation::key(&mut m.devices, 129, false)?;
    advance(m, 100_000_000)?;
    screenshot(m, "dismissed.ppm")?;
    let dismissed = m
        .devices
        .display_bitmap
        .as_ref()
        .unwrap()
        .visible
        .as_ref()
        .unwrap();
    assert_eq!(
        dismissed.words, desktop.words,
        "dismissed menu restores its background"
    );
    Ok(())
}
