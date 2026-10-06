use eyre::Result;
use proptest::prelude::*;
use rekursiv_asm::*;
use rekursiv_sim::{microprograms::*, runtime, Harness, Timing};

fn r(id: u64) -> Word {
    Word::reference(id, true).unwrap()
}
fn timing() -> Timing {
    Timing {
        request_delay: 2,
        memory_latency: 4,
        response_stall: 3,
    }
}
fn error<T: std::fmt::Debug>(result: Result<T>, expected: Status) {
    assert_eq!(
        result.unwrap_err().downcast_ref::<Status>(),
        Some(&expected)
    );
}
fn write(h: &mut Harness<'_>, index: i64, value: Word) -> Result<()> {
    execute(h, Command::index(index)?)?;
    execute(h, Command::write_field(value))?;
    Ok(())
}
fn vr(h: &mut Harness<'_>, slot: u8, value: Word) -> Result<()> {
    execute(
        h,
        Command {
            vr: slot,
            load_vr: true,
            data: value,
            ..Command::default()
        },
    )?;
    Ok(())
}

#[test]
fn typed_access_survives_target_class_and_index_pager_collisions() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, timing(), None)?;
    h.install(r(1), r(1), 0, &[Word::NIL])?;
    h.install(r(2), r(1), 1, &[Word::NIL])?;
    h.install(r(3), r(19), 2, &[Word::signed(10), Word::signed(20)])?;
    assert_eq!(h.service(Service::Invalidate(r(3)))?.status, Status::Ok);
    h.install(r(19), r(1), 4, &[r(2)])?;
    assert_eq!(h.service(Service::Invalidate(r(19)))?.status, Status::Ok);
    let index = Word::reference(35, false)?;
    h.install(index, r(2), 5, &[Word::index(2)?])?;
    assert_eq!(typed_read(&mut h, r(3), index)?, Word::signed(20));
    typed_write(&mut h, r(3), index, Word::signed(99))?;
    assert_eq!(typed_read(&mut h, r(3), index)?, Word::signed(99));
    assert!(h.stats.saved_objects > 0);
    // A read through an incompatible compact class must fail in the RTL check.
    assert_eq!(
        h.service(Service::CompactClass {
            code: 1,
            class: r(1)
        })?
        .status,
        Status::Ok
    );
    error(
        typed_write(&mut h, r(3), Word::boolean(true), Word::NIL),
        Status::TypeError,
    );
    assert_eq!(h.commands.last().unwrap().1.status, Status::TypeError);
    assert_eq!(typed_read(&mut h, r(3), index)?, Word::signed(99));
    Ok(())
}

#[test]
fn typed_indices_reject_wrong_shapes_and_preserve_data_on_bounds_errors() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, timing(), None)?;
    h.install(r(1), r(1), 0, &[Word::NIL])?;
    h.install(r(2), r(1), 1, &[Word::NIL])?;
    h.install(r(3), r(1), 2, &[r(2)])?;
    h.install(r(4), r(3), 3, &[Word::unsigned(111), Word::unsigned(222)])?;
    h.install(r(5), r(2), 5, &[Word::index(1)?])?; // Scanned indices are not numeric boxes.
    let empty = Word::reference(6, false)?;
    h.install(empty, r(2), 6, &[])?;
    let long = Word::reference(7, false)?;
    h.install(long, r(2), 7, &[Word::index(1)?, Word::ZERO])?;
    let negative = Word::reference(8, false)?;
    h.install(negative, r(2), 9, &[Word::index(INDEX_MIN)?])?;
    for code in [1, 2, 3] {
        assert_eq!(
            h.service(Service::CompactClass { code, class: r(2) })?
                .status,
            Status::Ok
        );
    }
    for index in [r(5), empty, long, Word::boolean(true)] {
        error(typed_read(&mut h, r(4), index), Status::BadValue);
    }
    for index in [
        negative,
        Word::signed(i32::MIN),
        Word::signed(-1),
        Word::signed(0),
        Word::unsigned(3),
        Word::unsigned(u32::MAX),
    ] {
        let writes = h.stats.writes;
        error(
            typed_write(&mut h, r(4), index, Word::NIL),
            Status::BoundsError,
        );
        assert_eq!(h.stats.writes, writes);
    }
    assert_eq!(
        typed_read(&mut h, r(4), Word::signed(1))?,
        Word::unsigned(111)
    );
    assert_eq!(
        typed_read(&mut h, r(4), Word::unsigned(2))?,
        Word::unsigned(222)
    );
    // Nil access types prohibit indexing. Other non-reference access types are malformed.
    execute(&mut h, Command::fetch(r(3)))?;
    write(&mut h, 1, Word::NIL)?;
    error(typed_read(&mut h, r(4), Word::signed(1)), Status::TypeError);
    execute(&mut h, Command::fetch(r(3)))?;
    write(&mut h, 1, Word::unsigned(2))?;
    error(typed_read(&mut h, r(4), Word::signed(1)), Status::BadValue);
    Ok(())
}

#[test]
fn dictionary_all_starts_nil_values_and_invalid_layouts() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, timing(), None)?;
    h.install(r(1), r(1), 0, &[Word::NIL])?;
    h.install(
        r(2),
        r(1),
        1,
        &[
            Word::unsigned(1),
            Word::NIL,
            Word::unsigned(2),
            Word::unsigned(20),
            Word::unsigned(3),
            Word::unsigned(30),
            Word::unsigned(4),
            Word::unsigned(40),
        ],
    )?;
    for start in [1, 3, 5, 7] {
        for key in 1..=4 {
            let expected = if key == 1 {
                Word::NIL
            } else {
                Word::unsigned(key * 10)
            };
            assert_eq!(
                lookup(&mut h, r(2), Word::unsigned(key), start)?,
                Some(expected)
            );
        }
        assert_eq!(lookup(&mut h, r(2), Word::unsigned(99), start)?, None);
    }
    for start in [INDEX_MIN, -1, 0, 2, 8, 9, INDEX_MAX] {
        error(
            lookup(&mut h, r(2), Word::unsigned(1), start),
            Status::BadValue,
        );
    }
    error(lookup(&mut h, r(2), Word::NIL, 1), Status::BadValue);
    for (i, size) in [0, 1, 3, 6].into_iter().enumerate() {
        let object = r(3 + i as u64);
        h.install(object, r(1), 32 + i as u32 * 8, &vec![Word::NIL; size])?;
        error(
            lookup(&mut h, object, Word::unsigned(1), 1),
            Status::BadValue,
        );
    }
    let table = execute(&mut h, Command::allocate(r(1), 2, true)?)?;
    assert_eq!(lookup(&mut h, table, Word::unsigned(1), 1)?, None);
    write(&mut h, 1, Word::unsigned(1))?;
    write(&mut h, 2, Word::unsigned(10))?;
    assert_eq!(
        lookup(&mut h, table, Word::unsigned(1), 1)?,
        Some(Word::unsigned(10))
    );
    assert_eq!(lookup(&mut h, table, Word::unsigned(2), 1)?, None);
    Ok(())
}

#[test]
fn malformed_frames_do_not_restore_value_registers() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, timing(), None)?;
    error(save_context(&mut h, r(1)), Status::NoSelection);
    assert_eq!(h.oracle.next_identity, 1);
    h.install(r(1), r(1), 0, &[Word::NIL])?;
    h.install(r(2), r(1), 1, &[Word::NIL])?;
    execute(&mut h, Command::fetch(r(1)))?;
    vr(&mut h, 0, Word::unsigned(100))?;
    let frame = save_context(&mut h, r(2))?;
    vr(&mut h, 0, Word::unsigned(200))?;
    let expected_vr = h.oracle.state.vr;
    error(restore_context(&mut h, frame, r(1)), Status::TypeError);
    for (field, value) in [
        (10, Word::signed(0)),
        (11, Word::unsigned(256)),
        (13, Word::NIL),
    ] {
        execute(&mut h, Command::fetch(frame))?;
        write(&mut h, field, value)?;
        error(restore_context(&mut h, frame, r(2)), Status::BadValue);
        assert_eq!(h.oracle.state.vr, expected_vr);
        execute(&mut h, Command::fetch(frame))?;
        write(&mut h, field, Word::unsigned(0))?;
    }
    execute(&mut h, Command::fetch(frame))?;
    write(&mut h, 1, r(99))?;
    error(
        restore_context(&mut h, frame, r(2)),
        Status::InvalidReference,
    );
    assert_eq!(h.oracle.state.vr, expected_vr);
    let short = execute(&mut h, Command::allocate(r(2), 12, true)?)?;
    error(restore_context(&mut h, short, r(2)), Status::BadValue);
    let opaque = execute(&mut h, Command::allocate(r(2), 13, false)?)?;
    error(restore_context(&mut h, opaque, r(2)), Status::BadValue);
    assert_eq!(h.oracle.state.vr, expected_vr);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]
    #[test]
    fn contexts_round_trip_all_index_bits(
        index in 0u64..=WORD_MASK,
        index_reg in 0u64..=WORD_MASK,
        values in prop::array::uniform8(any::<i32>()),
        compact_selection in any::<bool>(),
        delay in 0u32..4,
    ) {
        let runtime = runtime().unwrap();
        let mut h = Harness::new(&runtime, Timing { request_delay: delay, memory_latency: delay, response_stall: delay }, None).unwrap();
        h.install(r(1), r(1), 0, &[Word::NIL]).unwrap();
        h.service(Service::CompactClass { code: 2, class: r(1) }).unwrap();
        execute(&mut h, if compact_selection { Command::probe(Word::signed(values[0])) } else { Command::fetch(r(1)) }).unwrap();
        for (slot, value) in values.into_iter().enumerate() { vr(&mut h, slot as u8, Word::signed(value)).unwrap(); }
        execute(&mut h, Command::index(Word::from_bits(index).unwrap().as_index()).unwrap()).unwrap();
        execute(&mut h, Command { register: Register::Load, data: Word::from_bits(index_reg).unwrap(), ..Command::default() }).unwrap();
        let expected = read_context(&mut h).unwrap();
        let frame = save_context(&mut h, r(1)).unwrap();
        for slot in 0..8 { vr(&mut h, slot, Word::NIL).unwrap(); }
        execute(&mut h, Command::index(0).unwrap()).unwrap();
        execute(&mut h, Command { register: Register::Load, data: Word::ZERO, ..Command::default() }).unwrap();
        h.recover(RecoveryMode::Collect, &Roots { driver: vec![frame], ..Roots::default() }).unwrap();
        restore_context(&mut h, frame, r(1)).unwrap();
        prop_assert_eq!(read_context(&mut h).unwrap(), expected);
    }
}
