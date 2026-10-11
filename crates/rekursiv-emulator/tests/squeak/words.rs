//! Word raster paths: scalar pixel oracle across packing, edges and aliases.
use super::graphics::{blit, form};
use super::*;

pub(super) fn fixture(
    depth: u32,
    rule: u32,
    rect: [i32; 10],
    alias: bool,
    fill: bool,
) -> (Fixture, u32, Vec<u32>) {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let width = 96 / depth;
    let height = 3;
    let initial: Vec<_> = (0..9).map(|n| 0x936c5aa5u32.rotate_left(n * 3)).collect();
    let source: Vec<_> = (0..9).map(|n| 0xa35c87e1u32.rotate_left(n * 7)).collect();
    let half = [0x0ff0aa55u32, 0x807f33cc, 0xfedcba98];
    let (dst, bits) = form(&mut f, width as i32, height, depth as i32, initial.clone());
    let src = if fill {
        nil
    } else if alias {
        dst
    } else {
        form(&mut f, width as i32, height, depth as i32, source.clone()).0
    };
    let halftone = f.object(f.special[4], Body::Words(half.to_vec()), 6);
    blit(&mut f, dst, src, halftone, rule as i32, rect, nil);
    let source = if alias { &initial } else { &source };
    let mask = u32::MAX >> (32 - depth);
    let ppw = (32 / depth) as i32;
    let [dx, dy, w, h, sx, sy, cx, cy, cw, ch] = rect;
    let mut expected = initial.clone();
    let ignored_source = [0, 5, 10, 15].contains(&rule);
    for y in 0..h {
        for x in 0..w {
            let (tx, ty, fx, fy) = (dx + x, dy + y, sx + x, sy + y);
            if !(0..width as i32).contains(&tx)
                || !(0..height).contains(&ty)
                || !(cx..cx + cw).contains(&tx)
                || !(cy..cy + ch).contains(&ty)
                || (!fill
                    && !ignored_source
                    && (!(0..width as i32).contains(&fx) || !(0..height).contains(&fy)))
            {
                continue;
            }
            let shift = 32 - depth * (tx as u32 % ppw as u32 + 1);
            let s = if fill || ignored_source {
                mask
            } else {
                (source[(fy * 3 + fx / ppw) as usize]
                    >> (32 - depth * (fx as u32 % ppw as u32 + 1)))
                    & mask
            } & (half[ty as usize % half.len()] >> shift);
            let index = (ty * 3 + tx / ppw) as usize;
            let d = (initial[index] >> shift) & mask;
            let merged = super::graphics::boolean(rule, s, d) & mask;
            expected[index] = (expected[index] & !(mask << shift)) | (merged << shift);
        }
    }
    (f, bits, expected)
}
fn verify(f: &Fixture, bits: u32, expected: &[u32], jit: bool) -> Result<()> {
    let m = run(f, jit)?;
    assert_eq!(
        body(&m, target::reference(bits)?)?[2..]
            .iter()
            .map(|w| w.bits() as u32)
            .collect::<Vec<_>>(),
        expected
    );
    Ok(())
}
#[test]
fn word_rasters_cover_all_boolean_rules_and_packed_alignments() -> Result<()> {
    // Every possible source bit offset, plus different destination offsets,
    // covers cross-word carry, 32-bit masks and exact-end source reads.
    for depth in [1, 2, 4, 8, 16, 32] {
        let ppw = 32 / depth;
        for sx in 0..ppw {
            let dx = (sx * 3 + 1) % ppw;
            for rule in [3, 6] {
                let rect = [
                    dx as i32,
                    0,
                    (96 / depth - dx.max(sx)) as i32,
                    3,
                    sx as i32,
                    0,
                    0,
                    0,
                    (96 / depth) as i32,
                    3,
                ];
                let (f, bits, expected) = fixture(depth, rule, rect, false, false);
                verify(&f, bits, &expected, false)?;
            }
        }
    }
    for rule in 0..16 {
        for (alias, fill) in [(false, false), (true, false), (false, true)] {
            let (f, bits, expected) =
                fixture(8, rule, [1, 1, 10, 2, 0, 0, 2, 0, 9, 3], alias, fill);
            verify(&f, bits, &expected, false)?;
        }
    }
    Ok(())
}
#[test]
fn word_raster_clipping_and_short_rows_match_native() -> Result<()> {
    for (depth, rect) in [
        (8, [-2, -1, 16, 5, 1, 0, 1, 1, 10, 2]),
        (8, [1, 1, 1, 1, 3, 2, 0, 0, 12, 3]),
        (1, [31, 0, 1, 1, 95, 2, 0, 0, 96, 3]),
        (32, [1, 0, 2, 3, 0, 0, 0, 0, 3, 3]),
    ] {
        let (f, bits, expected) = fixture(depth, 3, rect, false, false);
        for jit in [false, true] {
            verify(&f, bits, &expected, jit)?;
        }
    }
    Ok(())
}
