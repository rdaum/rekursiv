//! Integer, object and scheduler primitives.
use super::*;

#[test]
fn integer_multiplication_detects_high_word_overflow_and_falls_back() -> Result<()> {
    for (a, b, expected) in [
        (100_000, 1000, 100_000_000),
        (1 << 29, 8, 77),
        (-(1 << 30), -1, 77),
    ] {
        let mut f = Fixture::new();
        let selector = f.primitive(9, 1, &[32, 124]);
        // The fallback returns 77; a wrapped product must never be reported as success.
        let dict = f.image.objects[&f.class].pointer(1)?;
        let methods = f.image.objects[&dict].pointer(1)?;
        let method = f.image.objects[&methods].pointer(5)?;
        if let Body::Method {
            literals,
            header,
            bytes,
        } = &mut f.image.objects.get_mut(&method).unwrap().body
        {
            *literals = vec![int(77)];
            header.0 |= 1 << 9;
            *bytes = vec![32, 124];
        }
        let dictionary = f.image.objects[&f.class].pointer(1)?;
        if let Body::Pointers(fields) = &mut f.image.objects.get_mut(&f.special[5]).unwrap().body {
            fields[1] = dictionary;
        }
        f.start(&[32, 33, 226, 124], vec![int(a), int(b), selector], vec![]);
        result(&f, Word::signed(expected))?;
    }
    Ok(())
}

#[test]
fn indexed_access_preserves_unsigned_word_bits_and_failed_writes() -> Result<()> {
    for value in [0, 255, 1 << 29, 1 << 30, u32::MAX] {
        let mut f = Fixture::new();
        let class = f.pointers(f.class, vec![f.class, f.special[0], int((6 << 7) | 2)]);
        let words = f.object(class, Body::Words(vec![value]), 6);
        let selector = f.primitive(60, 1, &[120]);
        f.start(&[32, 118, 225, 124], vec![words, selector], vec![]);
        for jit in [false, true] {
            let m = run(&f, jit)?;
            let result = m.objekt.state.vr[5];
            if value < 1 << 30 {
                assert_eq!(result, Word::signed(value as i32));
            } else {
                let bytes = body(&m, result)?;
                assert_eq!(bytes[0].bits(), 18);
                assert_eq!(
                    bytes[2..]
                        .iter()
                        .map(|w| w.bits() as u8)
                        .collect::<Vec<_>>(),
                    value.to_le_bytes()
                );
            }
        }
    }
    // A byte store above 255 must fail before mutation, returning fallback false.
    let mut f = Fixture::new();
    let class = f.pointers(f.class, vec![f.class, f.special[0], int((8 << 7) | 2)]);
    let bytes = f.object(class, Body::Bytes(vec![5]), 8);
    let selector = f.primitive(61, 2, &[122]);
    f.start(
        &[32, 118, 33, 242, 124],
        vec![bytes, int(256), selector],
        vec![],
    );
    let m = run(&f, false)?;
    assert_eq!(m.objekt.state.vr[5], target::reference(f.special[1])?);
    assert_eq!(body(&m, target::reference(bytes)?)?[2].bits(), 5);
    Ok(())
}

#[test]
fn allocation_initializes_pointer_and_word_objects_and_advances_hash_state() -> Result<()> {
    for (format, fixed, count) in [(1, 2, 0), (2, 0, 5), (6, 2, 0), (8, 0, 7)] {
        let mut f = Fixture::new();
        let class = f.pointers(
            f.class,
            vec![
                f.special[0],
                f.special[0],
                int((format << 7) | ((fixed + 1) << 1)),
            ],
        );
        let selector = f.primitive(71, 1, &[122]);
        f.start(
            &[32, 33, 226, 124],
            vec![class, int(count), selector],
            vec![],
        );
        for jit in [false, true] {
            let m = run(&f, jit)?;
            let reference = m.objekt.state.vr[5];
            let words = body(&m, reference)?;
            assert_eq!(words.len(), (fixed + count + 2) as usize);
            let hash = (13849 + 27181 * 999u32) & 65535;
            assert_eq!(words[1].bits() & 4095, u64::from(hash & 4095));
            assert_eq!(m.cpu.roots.unwrap()[2].bits(), u64::from(hash));
            let expected = if format < 4 {
                target::reference(f.special[0])?
            } else {
                Word::ZERO
            };
            assert!(words[2..].iter().all(|w| *w == expected));
            let kind = if format < 4 {
                0
            } else if format == 6 {
                1
            } else {
                2
            };
            let bytes = if kind == 2 {
                fixed + count
            } else {
                4 * (fixed + count)
            };
            assert_eq!(words[0].bits(), ((bytes << 2) | kind) as u64);
        }
    }
    Ok(())
}

#[test]
fn signed_division_shifts_and_boundary_arithmetic() -> Result<()> {
    for (prim, a, b, expected) in [
        (10, -1_000_000, 10, -100_000),
        (11, -7, 3, 2),
        (11, 7, -3, -2),
        (12, -7, 3, -3),
        (13, -7, 3, -2),
        (17, -(1 << 30), -30, -1),
        (17, 1, 29, 1 << 29),
        (17, 1 << 29, 4, 2),
        (1, (1 << 30) - 1, 1, 2),
    ] {
        let mut f = Fixture::new();
        let selector = f.primitive(prim, 1, &[119, 124]);
        let dict = f.image.objects[&f.class].pointer(1)?;
        if let Body::Pointers(fields) = &mut f.image.objects.get_mut(&f.special[5]).unwrap().body {
            fields[1] = dict;
        }
        f.start(&[32, 33, 226, 124], vec![int(a), int(b), selector], vec![]);
        result(&f, Word::signed(expected))?;
    }
    Ok(())
}

#[test]
fn semaphore_signal_and_wait_use_guest_state() -> Result<()> {
    for (primitive, signals, expected) in [(85, 0, 1), (86, 3, 2)] {
        let mut f = Fixture::new();
        let selector = f.primitive(primitive, 0, &[122]);
        if let Body::Pointers(fields) = &mut f.image.objects.get_mut(&f.special[18]).unwrap().body {
            fields[0] = f.class;
        }
        let semaphore = f.pointers(
            f.special[18],
            vec![f.special[0], f.special[0], int(signals)],
        );
        f.start(&[32, 209, 124], vec![semaphore, selector], vec![]);
        for jit in [false, true] {
            let m = run(&f, jit)?;
            assert_eq!(m.objekt.state.vr[5], target::reference(semaphore)?);
            assert_eq!(
                body(&m, target::reference(semaphore)?)?[4],
                Word::signed(expected)
            );
        }
    }
    Ok(())
}

#[test]
fn compiled_method_allocation_preserves_header_literals_and_byte_extent() -> Result<()> {
    let mut f = Fixture::new();
    let selector = f.primitive(79, 2, &[122]);
    let header = (1 << 24) | (2 << 18) | (3 << 9) | 257;
    f.start(
        &[32, 33, 34, 243, 124],
        vec![f.special[16], int(7), int(header), selector],
        vec![],
    );
    for jit in [false, true] {
        let m = run(&f, jit)?;
        let words = body(&m, m.objekt.state.vr[5])?;
        assert_eq!(words.len(), 13);
        assert_eq!(words[0].bits(), ((16 + 7) << 2) | 3);
        assert_eq!(words[1].bits() >> 12, 13);
        assert_eq!(words[2], Word::signed(header));
        assert!(words[3..6]
            .iter()
            .all(|w| *w == target::reference(f.special[0]).unwrap()));
        assert!(words[6..].iter().all(|w| *w == Word::ZERO));
    }
    Ok(())
}

#[test]
fn higher_priority_resume_saves_old_context_and_switches_at_boundary() -> Result<()> {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let selector = f.primitive(87, 0, &[122]);
    let method = f.method(&[119, 124], vec![], 0, 0, 0);
    let mut fields = vec![nil; 38];
    fields[..6].copy_from_slice(&[nil, int(5), int(0), method, nil, f.receiver]);
    let next_context = f.pointers(f.special[10], fields);
    let next_process = f.pointers(f.class, vec![nil, next_context, int(4), nil]);
    let scheduler = f.image.objects[&f.special[3]].pointer(1)?;
    let old_process = f.image.objects[&scheduler].pointer(1)?;
    let queues = f.image.objects[&scheduler].pointer(0)?;
    let queue = f.pointers(f.class, vec![nil, nil]);
    if let Body::Pointers(fields) = &mut f.image.objects.get_mut(&queues).unwrap().body {
        fields[2] = queue;
    }
    f.start(&[32, 209, 124], vec![next_process, selector], vec![]);
    for jit in [false, true] {
        let mut loaded = boot::squeak_source(&f.image, 131072, 16)?;
        let m = &mut loaded.machine;
        if jit {
            m.enable_jit()?;
        }
        let mut switched = false;
        for _ in 0..100_000 {
            ensure!(
                m.step()? == Step::Retired,
                "processor stopped before switching"
            );
            if m.cpu.pc == loaded.symbols["cycle"] as u16
                && m.objekt.state.vr[0] == target::reference(next_context)?
            {
                switched = true;
                break;
            }
        }
        assert!(switched);
        assert_eq!(
            body(m, target::reference(scheduler)?)?[3],
            target::reference(next_process)?
        );
        assert_eq!(
            body(m, target::reference(next_process)?)?[3],
            target::reference(nil)?
        );
        let old = body(m, target::reference(old_process)?)?;
        assert_eq!(old[3], target::reference(f.context)?);
        assert_eq!(old[5], target::reference(queue)?);
        assert_eq!(
            &body(m, target::reference(queue)?)?[2..4],
            &[target::reference(old_process)?; 2]
        );
    }
    Ok(())
}

#[test]
fn low_space_reserve_rejects_allocation_and_signals_after_guest_fallback() -> Result<()> {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let class = f.pointers(f.class, vec![nil, nil, int((2 << 7) | 2)]);
    let selector = f.primitive(71, 1, &[122]);
    let semaphore = f.pointers(f.special[18], vec![nil, nil, int(0)]);
    f.start(&[32, 118, 225, 124], vec![class, selector], vec![]);
    for jit in [false, true] {
        let m = run_with(&f, jit, |m| {
            let roots = m.cpu.roots.as_mut().unwrap();
            roots[11] = Word::raw(1 << 28).unwrap();
            roots[30] = target::reference(semaphore).unwrap();
            // Let allocation, rather than the periodic check, discover pressure.
            roots[14] = Word::raw(1023).unwrap();
        })?;
        assert_eq!(m.objekt.state.vr[5], target::reference(f.special[1])?);
        assert_eq!(body(&m, target::reference(semaphore)?)?[4], Word::signed(1));
        assert_eq!(m.cpu.roots.as_ref().unwrap()[11], Word::ZERO);
        assert_eq!(m.cpu.roots.as_ref().unwrap()[4], Word::ZERO);
        assert!(m.stats.collections > 0);
    }
    Ok(())
}

#[test]
fn archived_reserved_primitives_execute_the_guest_fallback() -> Result<()> {
    for primitive in [
        19, 55, 59, 120, 123, 126, 127, 148, 149, 163, 168, 171, 179, 200, 249, 254, 255,
    ] {
        let mut f = Fixture::new();
        let selector = f.primitive(primitive, 0, &[119, 124]);
        f.start(&[112, 208, 124], vec![selector], vec![]);
        assert_eq!(
            run(&f, false)?.objekt.state.vr[5],
            Word::signed(2),
            "primitive={primitive}"
        );
    }
    Ok(())
}
