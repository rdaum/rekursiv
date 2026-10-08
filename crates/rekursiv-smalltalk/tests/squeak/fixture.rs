//! Invented 32-bit graph covering all three allocated header types and a free chunk.
use rekursiv_smalltalk::squeak::image::Endian;

pub struct Fixture {
    pub bytes: Vec<u8>,
    pub oops: Vec<u32>,
    pub base_headers: Vec<usize>,
}

pub fn word(bytes: &mut [u8], offset: usize, value: u32, endian: Endian) {
    bytes[offset..offset + 4].copy_from_slice(&match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    });
}

pub fn fixture(endian: Endian) -> Fixture {
    let base = 0x4000_0000;
    let sizes: [usize; 14] = [12, 0, 0, 0, 124, 124, 8, 8, 8, 72, 317, 8, 5, 24];
    let formats = [1, 0, 0, 0, 2, 2, 1, 1, 1, 3, 15, 6, 11, 2];
    let mut base_headers = Vec::new();
    let mut oops = Vec::new();
    let mut cursor = 64;
    for (index, size) in sizes.iter().enumerate() {
        let extra = match index {
            1..=3 => 0,
            10 => 8,
            _ => 4,
        };
        base_headers.push(cursor + extra);
        oops.push(base + (cursor + extra - 64) as u32);
        cursor += extra + 4 + size.div_ceil(4) * 4;
    }
    let mut bytes = vec![0; cursor + 8];
    let heap_size = (bytes.len() - 64) as u32;
    for (i, v) in [
        6502,
        64,
        heap_size,
        base,
        oops[4],
        1234,
        (640 << 16) | 480,
        0,
    ]
    .iter()
    .enumerate()
    {
        word(&mut bytes, i * 4, *v, endian);
    }
    for (index, size) in sizes.iter().enumerate() {
        let pos = base_headers[index];
        let typ = match index {
            1..=3 => 3,
            10 => 0,
            _ => 1,
        };
        let stored_size = (4 + size.div_ceil(4) * 4) as u32;
        let bits = typ
            | (formats[index] << 8)
            | (77 << 17)
            | if typ == 0 { 0 } else { stored_size }
            | if typ == 3 { 1 << 12 } else { 0 };
        word(&mut bytes, pos, bits, endian);
        if typ != 3 {
            word(&mut bytes, pos - 4, oops[0] | typ, endian);
        }
        if typ == 0 {
            word(&mut bytes, pos - 8, stored_size, endian);
        }
    }
    // Free chunk has no class and cannot be a reference target.
    word(&mut bytes, cursor, 8 | 2, endian);
    let small = |n: i32| ((n as u32) << 1) | 1;
    let nil = oops[1];
    let mut specials = vec![nil; 31];
    for (slot, object) in [(0, 1), (1, 2), (2, 3), (3, 6), (5, 0), (9, 0), (28, 5)] {
        specials[slot] = oops[object];
    }
    let mut context = vec![nil; 18];
    context[..6].copy_from_slice(&[nil, small(17), small(2), oops[10], nil, oops[13]]);
    let method_header = ((1 << 24) | (2 << 18) | (3 << 9) | 511) * 2 + 1;
    for (index, fields) in [
        (0, vec![oops[0], nil, small(2)]),
        (4, specials),
        (5, vec![oops[0]; 31]),
        (6, vec![nil, oops[7]]),
        (7, vec![nil, oops[8]]),
        (8, vec![nil, oops[9]]),
        (9, context),
        (10, vec![method_header, oops[13], oops[11], small(-7)]),
        (11, vec![0x7ff8_1234, 0x5678_abcd]), // binary64 NaN payload, never a guest pointer
        (
            13,
            vec![
                oops[13],
                oops[12],
                oops[12],
                small(-(1 << 30)),
                small((1 << 30) - 1),
                small(-1),
            ],
        ),
    ] {
        for (i, field) in fields.iter().enumerate() {
            word(&mut bytes, base_headers[index] + 4 + i * 4, *field, endian);
        }
    }
    // Method bytes and byte bodies retain byte order in either image endianness.
    let code = base_headers[10] + 4 + 16;
    bytes[code..code + 301].fill(120);
    bytes[code] = 112;
    let data = base_headers[12] + 4;
    bytes[data..data + 5].copy_from_slice(&[0x40, 0, 0, 4, 0xff]);
    bytes[data + 5..data + 8].fill(0xa5); // padding is not object content
    Fixture {
        bytes,
        oops,
        base_headers,
    }
}
