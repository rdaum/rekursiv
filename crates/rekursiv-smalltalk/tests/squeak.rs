//! Format and conversion regressions. Real distribution tests are explicit opt-ins.
#[path = "squeak/fixture.rs"]
mod fixture;

use fixture::{fixture, word};
use rekursiv_asm::Word;
use rekursiv_smalltalk::squeak::{
    self, audit,
    image::{Body, Endian, Image},
    layout, target,
};

#[test]
fn headers_endianness_cycles_and_opaque_words_round_trip() {
    let mut prior = None;
    for endian in [Endian::Little, Endian::Big] {
        let f = fixture(endian);
        let image = Image::parse(&f.bytes).unwrap();
        assert_eq!(image.objects.len(), 14);
        assert_eq!(image.free_bytes, 8);
        assert_eq!(image.initial_context().unwrap(), f.oops[9]);
        assert_eq!(image.object(f.oops[1]).unwrap().class, f.oops[0]);
        assert_eq!(
            image.object(f.oops[12]).unwrap().body,
            Body::Bytes(vec![0x40, 0, 0, 4, 0xff])
        );
        let converted = target::Image::convert(&image).unwrap();
        converted.verify_against(&image).unwrap();
        assert_eq!(converted.profile, "squeak-1.1");
        let record = &converted.records[13];
        assert_eq!(record.body[2], record.reference); // self cycle
        assert_eq!(record.body[3], record.body[4]); // sharing
        assert_eq!(record.body[5], Word::signed(-(1 << 30)).bits());
        assert_eq!(record.body[6], Word::signed((1 << 30) - 1).bits());
        assert_eq!(
            &converted.records[11].body[2..],
            &[0x7ff8_1234, 0x5678_abcd]
        );
        assert_eq!(converted.records[1].body[1], converted.records[2].body[1]);
        assert_ne!(
            converted.records[1].reference,
            converted.records[2].reference
        ); // equal hashes, different identities
        let Body::Method {
            header,
            literals,
            bytes,
        } = &image.object(f.oops[10]).unwrap().body
        else {
            panic!()
        };
        assert_eq!(header.primitive(), 511);
        assert_eq!(header.argument_count(), 1);
        assert_eq!(header.temporary_count(), 2);
        assert_eq!(header.initial_ip(), 17);
        assert_eq!(literals.len(), 3);
        assert_eq!(bytes.len(), 301);
        if let Some(prior) = &prior {
            assert_eq!(&image.objects, prior);
        }
        prior = Some(image.objects);
    }
}

#[test]
fn reject_corrupt_extents_headers_and_edges() {
    let f = fixture(Endian::Little);
    let mut cases = Vec::new();
    for (offset, value) in [
        (0, 6504),
        (4, 16),
        (8, u32::MAX - 3),
        (12, u32::MAX - 3),
        (16, f.oops[4] + 4),    // specialObjects is an interior address
        (f.base_headers[1], 7), // short header with no compact class
        (f.base_headers[0] - 4, f.oops[0] | 2), // wrong class header type
        (f.base_headers[0], (5 << 8) | 16 | 1), // unsupported format
        (f.base_headers[10] - 8, 0), // empty long object
        (f.base_headers[10] - 8, u32::MAX - 3), // overflowing extent
        (f.base_headers[10] + 4, 2), // untagged method header
        (f.base_headers[10] + 4, (255 << 10) | 1), // literals exceed method body
        (f.base_headers[13] + 4, 0xdead_beec), // dangling pointer
        (f.base_headers[13] + 4, f.oops[13] + 4), // interior pointer
        (f.bytes.len() - 8, 2), // zero-length free chunk
        (f.base_headers[5] + 4, 3), // compact class is an integer
    ] {
        let mut bytes = f.bytes.clone();
        word(&mut bytes, offset, value, Endian::Little);
        cases.push(bytes);
    }
    for (case, bytes) in cases.iter().enumerate() {
        assert!(
            Image::parse(bytes).is_err(),
            "accepted corrupt fixture {case}"
        );
    }
    for length in [0, 1, 4, 31, 63, 64, f.bytes.len() - 1] {
        assert!(Image::parse(&f.bytes[..length]).is_err());
    }
}

#[test]
fn converted_graph_verification_detects_mutation() {
    let f = fixture(Endian::Little);
    let source = Image::parse(&f.bytes).unwrap();
    let converted = target::Image::convert(&source).unwrap();
    for kind in 0..6 {
        let mut bad = converted.clone();
        match kind {
            0 => bad.records[13].body[3] = bad.records[1].reference,
            1 => bad.records[11].body[2] = bad.records[1].reference,
            2 => bad.records[13].body[1] ^= 1,
            3 => bad.roots[0] = 0,
            4 => bad.next_identity -= 1,
            _ => bad.records[10].body[0] += 4,
        }
        assert!(bad.verify_against(&source).is_err());
    }
    assert!(target::reference(0).is_err());
    assert!(target::reference(2).is_err());
    assert_eq!(layout::small_integer(0x8000_0001).unwrap(), -(1 << 30));
    assert_eq!(layout::small_integer(0x7fff_ffff).unwrap(), (1 << 30) - 1);
}

#[test]
#[ignore = "requires the pinned distribution; set REKURSIV_SQUEAK_DIR"]
fn pinned_squeak_image_converts_every_object_and_exposes_actual_startup() {
    let dir = std::env::var_os("REKURSIV_SQUEAK_DIR").expect("set REKURSIV_SQUEAK_DIR");
    let bytes = std::fs::read(std::path::Path::new(&dir).join("Squeak1.1.image")).unwrap();
    assert_eq!(rekursiv_smalltalk::checksum(&bytes), squeak::IMAGE_SHA256);
    let source = Image::parse(&bytes).unwrap();
    let report = audit::inventory(&source).unwrap();
    assert_eq!(
        (report.objects, report.methods, report.floats),
        (31737, 7489, 1255)
    );
    assert_eq!(report.header.window_size, (640, 480));
    assert_eq!(report.startup.context, 0x911b18);
    assert_eq!(report.startup.method, 0x843504);
    assert_eq!(
        report.startup.bindings,
        ["SystemDictionary>>snapshot:andQuit:"]
    );
    assert_eq!(
        (
            report.startup.instruction_pointer,
            report.startup.stack_pointer,
            report.startup.next_bytecode
        ),
        (150, 5, 129)
    );
    let display = report.display.unwrap();
    assert_eq!(
        (
            display.width,
            display.height,
            display.depth,
            display.bitmap_words
        ),
        (240, 120, 8, 7200)
    );
    assert_eq!(report.primitives.len(), 182);
    assert!(report.primitives[&135].iter().any(|m| m
        .bindings
        .iter()
        .any(|b| b == "Time class>>primMillisecondClock")));
    assert_eq!(report.primitives[&256].len(), 109);
    let converted = target::Image::convert(&source).unwrap();
    converted.verify_against(&source).unwrap();
    for record in &converted.records {
        if source.object(record.source_oop).unwrap().class
            == source.special(layout::special::FLOAT_CLASS).unwrap()
        {
            assert_eq!(record.body.len(), 4); // descriptor, hash/format, two untouched 32-bit words
        }
    }
}
