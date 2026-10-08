//! Compare accelerated calls with the actual scanner bytecodes in the pinned
//! distribution. The harness forks only at a drained primitive boundary.
use super::*;
use rekursiv_devices::{Bitmap, Device, Files, Request};
use rekursiv_emulator::presentation;

fn fork(seed: &Machine) -> Result<Machine> {
    // New supplies execution bookkeeping; the full architectural CPU/object
    // state comes from the stopped machine. Scanner calls have no pending I/O.
    let mut m = Machine::new(seed.image().clone(), seed.cpu.pc, 2, 2)?;
    m.cpu = seed.cpu.clone();
    m.objekt = seed.objekt.clone();
    m.devices = presentation::workstation(0, 1000);
    m.devices.display_bitmap = Some(Bitmap::new(1024, 768));
    // Preserve the published frame and staged geometry. Otherwise each fork
    // pays for an unrelated full-screen upload on its first glyph.
    for (base, source) in [
        (0x410, &seed.devices.cursor_bitmap),
        (0x510, &seed.devices.display_bitmap),
    ] {
        if let Some(frame) = source.as_ref().and_then(|b| b.visible.as_ref()) {
            let dest = if base == 0x410 {
                &mut m.devices.cursor_bitmap
            } else {
                &mut m.devices.display_bitmap
            };
            dest.as_mut().unwrap().visible = Some(frame.clone());
            for (offset, value) in [
                (8, frame.width),
                (12, frame.height),
                (16, frame.stride),
                (32, frame.depth),
            ] {
                device_write(&mut m.devices, base + offset, value)?;
            }
            device_write(&mut m.devices, base + 36, 0)?;
            for &colour in &frame.palette {
                device_write(&mut m.devices, base + 40, colour)?;
            }
        }
    }
    Ok(m)
}

fn device_write(d: &mut Device, address: u32, data: u32) -> Result<()> {
    d.tick(
        Some(Request {
            address,
            data,
            write: true,
        }),
        false,
    )?;
    while d.response().is_none() {
        d.tick(None, false)?;
    }
    ensure!(!d.response().unwrap().error, "device restore failed");
    d.tick(None, true)
}

fn finish(m: &mut Machine, boundary: u16, caller: Word, ip: u32, sp: u32) -> Result<Word> {
    for _ in 0..20_000_000 {
        m.step()?;
        ensure!(
            !m.cpu.halted && !m.cpu.service,
            "scanner stopped at {} status {}",
            m.cpu.pc,
            m.cpu.rf[15]
        );
        if m.cpu.pc == boundary
            && m.objekt.state.vr[0] == caller
            && m.cpu.rf[8] == ip
            && m.cpu.rf[9] == sp - 6
        {
            return Ok(body(m, caller)?[(sp - 6) as usize + 7]);
        }
    }
    eyre::bail!("scanner failed to return to its caller")
}

#[test]
#[ignore = "requires pinned Squeak distribution; set REKURSIV_SQUEAK_DIR"]
fn archived_scanner_bytecodes_and_accelerator_agree_on_text_runs() -> Result<()> {
    let dir = std::path::PathBuf::from(std::env::var("REKURSIV_SQUEAK_DIR")?);
    let bytes = std::fs::read(dir.join("Squeak1.1.image"))?;
    let true_object =
        target::reference(rekursiv_smalltalk::squeak::image::Image::parse(&bytes)?.special(2)?)?;
    let mut l = boot::squeak(&bytes, 16_777_216)?;
    let entry = l.symbols["primitive_scanner"] as u16;
    let boundary = l.symbols["boundary"] as u16;
    let fallback = l.symbols["primitive_failed"] as u16;
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
    let original = m.image().code[entry as usize].unwrap();
    let mut totals = [0u64; 2];
    let mut displays = 0;
    for run in 0..8 {
        m.image_mut().code[entry as usize].as_mut().unwrap().halt = true;
        m.run_steps(300_000_000)?;
        ensure!(
            m.cpu.halted && m.cpu.pc == entry,
            "did not stop at scanner: {}",
            m.cpu.pc
        );
        m.image_mut().code[entry as usize] = Some(original);
        m.cpu.halted = false;
        m.cpu.pc = entry;
        let receiver = m.objekt.state.vr[6];
        let caller = m.objekt.state.vr[0];
        let (ip, sp) = (m.cpu.rf[8], m.cpu.rf[9]);
        let fields = body(m, receiver)?;
        let display = body(m, caller)?[sp as usize + 7] == true_object;
        let bits = if display {
            Some(body(m, fields[2])?[2])
        } else {
            None
        };
        displays += usize::from(display);
        let mut fast = fork(m)?;
        let mut slow = fork(m)?;
        slow.cpu.pc = fallback;
        let fast_result = finish(&mut fast, boundary, caller, ip, sp)?;
        let slow_result = finish(&mut slow, boundary, caller, ip, sp)?;
        assert_eq!(fast_result, slow_result, "run {run}");
        assert_eq!(
            body(&fast, receiver)?,
            body(&slow, receiver)?,
            "scanner fields run {run}"
        );
        if let Some(bits) = bits {
            assert_eq!(body(&fast, bits)?, body(&slow, bits)?, "bitmap run {run}");
        }
        totals[0] += slow.stats.retired;
        totals[1] += fast.stats.retired;
        eprintln!(
            "archived scanner run {run} display={display}: {} -> {} microinstructions",
            slow.stats.retired, fast.stats.retired
        );
        // Advance beyond this entry before arming the next primitive breakpoint.
        m.step()?;
    }
    assert!(displays > 0, "include real drawing calls");
    assert!(
        totals[1] * 2 < totals[0],
        "scanner must avoid the bytecode loop: {totals:?}"
    );
    Ok(())
}
