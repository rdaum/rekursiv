use rekursiv_asm::Word;
use rekursiv_smalltalk::{import_distribution, layout, source, target};
use std::collections::BTreeMap;

fn words(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

/// Invented object graph, not extracted Xerox content. Includes a metaclass
/// cycle, a shared cyclic pair, distinct singleton classes and mixed methods.
fn fixture_objects() -> BTreeMap<u16, (u16, bool, Vec<u8>)> {
    let mut objects = BTreeMap::new();
    for (oop, spec) in [
        (12, 0x4001),
        (14, 0x2001),
        (16, 0xe001),
        (20, 0x4005),
        (22, 0xe00d),
        (24, 0xe00d),
        (34, 0x2001),
        (52, 0xc001),
        (54, 0xc001),
        (56, 0xc001),
        (60, 0xc007),
    ] {
        objects.insert(oop, (60, true, words(&[2, 2, spec])));
    }
    for (oop, class) in [(2, 52), (4, 54), (6, 56)] {
        objects.insert(oop, (class, true, vec![]));
    }
    for (oop, fields) in [
        (8, vec![2, 80]),
        (80, vec![2, 82]),
        (82, vec![2, 84]),
        (86, vec![88, 88, 2, 4, 6, 0x8001, 0x7fff, 0xffff, 1]),
        (88, vec![86]),
    ] {
        objects.insert(oop, (16, true, words(&fields)));
    }
    objects.insert(84, (22, true, words(&[2, 15, 1, 64, 2, 86])));
    // Extended header: two literals, primitive 128, followed by raw bytes
    // that would look like references or integers if decoded as pointers.
    let mut method = words(&[0xe005, 0x0101, 86]);
    method.extend([0, 88, 0xff, 0xff, 0x80]);
    objects.insert(64, (34, false, method));
    objects.insert(66, (34, false, words(&[0x8001]))); // flag 4, no literals
    objects.insert(90, (14, false, vec![0, 86, 0xff, 0xfe, 1]));
    objects.insert(92, (20, false, words(&[0x8000, 0xffff])));
    objects
}

fn fixture() -> Vec<u8> {
    encode(fixture_objects())
}

fn encode(objects: BTreeMap<u16, (u16, bool, Vec<u8>)>) -> Vec<u8> {
    let table_words = usize::from(*objects.last_key_value().unwrap().0) + 2;
    let mut table = words(&[32, 0]).repeat(table_words / 2);
    let mut heap = Vec::new();
    for (oop, (class, pointers, data)) in objects {
        let address = heap.len() / 2;
        let size = 2 + data.len().div_ceil(2);
        let flags = 0x0100
            | ((address >> 16) as u16)
            | if pointers { 64 } else { 0 }
            | if data.len() % 2 != 0 { 128 } else { 0 };
        table[usize::from(oop) * 2..usize::from(oop) * 2 + 4]
            .copy_from_slice(&words(&[flags, address as u16]));
        heap.extend(words(&[size as u16, class]));
        heap.extend(&data);
        if data.len() % 2 != 0 {
            heap.push(0xa5);
        } // unused low byte is not content
    }
    let mut result = vec![0; 512];
    result[..4].copy_from_slice(&((heap.len() / 2) as u32).to_be_bytes());
    result[4..8].copy_from_slice(&(table_words as u32).to_be_bytes());
    result.extend(heap);
    result.resize(result.len().div_ceil(512) * 512, 0);
    result.extend(table);
    result
}

fn table_start(image: &[u8]) -> usize {
    (512 + u32::from_be_bytes(image[..4].try_into().unwrap()) as usize * 2).div_ceil(512) * 512
}

#[test]
fn graph_conversion_preserves_cycles_sharing_singletons_and_gc_edges() {
    let source = source::Image::parse(&fixture()).unwrap();
    let target = target::Image::convert(&source, &[2, 4, 6, 8, 86]).unwrap();
    target.verify_against(&source).unwrap();
    let records: BTreeMap<_, _> = target.records.iter().map(|r| (r.source_oop, r)).collect();
    assert_eq!(records[&86].body[1], records[&88].reference);
    assert_eq!(records[&86].body[2], records[&88].reference);
    assert_eq!(records[&88].body[1], records[&86].reference);
    assert_ne!(records[&2].class, records[&4].class);
    assert_ne!(records[&4].class, records[&6].class);
    assert_ne!(records[&2].reference, Word::NIL);
    assert!(target
        .records
        .iter()
        .all(|r| r.reference.bits() & (1 << 37) != 0));
    assert_eq!(
        target.compact_classes,
        [None, None, Some(records[&12].reference), None]
    );
    assert_eq!(source.initial_context().unwrap(), 84);
    assert_eq!(target.next_identity, 32768);
    assert_eq!(target.primitives.get(&128), Some(&1));
}

#[test]
fn methods_and_opaque_data_cannot_invent_references() {
    let source = source::Image::parse(&fixture()).unwrap();
    let target = target::Image::convert(&source, &[]).unwrap();
    let method = target.records.iter().find(|r| r.source_oop == 64).unwrap();
    assert_eq!(method.body[1], Word::signed(-4094)); // encoded method header 0xe005
    assert_eq!(method.body[2], Word::signed(128));
    assert_eq!(method.body[3], layout::reference(86).unwrap());
    assert_eq!(
        method.body[4..]
            .iter()
            .map(|w| w.bits())
            .collect::<Vec<_>>(),
        vec![0, 88, 255, 255, 128]
    );
    for oop in [90, 92] {
        let object = target.records.iter().find(|r| r.source_oop == oop).unwrap();
        assert!(object
            .body
            .iter()
            .all(|w| !w.is_reference() && !w.is_compact()));
    }
    let mut broken = target.clone();
    broken
        .records
        .iter_mut()
        .find(|r| r.source_oop == 64)
        .unwrap()
        .body[4] = layout::reference(88).unwrap();
    assert!(broken.verify_against(&source).is_err());
}

#[test]
fn every_small_integer_round_trips_without_widening_guest_range() {
    for v in layout::SMALL_INTEGER_MIN..=layout::SMALL_INTEGER_MAX {
        let oop = layout::integer_oop(v).unwrap();
        assert_eq!(layout::small_integer(oop).unwrap(), v);
        assert_eq!(
            target::source_oop(target::value(oop).unwrap()).unwrap(),
            oop
        );
    }
    for v in [-16385, 16384, i32::MIN, i32::MAX] {
        assert!(layout::integer_oop(v).is_err());
        assert!(target::source_oop(Word::signed(v)).is_err());
    }
    assert!(target::source_oop(Word::NIL).is_err());
    assert!(target::source_oop(Word::boolean(true)).is_err());
}

#[test]
fn identities_and_hashes_do_not_depend_on_physical_location() {
    for oop in (2..=65534).step_by(2) {
        let reference = layout::reference(oop).unwrap();
        assert_eq!(
            reference.identity().unwrap(),
            u64::from(layout::identity_hash(oop))
        );
        assert_eq!(target::source_oop(reference).unwrap(), oop);
    }
    assert!(layout::reference(0).is_err());
    assert!(layout::reference(1).is_err());
    assert!(target::source_oop(Word::reference(1, false).unwrap()).is_err());
}

#[test]
fn serialized_bundle_contains_machine_words_and_boot_metadata() {
    let source = source::Image::parse(&fixture()).unwrap();
    let target = target::Image::convert(&source, &[86]).unwrap();
    let mut output = Vec::new();
    target.write_json(&mut output).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["format_version"], 1);
    assert_eq!(json["next_identity"], 32768);
    assert_eq!(json["roots"][0], layout::reference(86).unwrap().bits());
    for (encoded, record) in json["records"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&target.records)
    {
        assert_eq!(encoded["reference"], record.reference.bits());
        assert_eq!(encoded["class"], record.class.bits());
        assert_eq!(
            encoded["body"],
            serde_json::json!(record.body.iter().map(|w| w.bits()).collect::<Vec<_>>())
        );
    }
}

#[test]
fn page_aligned_object_space_and_tape_padding_are_supported() {
    let mut objects = fixture_objects();
    let current: usize = objects
        .values()
        .map(|(_, _, d)| 2 + d.len().div_ceil(2))
        .sum();
    let filler_words = 256 - ((current + 2) % 256);
    objects.insert(94, (14, false, vec![0; filler_words * 2]));
    let bytes = encode(objects);
    assert_eq!(u32::from_be_bytes(bytes[..4].try_into().unwrap()) % 256, 0);
    source::Image::parse(&bytes).unwrap();
    let mut padded = bytes.clone();
    padded.resize(padded.len().div_ceil(2048) * 2048, 0);
    source::Image::parse(&padded).unwrap();
    *padded.last_mut().unwrap() = 1;
    assert!(source::Image::parse(&padded).is_err());
}

#[test]
fn malformed_and_unsupported_images_are_errors() {
    let original = fixture();
    for n in [0, 1, 8, 511, 512, original.len() - 1] {
        assert!(source::Image::parse(&original[..n]).is_err());
    }
    for position in [8, 9, 511] {
        let mut bad = original.clone();
        bad[position] = 1;
        assert!(source::Image::parse(&bad)
            .unwrap_err()
            .0
            .contains("unsupported image format"));
    }
    let mut bad = original.clone();
    bad[..4].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(source::Image::parse(&bad).is_err());
    let mut bad = original.clone();
    let off = table_start(&bad);
    bad[off + 1] = 0;
    assert!(source::Image::parse(&bad).is_err()); // oop zero cannot be allocated
    let mut bad = original.clone();
    bad[off + 4 + 1] |= 16;
    assert!(source::Image::parse(&bad).is_err()); // reserved entry flag
    let mut bad = original.clone();
    bad[512] = 0xff;
    bad[513] = 0xff;
    assert!(source::Image::parse(&bad).is_err()); // invalid extent
    let mut objects = fixture_objects();
    objects.get_mut(&88).unwrap().2 = words(&[0]);
    assert!(source::Image::parse(&encode(objects))
        .unwrap_err()
        .0
        .contains("dangling reference"));
    let mut objects = fixture_objects();
    objects.get_mut(&64).unwrap().2 = words(&[0xe001]);
    assert!(source::Image::parse(&encode(objects))
        .unwrap_err()
        .0
        .contains("header extension"));
    let mut objects = fixture_objects();
    objects.get_mut(&64).unwrap().2 = words(&[0x007f]);
    assert!(source::Image::parse(&encode(objects))
        .unwrap_err()
        .0
        .contains("literal frame"));
    assert!(import_distribution(&original)
        .unwrap_err()
        .0
        .contains("unsupported image"));
}

#[test]
fn reference_counts_are_not_import_liveness() {
    let mut bytes = fixture();
    let off = table_start(&bytes);
    for entry in bytes[off..].as_chunks_mut::<4>().0 {
        entry[0] = 0;
    }
    let source = source::Image::parse(&bytes).unwrap();
    assert_eq!(source.objects.len(), fixture_objects().len());
}

#[test]
#[ignore = "requires pinned external image; run scripts/check-smalltalk-image.sh"]
fn xerox_distribution_converts_every_object_and_matches_tape_metadata() {
    let directory = std::env::var_os("REKURSIV_ST80_DIR")
        .expect("set REKURSIV_ST80_DIR to the fetched distribution directory");
    let directory = std::path::PathBuf::from(directory);
    let bytes = std::fs::read(directory.join("VirtualImage")).unwrap();
    let source = source::Image::parse(&bytes).unwrap();
    let target = import_distribution(&bytes).unwrap();
    assert_eq!(source.objects.len(), 18391);
    assert_eq!(
        target.records.iter().map(|r| r.body.len()).sum::<usize>(),
        341467
    );
    assert_eq!(source.initial_context().unwrap(), 0x2b28);
    assert_eq!(
        target.roots[layout::ACTIVE_CONTEXT_ROOT],
        layout::reference(0x2b28).unwrap()
    );
    assert_eq!(target.primitives.get(&128), Some(&1));
    assert_eq!(target.primitives.get(&135), Some(&1));
    assert_eq!(target.primitives.get(&0), Some(&71));
    let mut counts = [0; 4];
    for o in source.objects.values() {
        counts[match o.body {
            source::Body::Pointers(_) => 0,
            source::Body::Words(_) => 1,
            source::Body::Bytes(_) => 2,
            source::Body::Method { .. } => 3,
        }] += 1;
    }
    assert_eq!(counts, [7607, 359, 5920, 4505]);
    // Independent distribution lists exercise the parser's oop/address mapping.
    // The tape lists fewer methods than the image contains; do not drop the rest.
    for file in ["class.oops", "method.oops"] {
        let listing = std::fs::read_to_string(directory.join(file)).unwrap();
        let mut seen = 0;
        let mut stale = Vec::new();
        for line in listing.lines().skip(1) {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 2 {
                continue;
            }
            let oop = u16::from_str_radix(fields[1].strip_prefix("16r").unwrap(), 16).unwrap();
            let Ok(object) = source.object(oop) else {
                stale.push(oop);
                continue;
            };
            if file == "method.oops" {
                assert!(matches!(object.body, source::Body::Method { .. }));
            } else {
                let expected = fields[2..].join(" ");
                let (class, expected) = if let Some(name) = expected.strip_suffix(" class") {
                    (source.object(object.pointer(6).unwrap()).unwrap(), name)
                } else {
                    (object, expected.as_str())
                };
                let name = source.object(class.pointer(6).unwrap()).unwrap();
                let source::Body::Bytes(name) = &name.body else {
                    panic!("class name must be bytes")
                };
                assert_eq!(name.as_slice(), expected.as_bytes());
            }
            seen += 1;
        }
        // The accompanying diagnostic lists include objects already freed in
        // VirtualImage. Pin these discrepancies rather than hiding new ones.
        if file == "class.oops" {
            assert_eq!(seen, 446);
            assert_eq!(stale, [0x93b0, 0x9670]);
        } else {
            assert_eq!(seen, 4493);
            assert_eq!(stale, [0x93f0, 0x9446, 0x9afc, 0x9c20, 0xabd0, 0xb6f8]);
        }
    }
    let context = source.object(0x2b28).unwrap();
    let ip = layout::small_integer(
        context
            .pointer(layout::context::INSTRUCTION_POINTER)
            .unwrap(),
    )
    .unwrap() as usize;
    let method = source
        .object(
            context
                .pointer(layout::context::METHOD_OR_ARGUMENT_COUNT)
                .unwrap(),
        )
        .unwrap();
    let source::Body::Method { header, bytes, .. } = &method.body else {
        panic!("active method")
    };
    assert_eq!(bytes[ip - header.initial_ip()], 131); // first bytecode in Xerox trace2
    target.verify_against(&source).unwrap();
}
