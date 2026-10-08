//! Device-facing primitives run on the CPU; peripherals never see guest oops.
use super::*;
use rekursiv_devices::{Device, Files, InputKind, InputPacket};

fn device_method(f: &mut Fixture, primitive: u32, arity: u32) -> (u32, u32) {
    let class = f.pointers(f.class, vec![f.special[0], f.special[0], int(2)]);
    let receiver = f.pointers(class, vec![]);
    let selector = f.selector(&format!("device{primitive}"));
    let method = f.method(&[122], vec![], arity, arity, primitive);
    f.bind(class, selector, method);
    (receiver, selector)
}
#[test]
fn guest_opens_and_reads_external_bytes_and_partial_words() -> Result<()> {
    for words in [false, true] {
        let mut f = Fixture::new();
        let (open, open_sel) = device_method(&mut f, 153, 2);
        let (read, read_sel) = device_method(&mut f, 154, 4);
        let name = f.object(f.special[6], Body::Bytes(b"/test".to_vec()), 8);
        let buffer = if words {
            f.object(f.special[4], Body::Words(vec![0xaaaaaaaa; 2]), 6)
        } else {
            f.object(f.special[26], Body::Bytes(vec![0xaa; 8]), 8)
        };
        f.start(
            &[32, 33, 114, 242, 104, 35, 16, 36, 118, 37, 132, 4, 6, 124],
            vec![
                open,
                name,
                open_sel,
                read,
                buffer,
                int(if words { 2 } else { 8 }),
                read_sel,
            ],
            vec![f.special[0]],
        );
        for jit in [false, true] {
            let m = run_with(&f, jit, |m| {
                m.devices = Device::workstation(0, 1000);
                let mut files = Files::new(b"/", b"/test.image", b'/');
                files.contents.insert(b"/test".to_vec(), b"ABCDE".to_vec());
                m.devices.files = Some(files);
            })?;
            assert_eq!(
                m.objekt.state.vr[5],
                Word::signed(if words { 1 } else { 5 })
            );
            let got = body(&m, target::reference(buffer)?)?;
            let got: Vec<_> = got[2..].iter().map(|w| w.bits() as u32).collect();
            assert_eq!(
                got,
                if words {
                    vec![0x44434241, 0xaaaaaa45]
                } else {
                    vec![65, 66, 67, 68, 69, 170, 170, 170]
                }
            );
        }
    }
    Ok(())
}
#[test]
fn input_transitions_are_buffered_by_microcode_and_peek_does_not_pop() -> Result<()> {
    for primitive in [108, 109] {
        let mut f = Fixture::new();
        let (receiver, selector) = device_method(&mut f, primitive, 0);
        f.start(&[32, 209, 124], vec![receiver, selector], vec![]);
        for jit in [false, true] {
            let m = run_with(&f, jit, |m| {
                m.devices = Device::workstation(0, 1000);
                for code in [136, 97] {
                    m.devices
                        .push_input(InputPacket {
                            kind: InputKind::KeyDown,
                            timestamp_ms: 0,
                            value: code,
                            extra: 0,
                        })
                        .unwrap();
                }
            })?;
            assert_eq!(m.objekt.state.vr[5], Word::signed(0x141)); // shift + 'A'
            assert_eq!(
                m.cpu.roots.as_ref().unwrap()[7].bits(),
                u64::from(primitive == 109)
            );
            assert_eq!(m.devices.events.as_ref().unwrap().acknowledgements[0], 2);
        }
    }
    Ok(())
}

#[test]
fn shift_caps_lock_punctuation_and_command_modifiers_are_guest_encoded() -> Result<()> {
    for (codes, expected) in [
        (vec![139, 97], 65),
        (vec![139, 136, 97], 0x161), // Caps + Shift -> lowercase, Shift flag remains
        (vec![136, 49], 0x121),
        (vec![139, 49], 49),
        (vec![140, 142, 97], 0xc61), // Option and Command
    ] {
        let mut f = Fixture::new();
        let (receiver, selector) = device_method(&mut f, 108, 0);
        f.start(&[32, 209, 124], vec![receiver, selector], vec![]);
        for jit in [false, true] {
            let m = run_with(&f, jit, |m| {
                m.devices = Device::workstation(0, 1000);
                for &code in &codes {
                    m.devices
                        .push_input(InputPacket {
                            kind: InputKind::KeyDown,
                            timestamp_ms: 0,
                            value: code,
                            extra: 0,
                        })
                        .unwrap();
                }
            })?;
            assert_eq!(
                m.objekt.state.vr[5],
                Word::signed(expected),
                "codes={codes:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn due_timer_signals_once_and_clears_both_registration_roots() -> Result<()> {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let semaphore = f.pointers(f.special[18], vec![nil, nil, int(0)]);
    let (receiver, selector) = device_method(&mut f, 136, 2);
    f.start(
        &[32, 33, 118, 242, 135, 118, 124],
        vec![receiver, semaphore, selector],
        vec![],
    );
    for jit in [false, true] {
        let m = run_with(&f, jit, |m| m.devices = Device::workstation(0, 1))?;
        assert_eq!(body(&m, target::reference(semaphore)?)?[4], Word::signed(1));
        assert_eq!(m.cpu.roots.as_ref().unwrap()[28], target::reference(nil)?);
        let specials = m.cpu.roots.as_ref().unwrap()[0];
        assert_eq!(body(&m, specials)?[31], target::reference(nil)?);
        assert_eq!(m.devices.events.as_ref().unwrap().acknowledgements[1], 1);
    }
    Ok(())
}

#[test]
fn image_name_property_round_trips_without_renaming_host_files() -> Result<()> {
    let mut f = Fixture::new();
    let (setter, set) = device_method(&mut f, 121, 1);
    let (getter, get) = device_method(&mut f, 121, 0);
    let name = f.object(f.special[6], Body::Bytes(b"C:\\next.image".to_vec()), 8);
    f.start(
        &[32, 33, 226, 135, 35, 212, 124],
        vec![setter, name, set, getter, get],
        vec![],
    );
    for jit in [false, true] {
        let m = run_with(&f, jit, |m| {
            m.devices = Device::workstation(0, 1000);
            m.devices.files = Some(Files::new(b"C:\\", b"C:\\old.image", b'\\'));
        })?;
        assert_eq!(
            body(&m, m.objekt.state.vr[5])?[2..]
                .iter()
                .map(|w| w.bits() as u8)
                .collect::<Vec<_>>(),
            b"C:\\next.image"
        );
        assert!(m.devices.files.as_ref().unwrap().contents.is_empty());
    }
    Ok(())
}

#[test]
fn full_keyboard_ring_defers_input_without_starving_a_due_timer() -> Result<()> {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let semaphore = f.pointers(f.special[18], vec![nil, nil, int(0)]);
    f.start(&[118, 124], vec![], vec![]);
    for jit in [false, true] {
        let m = run_with(&f, jit, |m| {
            m.devices = Device::workstation(0, 1);
            m.devices.clocks.as_mut().unwrap().deadline = Some(0);
            m.devices
                .push_input(InputPacket {
                    kind: InputKind::KeyDown,
                    timestamp_ms: 0,
                    value: 97,
                    extra: 0,
                })
                .unwrap();
            let roots = m.cpu.roots.as_mut().unwrap();
            roots[7] = Word::raw(64).unwrap();
            roots[28] = target::reference(semaphore).unwrap();
        })?;
        assert_eq!(body(&m, target::reference(semaphore)?)?[4], Word::signed(1));
        let events = m.devices.events.as_ref().unwrap();
        assert_eq!(events.acknowledgements[0], 0);
        assert_eq!(events.acknowledgements[1], 1);
        assert_eq!(m.cpu.roots.as_ref().unwrap()[7].bits(), 64);
    }
    Ok(())
}
