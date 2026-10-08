//! Mapped glyphs checked against scalar pixel expectations and the general
//! microcode path. All drawing still executes guest primitive 96.
use super::{
    graphics::{blit, form},
    *,
};
use rekursiv_asm::processor::Seq;

/// Keep the general path executable as a correctness and work-count baseline.
fn draw(f: &Fixture, jit: bool, fast: bool) -> Result<Machine> {
    let assembly = rekursiv_smalltalk::squeak::interpreter::assemble(&f.image)?;
    run_with(f, jit, |m| {
        if !fast {
            // Set frame31=0 normally, then bypass only the glyph selector.
            let pc = assembly.symbols["bb_choose_glyphs"] as usize + 1;
            let i = m.image_mut().code[pc].as_mut().unwrap();
            i.seq = Seq::Jump;
            i.branch = assembly.symbols["bb_choose_words"] as u16;
        }
    })
}

/// Destination padding, clipped pixels, and partial words start with nonzero
/// bytes so accidental writes outside the rectangle remain observable.
pub(super) fn fixture(rect: [i32; 10], halftone: &[u32]) -> (Fixture, u32, Vec<u32>) {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let (dw, dh, sw, sh) = (37, 5, 97, 4);
    let dest: Vec<u32> = (0..50).map(|n| 0x936c5aa5u32.rotate_left(n)).collect();
    let source: Vec<u32> = (0..16).map(|n| 0xa35c87e1u32.rotate_left(n * 7)).collect();
    let colours = [0x12340027u32, 0xabcdefd2]; // discard upper map bits
    let (dst, bits) = form(&mut f, dw, dh, 8, dest.clone());
    let (src, _) = form(&mut f, sw, sh, 1, source.clone());
    let map = f.object(f.special[4], Body::Words(colours.to_vec()), 6);
    let half = if halftone.is_empty() {
        nil
    } else {
        f.object(f.special[4], Body::Words(halftone.to_vec()), 6)
    };
    blit(&mut f, dst, src, half, 3, rect, map);
    let [dx, dy, w, h, sx, sy, cx, cy, cw, ch] = rect;
    let mut expected = dest;
    for y in 0..h {
        for x in 0..w {
            let (tx, ty, fx, fy) = (dx + x, dy + y, sx + x, sy + y);
            if !(0..dw).contains(&tx)
                || !(0..dh).contains(&ty)
                || !(0..sw).contains(&fx)
                || !(0..sh).contains(&fy)
                || !(cx..cx + cw).contains(&tx)
                || !(cy..cy + ch).contains(&ty)
            {
                continue;
            }
            let bit = (source[(fy * 4 + fx / 32) as usize] >> (31 - fx % 32)) & 1;
            let shift = (3 - tx % 4) * 8;
            let half = if halftone.is_empty() {
                u32::MAX
            } else {
                halftone[ty as usize % halftone.len()]
            };
            let byte = colours[bit as usize] & 255 & (half >> shift);
            let word = &mut expected[(ty * 10 + tx / 4) as usize];
            *word = (*word & !(255 << shift)) | (byte << shift);
        }
    }
    (f, bits, expected)
}

fn pixels(m: &Machine, bits: u32) -> Result<Vec<u32>> {
    Ok(body(m, target::reference(bits)?)?[2..]
        .iter()
        .map(|w| w.bits() as u32)
        .collect())
}

#[test]
fn mapped_glyphs_preserve_edges_across_source_and_destination_alignments() -> Result<()> {
    for sx in 0..32 {
        let dx = sx % 4;
        let half: &[u32] = if sx % 2 == 0 {
            &[]
        } else {
            &[0x0ff0aa55, 0x807f33cc, 0xfedcba98]
        };
        // Alternate full and short rows, including a row ending exactly at a
        // source word boundary. Height and stride differ between the Forms.
        let width = if sx % 3 == 0 { 32 - sx } else { 33 - dx };
        let (f, bits, expected) = fixture([dx, 1, width, 3, sx, 0, 0, 0, 37, 5], half);
        for jit in [false, true] {
            assert_eq!(
                pixels(&draw(&f, jit, true)?, bits)?,
                expected,
                "sx={sx} jit={jit}"
            );
        }
    }
    Ok(())
}

#[test]
fn mapped_glyphs_clip_negative_coordinates_and_match_the_general_path() -> Result<()> {
    for rect in [
        [-3, -1, 43, 7, 29, 0, 1, 1, 33, 3],
        [1, 1, 40, 4, -3, -1, 0, 0, 37, 5],
        [2, 0, 36, 5, 90, 1, 0, 0, 37, 5],
        [3, 1, 1, 1, 31, 1, 0, 0, 37, 5],
        [0, 0, 0, 1, 0, 0, 0, 0, 37, 5],
    ] {
        let (f, bits, expected) = fixture(rect, &[0x0ff00ff0, 0xf00ff00f]);
        for jit in [false, true] {
            for fast in [false, true] {
                assert_eq!(
                    pixels(&draw(&f, jit, fast)?, bits)?,
                    expected,
                    "rect={rect:?} jit={jit} fast={fast}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn mapped_glyphs_reduce_instructions_and_object_traffic() -> Result<()> {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let (dst, bits) = form(&mut f, 256, 8, 8, vec![0x55555555; 512]);
    let (src, _) = form(&mut f, 256, 8, 1, vec![0xa35c87e1; 64]);
    let map = f.object(f.special[4], Body::Words(vec![0x27, 0xd2]), 6);
    blit(
        &mut f,
        dst,
        src,
        nil,
        3,
        [1, 0, 254, 8, 1, 0, 0, 0, 256, 8],
        map,
    );
    for jit in [false, true] {
        let general = draw(&f, jit, false)?;
        let glyphs = draw(&f, jit, true)?;
        assert_eq!(pixels(&glyphs, bits)?, pixels(&general, bits)?);
        eprintln!(
            "glyph jit={jit}: {} -> {} instructions; {} -> {} object commands",
            general.stats.retired,
            glyphs.stats.retired,
            general.stats.object_commands,
            glyphs.stats.object_commands
        );
        assert!(glyphs.stats.retired * 3 < general.stats.retired);
        assert!(glyphs.stats.object_commands * 3 < general.stats.object_commands);
    }
    Ok(())
}

#[test]
fn mapped_glyphs_keep_source_alias_snapshots_and_do_not_cache_destination_maps() -> Result<()> {
    for jit in [false, true] {
        let mut f = Fixture::new();
        let nil = f.special[0];
        let (dst, bits) = form(&mut f, 8, 1, 8, vec![0x01020304, 0x112233aa]);
        let (src, _) = form(&mut f, 8, 1, 1, vec![0x10000000]);
        // Map entries change as destination pixels are written. This unusual
        // but valid alias must keep the general path's sequential reads.
        blit(
            &mut f,
            dst,
            src,
            nil,
            3,
            [0, 0, 8, 1, 0, 0, 0, 0, 8, 1],
            bits,
        );
        assert_eq!(
            pixels(&draw(&f, jit, true)?, bits)?,
            [0x040404aa, 0xaaaaaaaa]
        );

        let mut f = Fixture::new();
        let nil = f.special[0];
        let (dst, bits) = form(&mut f, 8, 1, 8, vec![0xa35c87e1, 0x112233aa]);
        let origin = f.pointers(f.special[12], vec![int(0), int(0)]);
        let src = f.pointers(f.class, vec![bits, int(64), int(1), int(1), origin]);
        let map = f.object(f.special[4], Body::Words(vec![0x27, 0xd2]), 6);
        blit(
            &mut f,
            dst,
            src,
            nil,
            3,
            [0, 0, 8, 1, 0, 0, 0, 0, 8, 1],
            map,
        );
        assert_eq!(
            pixels(&draw(&f, jit, true)?, bits)?,
            [0xd227d227, 0x2727d2d2]
        );
    }
    Ok(())
}
