//! Bytecode, activation, block and message semantics.
use super::*;

#[test]
fn bytecode_fields_temporaries_and_double_extended_operations() -> Result<()> {
    let mut f = Fixture::new();
    let assoc = f.pointers(f.class, vec![f.special[0], int(100)]);
    // 132 push receiver field, store temp; push literal, store receiver;
    // push association, store-and-pop receiver; then push its new value.
    f.start(
        &[
            132, 64, 1, 104, 132, 96, 0, 132, 160, 0, 135, 132, 128, 1, 132, 192, 1, 1, 124,
        ],
        vec![int(123), assoc],
        vec![int(0)],
    );
    result(&f, Word::signed(100))?;
    let m = run(&f, false)?;
    let body = body(&m, target::reference(f.receiver)?)?;
    assert_eq!(&body[2..4], &[Word::signed(123), Word::signed(100)]);
    Ok(())
}

#[test]
fn normal_send_and_quick_returns_use_squeak_headers_and_stored_hashes() -> Result<()> {
    for primitive in [0, 256, 257, 258, 259, 260, 261, 262, 263, 264, 265] {
        let mut f = Fixture::new();
        let selector = f.primitive(primitive, 0, &[0, 124]);
        f.start(&[112, 134, 0, 124], vec![selector], vec![]);
        let expected = match primitive {
            0 | 264 => Word::signed(42),
            256 => target::reference(f.receiver)?,
            257 => target::reference(f.special[2])?,
            258 => target::reference(f.special[1])?,
            259 => target::reference(f.special[0])?,
            265 => Word::signed(77),
            _ => Word::signed(primitive as i32 - 261),
        };
        result(&f, expected)?;
    }
    Ok(())
}

#[test]
fn blocks_share_home_temporaries_and_return_to_their_caller() -> Result<()> {
    let mut f = Fixture::new();
    f.specials_for_blocks();
    f.start(&[137, 117, 200, 164, 2, 119, 125, 201, 124], vec![], vec![]);
    result(&f, Word::signed(2))?;
    let mut f = Fixture::new();
    f.specials_for_blocks();
    f.start(
        &[137, 118, 200, 164, 3, 104, 16, 125, 32, 202, 124],
        vec![int(42)],
        vec![int(0)],
    );
    result(&f, Word::signed(42))?;
    Ok(())
}

#[test]
fn double_extended_super_send_starts_above_defining_class() -> Result<()> {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let parent = f.pointers(f.class, vec![nil, nil, int(6)]);
    let child = f.pointers(f.class, vec![parent, nil, int(6)]);
    f.image.objects.get_mut(&f.receiver).unwrap().class = child;
    let selector = f.selector("answer");
    let method = f.method(&[32, 124], vec![int(77)], 0, 0, 0);
    f.bind(parent, selector, method);
    let method = f.method(&[32, 124], vec![int(99)], 0, 0, 0);
    f.bind(child, selector, method);
    let association = f.pointers(f.class, vec![nil, child]);
    f.start(&[112, 132, 32, 0, 124], vec![selector, association], vec![]);
    result(&f, Word::signed(77))
}

#[test]
fn perform_validates_and_copies_direct_and_array_arguments() -> Result<()> {
    for array in [false, true] {
        let mut f = Fixture::new();
        let selector = f.primitive(if array { 84 } else { 83 }, 2, &[122]);
        // Bind the target on a subclass so both methods remain reachable.
        let subclass = f.pointers(f.class, vec![f.class, f.special[0], int(6)]);
        f.image.objects.get_mut(&f.receiver).unwrap().class = subclass;
        let target = f.selector("echo:");
        let method = f.method(&[16, 124], vec![], 1, 1, 0);
        f.bind(subclass, target, method);
        let arg = if array {
            f.pointers(f.special[7], vec![int(123456)])
        } else {
            int(123456)
        };
        f.start(
            &[112, 32, 33, 242, 124],
            vec![target, arg, selector],
            vec![],
        );
        result(&f, Word::signed(123456))?;
    }
    Ok(())
}

#[test]
fn does_not_understand_constructs_the_original_two_field_message() -> Result<()> {
    let mut f = Fixture::new();
    let dnu = f.selector("doesNotUnderstand:");
    f.set_special(20, dnu);
    let handler = f.method(&[16, 124], vec![], 1, 1, 0);
    f.bind(f.class, dnu, handler);
    let missing = f.selector("missing:");
    f.start(&[112, 32, 225, 124], vec![int(456), missing], vec![]);
    for jit in [false, true] {
        let m = run(&f, jit)?;
        let message = body(&m, m.objekt.state.vr[5])?;
        assert_eq!(message.len(), 4);
        assert_eq!(message[0].bits(), 32);
        assert_eq!(message[2], target::reference(missing)?);
        let arguments = body(&m, message[3])?;
        assert_eq!(&arguments[2..], &[Word::signed(456)]);
    }
    Ok(())
}
