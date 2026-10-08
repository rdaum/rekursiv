//! Small packed-pixel fixtures with independent expected destination words.
use super::*;

pub(super) fn form(
    f: &mut Fixture,
    width: i32,
    height: i32,
    depth: i32,
    words: Vec<u32>,
) -> (u32, u32) {
    let bits = f.object(f.special[4], Body::Words(words), 6);
    let origin = f.pointers(f.special[12], vec![int(0), int(0)]);
    (
        f.pointers(
            f.class,
            vec![bits, int(width), int(height), int(depth), origin],
        ),
        bits,
    )
}
pub(super) fn blit(
    f: &mut Fixture,
    dst: u32,
    src: u32,
    halftone: u32,
    rule: i32,
    rect: [i32; 10],
    map: u32,
) {
    let mut fields = vec![dst, src, halftone, int(rule)];
    fields.extend(rect.map(int));
    fields.push(map);
    let bb = f.pointers(f.class, fields);
    let selector = f.primitive(96, 0, &[122]);
    f.start(&[32, 209, 124], vec![bb, selector], vec![]);
}
fn boolean(rule: u32, s: u32, d: u32) -> u32 {
    match rule {
        0 => 0,
        1 => s & d,
        2 => s & !d,
        3 => s,
        4 => !s & d,
        5 => d,
        6 => s ^ d,
        7 => s | d,
        8 => !(s | d),
        9 => !(s ^ d),
        10 => !d,
        11 => s | !d,
        12 => !s,
        13 => !s | d,
        14 => !(s & d),
        15 => d,
        _ => unreachable!(),
    }
}
#[test]
fn packed_colour_boolean_rules_and_edge_masks() -> Result<()> {
    for (depth, rule) in (0..16)
        .map(|r| (8, r))
        .chain([1, 2, 4, 16, 32].map(|d| (d, 3)))
    {
        let mut f = Fixture::new();
        let nil = f.special[0];
        let width = 64 / depth;
        let (dst, bits) = form(&mut f, width, 1, depth, vec![0x936c5aa5, 0x12345678]);
        let (src, _) = form(&mut f, width, 1, depth, vec![0x5678abcd, 0xfedc7654]);
        blit(
            &mut f,
            dst,
            src,
            nil,
            rule,
            [0, 0, width - 1, 1, 0, 0, 0, 0, width, 1],
            nil,
        );
        let last_mask = u32::MAX.checked_shl(depth as u32).unwrap_or(0);
        let expected = [
            boolean(rule as u32, 0x5678abcd, 0x936c5aa5),
            (boolean(rule as u32, 0xfedc7654, 0x12345678) & last_mask) | (0x12345678 & !last_mask),
        ];
        for jit in [false, true] {
            let m = run(&f, jit)?;
            let got = body(&m, target::reference(bits)?)?;
            assert_eq!(
                got[2..].iter().map(|w| w.bits() as u32).collect::<Vec<_>>(),
                expected,
                "depth={depth} rule={rule} jit={jit}"
            );
        }
    }
    Ok(())
}
#[test]
fn indexed_mapping_halftone_clipping_and_aliasing() -> Result<()> {
    let mut f = Fixture::new();
    let (dst, bits) = form(&mut f, 4, 2, 8, vec![0x12345678, 0xabcdef01]);
    let (src, _) = form(&mut f, 4, 2, 1, vec![0xa0000000, 0x50000000]);
    let map = f.object(f.special[4], Body::Words(vec![0x22, 0xff]), 6);
    let half = f.object(f.special[4], Body::Words(vec![0x0ff00ff0, 0xf00ff00f]), 6);
    blit(
        &mut f,
        dst,
        src,
        half,
        3,
        [0, 0, 4, 2, 0, 0, 1, 0, 2, 2],
        map,
    );
    for jit in [false, true] {
        let m = run(&f, jit)?;
        assert_eq!(
            body(&m, target::reference(bits)?)?[2..],
            [Word::raw(0x12200f78)?, Word::raw(0xab0f2001)?]
        );
    }
    let mut f = Fixture::new();
    let nil = f.special[0];
    let (dst, bits) = form(&mut f, 8, 1, 8, vec![0x01020304, 0x05060708]);
    blit(
        &mut f,
        dst,
        dst,
        nil,
        3,
        [1, 0, 7, 1, 0, 0, 0, 0, 8, 1],
        nil,
    );
    for jit in [false, true] {
        let m = run(&f, jit)?;
        assert_eq!(
            body(&m, target::reference(bits)?)?[2..],
            [Word::raw(0x01010203)?, Word::raw(0x04050607)?]
        );
    }
    Ok(())
}

#[test]
fn drawing_to_registered_display_publishes_changed_words() -> Result<()> {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let (display, _) = form(&mut f, 4, 1, 8, vec![0x01020304]);
    let register = f.primitive(102, 0, &[122]);
    let bb_class = f.pointers(f.class, vec![nil, nil, int(2)]);
    let draw = f.selector("draw");
    let method = f.method(&[122], vec![], 0, 0, 96);
    f.bind(bb_class, draw, method);
    let half = f.object(f.special[4], Body::Words(vec![0x07070707]), 6);
    let mut fields = vec![display, nil, half, int(3)];
    fields.extend([1, 0, 2, 1, 0, 0, 0, 0, 4, 1].map(int));
    fields.push(nil);
    let bb = f.pointers(bb_class, fields);
    f.start(
        &[32, 209, 135, 34, 211, 124],
        vec![display, register, bb, draw],
        vec![],
    );
    for jit in [false, true] {
        let m = run_with(&f, jit, |m| {
            m.devices = rekursiv_devices::Device::workstation(0, 1000)
        })?;
        let device = m.devices.display_bitmap.as_ref().unwrap();
        assert_eq!(device.publications, 2);
        assert_eq!(device.pixel_writes, 2);
        let visible = device.visible.as_ref().unwrap();
        assert_eq!(visible.words, [0x01070704]);
        assert_eq!(visible.depth, 8);
        assert_eq!(visible.palette[7], 0x00ffff);
    }
    Ok(())
}

#[test]
fn cursor_registration_paints_black_on_grey_and_leaves_clear_bits_transparent() -> Result<()> {
    use rekursiv_devices::{BitmapFrame, CursorMode, Device};
    use rekursiv_emulator::presentation;
    let mut f = Fixture::new();
    let mut words = vec![0; 16];
    words[0] = 0x80000000;
    let (cursor, _) = form(&mut f, 16, 16, 1, words);
    let register = f.primitive(101, 0, &[122]);
    f.start(&[32, 209, 124], vec![cursor, register], vec![]);
    for jit in [false, true] {
        let m = run_with(&f, jit, |m| m.devices = Device::workstation(0, 1000))?;
        let cursor = m
            .devices
            .cursor_bitmap
            .as_ref()
            .unwrap()
            .visible
            .as_ref()
            .unwrap();
        assert_eq!(cursor.cursor_mode, CursorMode::Paint);
        let frame = BitmapFrame {
            width: 3,
            height: 1,
            stride: 3,
            depth: 32,
            words: vec![0x808080, 0x808080, 0xffffff],
            ..BitmapFrame::default()
        };
        assert_eq!(
            presentation::pixels(&frame, Some((cursor, (0, 0)))),
            [0, 0x808080, 0xffffff]
        );
        assert_eq!(
            presentation::pixels(&frame, Some((cursor, (2, 0)))),
            [0x808080, 0x808080, 0]
        );
    }
    Ok(())
}
