//! Bulk and stream operations must retain guest bounds and fallback behavior.
use super::*;

#[test]
fn replace_with_a_compact_operand_fails_before_touching_destination() -> Result<()> {
    let mut f = Fixture::new();
    let bytes = f.object(f.special[26], Body::Bytes(vec![10, 20]), 8);
    let selector = f.primitive(105, 4, &[122]);
    if let Body::Pointers(fields) = &mut f.image.objects.get_mut(&f.special[26]).unwrap().body {
        fields[0] = f.class;
    }
    f.start(
        &[32, 118, 118, 118, 118, 132, 4, 1, 124],
        vec![bytes, selector],
        vec![],
    );
    for jit in [false, true] {
        let m = run(&f, jit)?;
        assert_eq!(m.objekt.state.vr[5], target::reference(f.special[1])?);
        assert_eq!(
            body(&m, target::reference(bytes)?)?[2..],
            [Word::raw(10)?, Word::raw(20)?]
        );
    }
    Ok(())
}

#[test]
fn streams_advance_only_after_a_valid_read_or_write() -> Result<()> {
    for kind in [0, 1, 2] {
        for (primitive, index, value, expected_index, success) in [
            (65, 0, 0, 1, true),
            (65, 2, 0, 2, false),
            (66, 0, 42, 1, true),
            (66, 2, 42, 2, false),
        ] {
            let mut f = Fixture::new();
            let collection = match kind {
                0 => f.pointers(f.special[7], vec![int(17), int(18)]),
                1 => f.object(f.special[26], Body::Bytes(vec![17, 18]), 8),
                _ => f.object(f.special[4], Body::Words(vec![17, 18]), 6),
            };
            let stream = f.pointers(f.class, vec![collection, int(index), int(2), int(2)]);
            let selector = f.primitive(primitive, u32::from(primitive == 66), &[122]);
            if primitive == 66 {
                f.start(
                    &[32, 33, 226, 124],
                    vec![stream, int(value), selector],
                    vec![],
                );
            } else {
                f.start(&[32, 209, 124], vec![stream, selector], vec![]);
            }
            for jit in [false, true] {
                let m = run(&f, jit)?;
                let expected = if !success {
                    target::reference(f.special[1])?
                } else {
                    Word::signed(if primitive == 65 { 17 } else { 42 })
                };
                assert_eq!(
                    m.objekt.state.vr[5], expected,
                    "kind={kind} primitive={primitive} index={index}"
                );
                assert_eq!(
                    body(&m, target::reference(stream)?)?[3],
                    Word::signed(expected_index)
                );
                let expected = if primitive == 66 && success { 42 } else { 17 };
                assert_eq!(
                    body(&m, target::reference(collection)?)?[2],
                    if kind == 0 {
                        Word::signed(expected)
                    } else {
                        Word::raw(expected as u64)?
                    }
                );
            }
        }
    }
    Ok(())
}
