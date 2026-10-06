use super::{get, ok, reference};
use crate::{microprograms::*, Harness};
use eyre::{ensure, Result};
use rekursiv_asm::*;

fn expect_error<T: std::fmt::Debug>(result: Result<T>, status: Status) -> Result<()> {
    let error = result.expect_err("operation unexpectedly succeeded");
    ensure!(error.downcast_ref::<Status>() == Some(&status), "{error}");
    Ok(())
}

fn set_vr(h: &mut Harness<'_>, vr: u8, value: Word) -> Result<()> {
    ok(
        h,
        Command {
            load_vr: true,
            vr,
            data: value,
            ..Command::default()
        },
    )?;
    Ok(())
}

pub(super) fn typed_access(h: &mut Harness<'_>) -> Result<String> {
    h.install(reference(1), reference(1), 0, &[Word::NIL])?;
    h.install(reference(2), reference(1), 1, &[Word::NIL])?;
    h.install(reference(3), reference(1), 2, &[reference(2)])?;
    h.install(reference(4), reference(1), 3, &[Word::NIL])?;
    h.install(reference(5), reference(1), 4, &[Word::NIL])?;
    let boxed = Word::reference(6, false)?;
    h.install(boxed, reference(2), 5, &[Word::index(2)?])?;
    h.install(
        reference(7),
        reference(3),
        6,
        &[Word::signed(10), Word::signed(20), Word::signed(30)],
    )?;
    h.install(reference(8), reference(4), 9, &[Word::signed(77)])?;
    for (code, class) in [(2, reference(2)), (3, reference(2)), (1, reference(5))] {
        ensure!(
            h.service(Service::CompactClass { code, class })?.status == Status::Ok,
            "compact class setup"
        );
    }
    // Both target and index must work after eviction, including dirty writeback.
    for _ in 0..16 {
        ok(h, Command::allocate(reference(1), 0, false)?)?;
    }
    ensure!(
        h.execute(Command::probe(reference(7)))?.status == Status::NotResident,
        "target was not evicted"
    );
    ensure!(
        typed_read(h, reference(7), Word::signed(1))? == Word::signed(10),
        "signed index"
    );
    ensure!(
        typed_read(h, reference(7), Word::unsigned(3))? == Word::signed(30),
        "unsigned index"
    );
    typed_write(h, reference(7), boxed, Word::signed(222))?;
    ensure!(
        typed_read(h, reference(7), boxed)? == Word::signed(222),
        "stored index"
    );
    expect_error(
        typed_write(h, reference(7), Word::boolean(true), Word::NIL),
        Status::TypeError,
    )?;
    expect_error(
        typed_read(h, reference(8), Word::signed(1)),
        Status::TypeError,
    )?;
    for index in [
        Word::signed(-1),
        Word::signed(0),
        Word::unsigned(4),
        Word::unsigned(u32::MAX),
    ] {
        expect_error(
            typed_write(h, reference(7), index, Word::NIL),
            Status::BoundsError,
        )?;
    }
    ensure!(
        typed_read(h, reference(7), boxed)? == Word::signed(222),
        "rejected writes changed body"
    );
    Ok("compact and stored indices accessed paged objects; typed write=222; wrong types and invalid bounds rejected".into())
}

pub(super) fn paged_dictionary(h: &mut Harness<'_>) -> Result<String> {
    h.install(reference(1), reference(1), 0, &[Word::NIL])?;
    let table = ok(h, Command::allocate(reference(1), 8, true)?)?;
    for (i, value) in [11, 111, 22, 222, 33, 333, 44, 444].into_iter().enumerate() {
        ok(h, Command::index(i as i64 + 1)?)?;
        ok(h, Command::write_field(Word::unsigned(value)))?;
    }
    for _ in 0..16 {
        ok(h, Command::allocate(reference(1), 0, false)?)?;
    }
    ensure!(
        h.execute(Command::probe(table))?.status == Status::NotResident,
        "table was not evicted"
    );
    ensure!(
        lookup(h, table, Word::unsigned(11), 5)? == Some(Word::unsigned(111)),
        "wrapped lookup"
    );
    ensure!(
        lookup(h, table, Word::unsigned(99), 5)?.is_none(),
        "full table miss"
    );
    // Create an empty-key sentinel, then evict the modified table again.
    ok(h, Command::index(3)?)?;
    ok(h, Command::write_field(Word::NIL))?;
    for _ in 0..16 {
        ok(h, Command::allocate(reference(1), 0, false)?)?;
    }
    ensure!(
        lookup(h, table, Word::unsigned(99), 1)?.is_none(),
        "nil slot miss"
    );
    ensure!(
        get(h, 4)? == Word::unsigned(222),
        "neighbor changed during writeback"
    );
    Ok("allocated dictionary survived new and dirty eviction; wrapped hit=111, full-table and nil-slot misses terminated".into())
}

pub(super) fn save_restore(h: &mut Harness<'_>) -> Result<String> {
    h.install(reference(1), reference(1), 0, &[Word::NIL])?;
    h.install(reference(2), reference(1), 1, &[Word::NIL])?;
    h.install(reference(3), reference(1), 2, &[Word::unsigned(123)])?;
    h.install(reference(4), reference(1), 3, &[Word::unsigned(88)])?;
    for (slot, value) in [
        reference(4),
        Word::signed(-1),
        Word::NIL,
        Word::boolean(true),
        Word::unsigned(u32::MAX),
        Word::ZERO,
        reference(3),
        Word::signed(7),
    ]
    .into_iter()
    .enumerate()
    {
        set_vr(h, slot as u8, value)?;
    }
    ok(h, Command::fetch(reference(3)))?;
    // These index bits resemble a pointer to a nonexistent object. The frame
    // must encode them as numeric limbs so collection does not follow them.
    ok(
        h,
        Command::index(Word::from_bits(0xa000000063)?.as_index())?,
    )?;
    ok(
        h,
        Command {
            register: Register::Load,
            data: Word::index(INDEX_MAX)?,
            ..Command::default()
        },
    )?;
    let saved = read_context(h)?;
    let frame = save_context(h, reference(2))?;
    for slot in 0..8 {
        set_vr(h, slot, Word::NIL)?;
    }
    ok(h, Command::index(0)?)?;
    ok(
        h,
        Command {
            register: Register::Load,
            data: Word::ZERO,
            ..Command::default()
        },
    )?;
    for _ in 0..16 {
        ok(h, Command::allocate(reference(1), 0, false)?)?;
    }
    ensure!(
        h.execute(Command::probe(frame))?.status == Status::NotResident,
        "frame was not evicted"
    );
    ok(h, Command::fetch(reference(1)))?;
    let roots = Roots {
        driver: vec![frame],
        ..Roots::default()
    };
    let report = h.recover(RecoveryMode::Collect, &roots)?;
    ensure!(report.objects_after == 5, "saved context graph");
    restore_context(h, frame, reference(2))?;
    ensure!(read_context(h)? == saved, "context did not round trip");
    ensure!(get(h, 1)? == Word::unsigned(123), "selected object changed");
    ok(h, Command::fetch(reference(4)))?;
    ensure!(get(h, 1)? == Word::unsigned(88), "saved VR root was lost");
    Ok("restored selection, all eight VRs, and both 40-bit indices after frame eviction and collection; saved references remained live".into())
}
