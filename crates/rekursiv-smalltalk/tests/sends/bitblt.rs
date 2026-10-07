//! Pixel-level oracle against wordwise machine microcode. Rust computes only
//! expectations; both executors run the same primitive and refresh program.
use super::*;
use rekursiv_emulator::Machine;
const DEST: u16 = 220;
const SOURCE: u16 = 222;
const HALF: u16 = 224;
const DEST_BITS: u16 = 226;
const SOURCE_BITS: u16 = 228;
const HALF_BITS: u16 = 230;

#[derive(Clone, Debug)]
struct Case {
    dimensions: (i32, i32),
    rule: i32,
    dest: (i32, i32),
    source: (i32, i32),
    extent: (i32, i32),
    clip: (i32, i32, i32, i32),
    nil_source: bool,
    halftone: bool,
    halftone_alias: bool,
    overlap: bool,
    alias: bool,
}
impl Default for Case {
    fn default() -> Self {
        Self {
            dimensions: (37, 7),
            rule: 3,
            dest: (7, 1),
            source: (2, 0),
            extent: (30, 5),
            clip: (0, 0, 37, 7),
            nil_source: false,
            halftone: true,
            halftone_alias: false,
            overlap: false,
            alias: false,
        }
    }
}
fn words(image: &mut source::Image, object: u16, data: Vec<u16>) {
    image.objects.insert(
        object,
        source::Object {
            oop: object,
            class: 16,
            body: source::Body::Words(data),
        },
    );
}
fn make(case: &Case) -> source::Image {
    let mut image = fixture(&[112, 208, 124], &[SELECTOR]);
    let dimensions = case.dimensions;
    let count = (((dimensions.0 + 15) / 16) * dimensions.1) as u32;
    for (form, bits, width, height) in [
        (DEST, DEST_BITS, dimensions.0, dimensions.1),
        (
            SOURCE,
            if case.overlap { DEST_BITS } else { SOURCE_BITS },
            dimensions.0,
            dimensions.1,
        ),
        (
            HALF,
            if case.halftone_alias {
                DEST_BITS
            } else {
                HALF_BITS
            },
            16,
            16,
        ),
    ] {
        pointers(
            &mut image,
            form,
            CLASS,
            vec![bits, oop(width), oop(height), 2],
        );
    }
    for (object, salt, count) in [
        (DEST_BITS, 0x53a9u16, count),
        (SOURCE_BITS, 0xb641, count),
        (HALF_BITS, 0x8241, 16),
    ] {
        words(
            &mut image,
            object,
            (0..count).map(|n| salt.rotate_left(n % 16)).collect(),
        );
    }
    pointers(
        &mut image,
        RECEIVER,
        CLASS,
        vec![
            DEST,
            if case.nil_source {
                2
            } else if case.overlap && !case.alias {
                DEST
            } else {
                SOURCE
            },
            if case.halftone { HALF } else { 2 },
            oop(case.rule),
            oop(case.dest.0),
            oop(case.dest.1),
            oop(case.extent.0),
            oop(case.extent.1),
            oop(case.source.0),
            oop(case.source.1),
            oop(case.clip.0),
            oop(case.clip.1),
            oop(case.clip.2),
            oop(case.clip.3),
        ],
    );
    primitive_method(&mut image, 96, 0, &[116, 124]); // failure returns -1
    image
}
fn bits(image: &source::Image, object: u16) -> Vec<u16> {
    let source::Body::Words(words) = &image.objects[&object].body else {
        panic!("word fixture")
    };
    words.clone()
}
fn expected(image: &source::Image, case: &Case) -> Vec<u16> {
    let mut dest = bits(image, DEST_BITS);
    let src = bits(image, if case.overlap { DEST_BITS } else { SOURCE_BITS });
    let half = bits(
        image,
        if case.halftone_alias {
            DEST_BITS
        } else {
            HALF_BITS
        },
    );
    let (width, height) = case.dimensions;
    let stride = (width + 15) / 16;
    for y in 0..height {
        for x in 0..width {
            let (tx, ty) = (x - case.dest.0, y - case.dest.1);
            if tx < 0
                || ty < 0
                || tx >= case.extent.0
                || ty >= case.extent.1
                || x < case.clip.0
                || y < case.clip.1
                || x >= case.clip.0 + case.clip.2
                || y >= case.clip.1 + case.clip.3
            {
                continue;
            }
            let (sx, sy) = (case.source.0 + tx, case.source.1 + ty);
            if !case.nil_source && (!(0..width).contains(&sx) || !(0..height).contains(&sy)) {
                continue;
            }
            let source_bit = case.nil_source
                || src[(sy * stride + sx / 16) as usize] & (1 << (15 - sx % 16)) != 0;
            let source_bit =
                source_bit && (!case.halftone || half[y as usize % 16] & (1 << (15 - x % 16)) != 0);
            let index = (y * stride + x / 16) as usize;
            let mask = 1 << (15 - x % 16);
            let destination_bit = dest[index] & mask != 0;
            // The Blue Book's ordering: 0 clear, 1 AND, 3 source, 5 destination,
            // 6 XOR, 7 OR, 10 invert destination, 15 set.
            let truth_index = 3 - (2 * u32::from(source_bit) + u32::from(destination_bit));
            if (case.rule as u32 >> truth_index) & 1 != 0 {
                dest[index] |= mask;
            } else {
                dest[index] &= !mask;
            }
        }
    }
    dest
}
fn native(source: &source::Image, device: rekursiv_sim::device::Device) -> Result<Machine> {
    native_prepared(source, device, |_| {})
}
fn native_prepared(
    source: &source::Image,
    device: rekursiv_sim::device::Device,
    prepare: impl FnOnce(&mut target::Image),
) -> Result<Machine> {
    let mut converted = target::Image::convert(source, &[ROOT])?;
    prepare(&mut converted);
    let assembly = interpreter::assemble(r(ROOT))?;
    let program =
        Program::from_assembly(&assembly)?.with_ram_collector(interpreter::COLLECTOR_ENTRY, 16)?;
    let mut m = Machine::new(program, assembly.entry.unwrap(), 16, 512)?;
    for record in &converted.records {
        m.objekt.store.records.insert(
            record.reference.identity()?,
            Record {
                reference: record.reference,
                class: record.class,
                cond: record.cond,
                body: record.body.clone(),
            },
        );
    }
    let root = converted
        .records
        .iter()
        .find(|r| r.source_oop == ROOT)
        .unwrap();
    m.objekt.memory[..root.body.len()].copy_from_slice(&root.body);
    m.objekt.service(
        Service::Install(rekursiv_asm::Entry {
            reference: root.reference,
            class: root.class,
            size: root.body.len() as u32,
            base: 0,
            representation: root.body[0],
            new: false,
            modified: false,
            cond: false,
        }),
        false,
    );
    // Exhaust the first semispace. Validation/refill and scratch allocation
    // must retain every Form and full-width reference in the ESTK frame.
    m.objekt.service(
        Service::Install(rekursiv_asm::Entry {
            reference: Word::reference(32000, false)?,
            class: r(16),
            size: 16,
            base: 240,
            representation: Word::ZERO,
            new: false,
            modified: false,
            cond: false,
        }),
        false,
    );
    m.objekt
        .service(Service::ReserveIdentities(converted.next_identity), false);
    m.objekt.service(
        Service::CompactClass {
            code: 2,
            class: r(12),
        },
        false,
    );
    m.devices = device;
    for _ in 0..3_000_000 {
        if m.cpu.halted {
            return Ok(m);
        }
        m.step()
            .wrap_err_with(|| format!("micro-PC {} rf {:?}", m.cpu.pc, m.cpu.rf))?;
    }
    eyre::bail!("native timeout pc {}", m.cpu.pc)
}
fn body(m: &Machine, object: u16) -> Vec<Word> {
    let reference = r(object);
    if let Ok(e) = m.objekt.resolve(reference) {
        m.objekt.memory[e.base as usize..(e.base + e.size) as usize].to_vec()
    } else {
        m.objekt.store.records[&reference.identity().unwrap()]
            .body
            .clone()
    }
}
#[test]
fn native_bitblt_all_rules_clipping_alignment_and_shared_bitmap_overlap() -> Result<()> {
    let mut cases: Vec<_> = (0..16)
        .map(|rule| Case {
            rule,
            ..Default::default()
        })
        .collect();
    for offset in 0..16 {
        cases.push(Case {
            dest: (offset, 1),
            source: (15 - offset, 0),
            halftone: false,
            ..Default::default()
        });
    }
    for (dest, source, clip, extent) in [
        ((-5, -2), (2, 1), (-3, -3, 99, 99), (44, 12)),
        ((2, 1), (-4, -2), (5, 2, 25, 4), (40, 8)),
        ((-16384, 0), (0, 0), (0, 0, 37, 7), (16383, 3)),
        ((0, 0), (0, 0), (0, 0, 37, 7), (0, 5)),
        ((0, 0), (0, 0), (0, 0, -1, 7), (30, 5)),
    ] {
        cases.push(Case {
            dest,
            source,
            clip,
            extent,
            ..Default::default()
        });
    }
    for alias in [false, true] {
        for (dest, source) in [
            ((3, 1), (0, 0)),
            ((0, 0), (3, 1)),
            ((3, 0), (0, 0)),
            ((0, 0), (3, 0)),
        ] {
            cases.push(Case {
                dest,
                source,
                overlap: true,
                alias,
                ..Default::default()
            });
        }
    }
    cases.extend([
        Case {
            nil_source: true,
            ..Default::default()
        },
        Case {
            nil_source: true,
            halftone: false,
            rule: 6,
            ..Default::default()
        },
    ]);
    cases.push(Case {
        dimensions: (16, 16),
        halftone_alias: true,
        overlap: true,
        rule: 6,
        ..Default::default()
    });
    for case in cases {
        let image = make(&case);
        let expected = expected(&image, &case);
        let m = native(&image, Default::default()).wrap_err_with(|| format!("{case:?}"))?;
        assert_eq!(m.cpu.rf[15], 1, "{case:?}");
        assert_eq!(m.objekt.state.vr[5], r(RECEIVER), "{case:?}");
        assert_eq!(
            body(&m, DEST_BITS)[1..]
                .iter()
                .map(|w| w.bits() as u16)
                .collect::<Vec<_>>(),
            expected,
            "{case:?}"
        );
        assert!(m.stats.collections > 0);
    }
    Ok(())
}
#[test]
fn rtl_bitblt_rules_and_overlap_match_pixel_oracle() -> Result<()> {
    for rule in 0..16 {
        let case = Case {
            rule,
            overlap: true,
            alias: true,
            ..Default::default()
        };
        let image = make(&case);
        let expected = expected(&image, &case);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!((e.status, e.result), (1, r(RECEIVER)), "rule {rule}");
        assert_eq!(
            e.records[&r(DEST_BITS).identity()?].body[1..]
                .iter()
                .map(|w| w.bits() as u16)
                .collect::<Vec<_>>(),
            expected,
            "rule {rule}"
        );
        assert!(e.collections > 0);
    }
    Ok(())
}

/// Register a different Form sharing the destination's bitmap, then draw.
/// The refresh must compare bitmap identity rather than Form identity.
fn registered(case: &Case, primitive: i32) -> source::Image {
    let mut image = make(case);
    pointers(
        &mut image,
        SOURCE,
        232,
        vec![DEST_BITS, oop(case.dimensions.0), oop(case.dimensions.1), 2],
    );
    dictionary(&mut image, 232, 2, 234, 236, &[(116, 238)]);
    pointers(&mut image, 240, 16, vec![2, 232]);
    method(&mut image, 238, 7, 0, &[oop(primitive), 240], &[116, 124]);
    method(
        &mut image,
        CALLER,
        0,
        0,
        &[SOURCE, 116, SELECTOR],
        &[32, 209, 135, 112, 210, 124],
    );
    if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&ROOT).unwrap().body {
        fields[1] = oop(9);
    }
    image
}
fn packed(words: &[u16], dimensions: (i32, i32)) -> Vec<u32> {
    let (width, height) = dimensions;
    let stride16 = (width + 15) / 16;
    let stride32 = (width + 31) / 32;
    let mut output = vec![0; (stride32 * height) as usize];
    for y in 0..height {
        for x in 0..width {
            if words[(y * stride16 + x / 16) as usize] & (1 << (15 - x % 16)) != 0 {
                output[(y * stride32 + x / 32) as usize] |= 1 << (31 - x % 32);
            }
        }
    }
    output
}
#[test]
fn bitblt_refreshes_registered_display_and_cursor_through_devices() -> Result<()> {
    for primitive in [101, 102] {
        let case = Case {
            dimensions: if primitive == 101 { (16, 16) } else { (37, 7) },
            overlap: true,
            alias: true,
            rule: 6,
            ..Default::default()
        };
        let image = registered(&case, primitive);
        let pixels = expected(&image, &case);
        let wanted = packed(&pixels, case.dimensions);
        let m = native(&image, bitmap_device())?;
        assert_eq!((m.cpu.rf[15], m.objekt.state.vr[5]), (1, r(RECEIVER)));
        let device = if primitive == 101 {
            &m.devices.cursor_bitmap
        } else {
            &m.devices.display_bitmap
        }
        .as_ref()
        .unwrap();
        assert_eq!(device.publications, 2);
        assert_eq!(device.visible.as_ref().unwrap().words, wanted);
        let e = execute_with_device(image, ROOT, true, 512, |_| {}, bitmap_device())?;
        assert_eq!((e.status, e.result), (1, r(RECEIVER)));
        assert_eq!(
            e.bitmap_publications,
            if primitive == 101 { [2, 0] } else { [0, 2] }
        );
        let frame = if primitive == 101 {
            e.cursor_frame
        } else {
            e.display_frame
        }
        .unwrap();
        assert_eq!(frame.words, wanted);
    }
    Ok(())
}

fn field(image: &mut source::Image, object: u16, index: usize, value: u16) {
    let source::Body::Pointers(fields) = &mut image.objects.get_mut(&object).unwrap().body else {
        panic!("pointer fixture")
    };
    fields[index] = value;
}
#[test]
fn bitblt_invalid_operands_preserve_destination_and_fallback_receiver() -> Result<()> {
    // An XOR would expose accidental writes before validation or a repeated draw.
    for invalid in 0..14 {
        let case = Case {
            rule: 6,
            ..Default::default()
        };
        let mut image = make(&case);
        match invalid {
            0 => field(&mut image, RECEIVER, 3, oop(16)),
            1 => field(&mut image, RECEIVER, 3, oop(-1)),
            2 => field(&mut image, RECEIVER, 4, 2),
            3 => field(&mut image, DEST, 1, oop(-1)),
            4 => field(&mut image, SOURCE, 0, 2),
            5 => field(&mut image, HALF, 1, oop(15)),
            6 => words(&mut image, SOURCE_BITS, vec![0; 20]),
            7 => pointers(&mut image, DEST_BITS, 16, vec![oop(0); 21]),
            8 => field(&mut image, RECEIVER, 0, oop(3)),
            9 => field(&mut image, RECEIVER, 1, oop(3)),
            10 => {
                primitive_method(&mut image, 96, 1, &[116, 124]);
                method(&mut image, CALLER, 0, 0, &[SELECTOR], &[112, 118, 224, 124]);
            }
            11 => {
                dictionary(&mut image, 12, 2, 242, 244, &[(SELECTOR, CALLEE)]);
                field(&mut image, ROOT, 5, oop(7));
            }
            12 => pointers(&mut image, RECEIVER, CLASS, vec![DEST; 13]),
            13 => {
                image.objects.get_mut(&RECEIVER).unwrap().body = source::Body::Words(vec![0; 14]);
            }
            _ => unreachable!(),
        }
        let before = target::Image::convert(&image, &[ROOT])?
            .records
            .into_iter()
            .find(|r| r.source_oop == DEST_BITS)
            .unwrap()
            .body;
        let m = native(&image, bitmap_device())?;
        assert_eq!(
            (m.cpu.rf[15], m.objekt.state.vr[5]),
            (1, i(-1)),
            "invalid {invalid}"
        );
        assert_eq!(body(&m, DEST_BITS), before, "invalid {invalid}");
        assert_eq!(m.devices.display_bitmap.as_ref().unwrap().publications, 0);
        let e = execute_with_device(image, ROOT, true, 512, |_| {}, bitmap_device())?;
        assert_eq!((e.status, e.result), (1, i(-1)), "invalid {invalid}");
        assert_eq!(
            e.records[&r(DEST_BITS).identity()?].body,
            before,
            "invalid {invalid}"
        );
        assert_eq!(e.bitmap_publications, [0, 0]);
    }
    Ok(())
}

#[test]
fn bitblt_rejects_late_nonword_data_before_any_write() -> Result<()> {
    for corrupt in [Word::raw(0x10000)?, r(SOURCE)] {
        let image = make(&Case::default());
        let before = bits(&image, DEST_BITS);
        // Converted word bodies may be modified by guest code. Put invalid data
        // at the last used component, outside this copy's rectangle, to verify
        // that validation finishes before drawing the first destination word.
        let prepare = |converted: &mut target::Image| {
            *converted
                .records
                .iter_mut()
                .find(|r| r.source_oop == SOURCE_BITS)
                .unwrap()
                .body
                .last_mut()
                .unwrap() = corrupt;
        };
        let m = native_prepared(&image, bitmap_device(), prepare)?;
        assert_eq!((m.cpu.rf[15], m.objekt.state.vr[5]), (1, i(-1)));
        assert_eq!(
            body(&m, DEST_BITS)[1..]
                .iter()
                .map(|w| w.bits() as u16)
                .collect::<Vec<_>>(),
            before
        );
        let e = execute_with_device(image, ROOT, true, 512, prepare, bitmap_device())?;
        assert_eq!((e.status, e.result), (1, i(-1)));
        assert_eq!(
            e.records[&r(DEST_BITS).identity()?].body[1..]
                .iter()
                .map(|w| w.bits() as u16)
                .collect::<Vec<_>>(),
            before
        );
        assert_eq!(e.bitmap_publications, [0, 0]);
    }
    Ok(())
}
