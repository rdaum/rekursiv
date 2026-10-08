//! CharacterScanner primitive contracts: return code, observable scanner state,
//! raster output and failures before any guest mutation.
use super::{graphics::form, *};

pub(super) struct ScanFixture {
    pub f: Fixture,
    pub scanner: u32,
    pub bits: u32,
    pub xtable: u32,
    pub stops: u32,
}

pub(super) fn fixture(
    start: i32,
    stop: i32,
    right: i32,
    display: bool,
    stop_byte: Option<u8>,
) -> ScanFixture {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let (dest, bits) = form(&mut f, 64, 4, 8, vec![0x55555555; 64]);
    let (source, _) = form(&mut f, 512, 2, 1, vec![0xa35c87e1; 32]);
    let map = f.object(f.special[4], Body::Words(vec![0, 0xd2]), 6);
    let xtable = f.pointers(f.special[7], (0..257).map(|n| int(n * 2)).collect());
    let mut stops = vec![nil; 258];
    stops[256] = int(9001);
    stops[257] = int(9002);
    if let Some(byte) = stop_byte {
        stops[byte as usize] = int(9003);
    }
    let stops = f.pointers(f.special[7], stops);
    let scanner = f.pointers(
        f.class,
        vec![
            dest,
            source,
            nil,
            int(3),
            int(2),
            int(1),
            nil,
            int(2),
            nil,
            int(0),
            int(0),
            int(0),
            int(64),
            int(4),
            map,
            int(99),
            xtable,
            stops,
        ],
    );
    let text = f.object(f.special[6], Body::Bytes(b"ABC".to_vec()), 8);
    let selector = f.primitive(103, 6, &[122]); // fallback marker false
    let flag = f.special[if display { 2 } else { 1 }];
    f.start(
        &[32, 33, 34, 35, 36, 37, 38, 132, 6, 7, 124],
        vec![
            scanner,
            int(start),
            int(stop),
            text,
            int(right),
            stops,
            flag,
            selector,
        ],
        vec![],
    );
    ScanFixture {
        f,
        scanner,
        bits,
        xtable,
        stops,
    }
}

#[test]
fn scanner_stops_and_state_match_the_smalltalk_contract() -> Result<()> {
    for (start, stop, right, stop_byte, reason, x, last, sx, width) in [
        (1, 3, 64, None, 9001, 8, 3, Some(134), Some(2)),
        (1, 3, 8, None, 9001, 8, 3, Some(134), Some(2)),
        (1, 3, 5, None, 9002, 4, 2, Some(132), Some(2)),
        (1, 3, 1, None, 9002, 2, 1, Some(130), Some(2)),
        (1, 3, 64, Some(b'B'), 9003, 4, 2, Some(130), Some(2)),
        (1, 3, 64, Some(b'A'), 9003, 2, 1, None, None),
        (3, 2, 64, None, 9001, 2, 2, None, None),
    ] {
        for display in [false, true] {
            let sf = fixture(start, stop, right, display, stop_byte);
            for jit in [false, true] {
                let m = run(&sf.f, jit)?;
                assert_eq!(
                    m.objekt.state.vr[5],
                    Word::signed(reason),
                    "start={start} stop={stop} right={right} display={display} jit={jit}"
                );
                let fields = body(&m, target::reference(sf.scanner)?)?;
                let nil = target::reference(sf.f.special[0])?;
                assert_eq!(fields[6], Word::signed(x));
                assert_eq!(fields[8], width.map(Word::signed).unwrap_or(nil));
                assert_eq!(fields[10], sx.map(Word::signed).unwrap_or(nil));
                assert_eq!(fields[17], Word::signed(last));
                let mut expected = vec![0x55555555u32; 64];
                if display {
                    for dx in 2..x {
                        let fx = 130 + dx - 2;
                        let pixel = if (0xa35c87e1u32 >> (31 - fx % 32)) & 1 == 0 {
                            0
                        } else {
                            0xd2
                        };
                        for y in 1..3 {
                            let shift = (3 - dx % 4) * 8;
                            let word = &mut expected[(y * 16 + dx / 4) as usize];
                            *word = (*word & !(255 << shift)) | (pixel << shift);
                        }
                    }
                }
                assert_eq!(
                    body(&m, target::reference(sf.bits)?)?[2..]
                        .iter()
                        .map(|w| w.bits() as u32)
                        .collect::<Vec<_>>(),
                    expected
                );
                for slot in [8, 9, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25] {
                    assert_eq!(
                        m.cpu.roots.as_ref().unwrap()[slot],
                        Word::ZERO,
                        "leaked scanner root {slot}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn scanner_invalid_late_metrics_and_raster_arguments_fail_without_mutation() -> Result<()> {
    for defect in 0..5 {
        let mut sf = fixture(1, 3, 64, true, None);
        let nil = sf.f.special[0];
        match defect {
            0 => {
                if let Body::Pointers(v) = &mut sf.f.image.objects.get_mut(&sf.xtable).unwrap().body
                {
                    v[68] = nil;
                }
            }
            1 => {
                if let Body::Pointers(v) =
                    &mut sf.f.image.objects.get_mut(&sf.scanner).unwrap().body
                {
                    v[7] = nil;
                }
            }
            2 => {
                if let Body::Pointers(v) =
                    &mut sf.f.image.objects.get_mut(&sf.scanner).unwrap().body
                {
                    v[17] = nil;
                }
            }
            3 => {
                if let Body::Pointers(v) = &mut sf.f.image.objects.get_mut(&sf.stops).unwrap().body
                {
                    v.truncate(257);
                }
            }
            _ => {
                if let Body::Pointers(v) = &mut sf.f.image.objects.get_mut(&sf.xtable).unwrap().body
                {
                    v[65] = int(-(1 << 30));
                    v[66] = int((1 << 30) - 1);
                }
            }
        }
        let Body::Pointers(before) = &sf.f.image.objects[&sf.scanner].body else {
            unreachable!()
        };
        let before: Vec<_> = before.iter().map(|&v| target::value(v).unwrap()).collect();
        for jit in [false, true] {
            let m = run(&sf.f, jit)?;
            assert_eq!(
                m.objekt.state.vr[5],
                target::reference(sf.f.special[1])?,
                "defect={defect} jit={jit}"
            );
            assert_eq!(
                body(&m, target::reference(sf.scanner)?)?[2..],
                before,
                "defect={defect}"
            );
            assert!(body(&m, target::reference(sf.bits)?)?[2..]
                .iter()
                .all(|w| w.bits() == 0x55555555));
        }
    }
    Ok(())
}

#[test]
fn scanner_accepts_short_font_tables_and_keeps_noncopy_rules() -> Result<()> {
    let mut sf = fixture(1, 3, 64, true, None);
    if let Body::Pointers(v) = &mut sf.f.image.objects.get_mut(&sf.xtable).unwrap().body {
        v.truncate(69);
    }
    if let Body::Pointers(v) = &mut sf.f.image.objects.get_mut(&sf.scanner).unwrap().body {
        v[3] = int(6);
    }
    for jit in [false, true] {
        let m = run(&sf.f, jit)?;
        assert_eq!(m.objekt.state.vr[5], Word::signed(9001));
        let mut expected = vec![0x55555555u32; 64];
        for x in 2..8 {
            let source_x = 130 + x - 2;
            let source = if (0xa35c87e1u32 >> (31 - source_x % 32)) & 1 == 0 {
                0
            } else {
                0xd2
            };
            for y in 1..3 {
                expected[y * 16 + x / 4] ^= source << ((3 - x % 4) * 8);
            }
        }
        assert_eq!(
            body(&m, target::reference(sf.bits)?)?[2..]
                .iter()
                .map(|w| w.bits() as u32)
                .collect::<Vec<_>>(),
            expected
        );
    }
    // A missing entry for the second glyph must fail before the first XOR.
    if let Body::Pointers(v) = &mut sf.f.image.objects.get_mut(&sf.xtable).unwrap().body {
        v.truncate(67);
    }
    for jit in [false, true] {
        let m = run(&sf.f, jit)?;
        assert_eq!(m.objekt.state.vr[5], target::reference(sf.f.special[1])?);
        assert!(body(&m, target::reference(sf.bits)?)?[2..]
            .iter()
            .all(|w| w.bits() == 0x55555555));
    }
    Ok(())
}
