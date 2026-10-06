//! Named Rust-built programs. Host branching is the standalone command driver.
use crate::{microprograms::lookup, Harness};
mod stage5;
use eyre::{ensure, Result};
use rekursiv_asm::*;
pub const NAMES: &[&str] = &[
    "field-access",
    "compact",
    "cons",
    "dictionary",
    "rejected",
    "paged-list",
    "space-recovery",
    "typed-access",
    "paged-dictionary",
    "save-restore",
    "processor",
    "machine-gc",
];
fn reference(id: u64) -> Word {
    Word::reference(id, true).unwrap()
}
fn ok(h: &mut Harness<'_>, c: Command) -> Result<Word> {
    let r = h.execute(c)?;
    ensure!(r.status == Status::Ok, "{c}: {:?}", r.status);
    Ok(r.data)
}
fn get(h: &mut Harness<'_>, index: i64) -> Result<Word> {
    ok(h, Command::index(index)?)?;
    ok(h, Command::read_field())
}
pub fn run(h: &mut Harness<'_>, name: &str) -> Result<String> {
    match name {
        "machine-gc" => crate::processor::collection_example(h),
        "processor" => crate::processor::allocation_example(h),
        "field-access" => {
            h.install(
                reference(1),
                reference(8),
                16,
                &[Word::signed(11), Word::signed(22), Word::signed(33)],
            )?;
            ok(h, Command::probe(reference(1)))?;
            ensure!(get(h, 1)? == Word::signed(11), "first field");
            ensure!(get(h, 3)? == Word::signed(33), "last field");
            ok(h, Command::index(1)?)?;
            ok(h, Command::write_field(Word::signed(99)))?;
            ensure!(get(h, 1)? == Word::signed(99), "cached write");
            ensure!(
                ok(h, Command::read(Read::Flags))?.bits() & 2 != 0,
                "modified flag"
            );
            Ok("first=99, last=33, cached first field coherent, modified=true".into())
        }
        "compact" => {
            for code in 0..4 {
                ensure!(
                    h.service(Service::CompactClass {
                        code,
                        class: reference(8 + code as u64)
                    })?
                    .status
                        == Status::Ok,
                    "class setup"
                );
            }
            for (value, payload) in [
                (Word::NIL, 0),
                (Word::boolean(true), 1),
                (Word::signed(-42), (-42i32) as u32),
                (Word::unsigned(u32::MAX), u32::MAX),
            ] {
                ok(h, Command::probe(value))?;
                ensure!(
                    ok(h, Command::read(Read::Size))?.bits() == 0,
                    "compact size"
                );
                ensure!(
                    ok(h, Command::read(Read::Representation))?.bits() == payload as u64,
                    "compact payload"
                );
            }
            Ok("nil, Boolean, signed -42, unsigned 4294967295 decoded without object-memory traffic".into())
        }
        "cons" => {
            h.install(
                reference(1),
                reference(8),
                16,
                &[reference(2), Word::signed(10)],
            )?;
            h.install(
                reference(2),
                reference(8),
                18,
                &[reference(3), Word::signed(20)],
            )?;
            h.install(
                reference(3),
                reference(8),
                20,
                &[Word::NIL, Word::signed(30)],
            )?;
            let mut next = reference(1);
            let mut values = Vec::new();
            while next != Word::NIL {
                ensure!(values.len() < 3, "list cycle");
                ok(h, Command::probe(next))?;
                values.push(get(h, 2)?.compact_parts()?.1);
                next = get(h, 1)?;
            }
            ensure!(values == [10, 20, 30], "list contents");
            Ok("traversed preinstalled list: [10, 20, 30]".into())
        }
        "dictionary" => {
            let key = Word::unsigned;
            h.install(
                reference(1),
                reference(8),
                32,
                &[
                    key(11),
                    key(111),
                    key(22),
                    key(222),
                    key(33),
                    key(333),
                    key(44),
                    key(444),
                ],
            )?;
            h.install(
                reference(2),
                reference(8),
                48,
                &[
                    key(11),
                    key(111),
                    Word::NIL,
                    Word::NIL,
                    key(33),
                    key(333),
                    key(44),
                    key(444),
                ],
            )?;
            ensure!(
                lookup(h, reference(1), key(11), 5)? == Some(key(111)),
                "wrapped successful lookup"
            );
            ensure!(
                lookup(h, reference(1), key(99), 5)?.is_none(),
                "full-table miss"
            );
            ensure!(
                lookup(h, reference(2), key(99), 1)?.is_none(),
                "empty-slot miss"
            );
            Ok("wrapped lookup returned 111; full-table and nil-slot misses terminated".into())
        }
        "rejected" => {
            h.install(reference(1), reference(8), 16, &[Word::signed(1)])?;
            ok(h, Command::probe(reference(1)))?;
            for idx in [-1, 0, 2, INDEX_MAX] {
                ok(h, Command::index(idx)?)?;
                ensure!(
                    h.execute(Command::write_field(Word::signed(99)))?.status
                        == Status::BoundsError,
                    "invalid write accepted"
                );
            }
            ensure!(
                h.execute(Command::probe(reference(17)))?.status == Status::NotResident,
                "collision miss"
            );
            ensure!(
                get(h, 1)? == Word::signed(1),
                "failed commands changed selection or memory"
            );
            Ok("invalid writes and pager collision rejected; selection and body preserved".into())
        }
        "paged-list" => {
            // Bootstrap one self-typed class. Every list cell uses the RTL allocator.
            h.install(reference(1), reference(1), 0, &[])?;
            let mut head = Word::NIL;
            for value in 1..=24 {
                let cell = ok(h, Command::allocate(reference(1), 2, true)?)?;
                ensure!(
                    get(h, 1)? == Word::NIL && get(h, 2)? == Word::NIL,
                    "allocation initialization"
                );
                ok(h, Command::index(1)?)?;
                ok(h, Command::write_field(head))?;
                ok(h, Command::index(2)?)?;
                ok(h, Command::write_field(Word::signed(value)))?;
                head = cell;
            }
            ok(h, Command::index(2)?)?;
            ok(h, Command::write_field(Word::signed(999)))?;
            ok(h, Command::index(3)?)?;
            ensure!(
                h.execute(Command::write_field(Word::signed(-1)))?.status == Status::BoundsError,
                "invalid field write was accepted"
            );
            ensure!(
                get(h, 2)? == Word::signed(999),
                "rejected write changed the head"
            );
            // A full lap through the pager evicts every cell, including the modified head.
            for _ in 0..16 {
                ok(h, Command::allocate(reference(1), 0, false)?)?;
            }
            ensure!(
                h.execute(Command::probe(head))?.status == Status::NotResident,
                "head was not evicted"
            );
            let mut next = head;
            let mut values = Vec::new();
            while next != Word::NIL {
                ensure!(values.len() < 24, "list cycle");
                ok(h, Command::fetch(next))?;
                values.push(get(h, 2)?.compact_parts()?.1);
                next = get(h, 1)?;
            }
            let expected: Vec<_> = std::iter::once(999).chain((1..24).rev()).collect();
            ensure!(
                values == expected,
                "list did not survive eviction and refill"
            );
            ensure!(
                h.stats.saved_objects >= 24,
                "not all cells reached the backing store"
            );
            Ok(
                "allocated 24 cells; modified head=999; evicted and refetched the complete list"
                    .into(),
            )
        }
        "space-recovery" => {
            h.install(reference(1), reference(1), 0, &[])?;
            let a = ok(h, Command::allocate(reference(1), 2, true)?)?;
            let b = ok(h, Command::allocate(reference(1), 2, true)?)?;
            ok(h, Command::index(1)?)?;
            ok(h, Command::write_field(a))?;
            ok(h, Command::fetch(a))?;
            ok(h, Command::write_field(b))?;
            let live = ok(h, Command::allocate(reference(1), 2, true)?)?;
            ok(h, Command::index(2)?)?;
            ok(h, Command::write_field(Word::signed(123)))?;
            let roots = Roots {
                driver: vec![live],
                ..Default::default()
            };
            for _ in 0..32 {
                ok(h, Command::allocate(reference(1), 8, true)?)?;
            }
            ok(h, Command::fetch(reference(1)))?;
            let compact = h.recover(RecoveryMode::Compact, &roots)?;
            ensure!(
                compact.cursor_after < compact.cursor_before,
                "compaction did not reclaim holes"
            );
            let gc = h.recover(RecoveryMode::Collect, &roots)?;
            ensure!(
                gc.objects_after == 2,
                "collector retained unreachable objects"
            );
            for reference in [a, b] {
                ensure!(
                    h.execute(Command::fetch(reference))?.status == Status::InvalidReference,
                    "dead cycle resurrected"
                );
            }
            for _ in 0..40 {
                ensure!(
                    h.execute_recovering(Command::allocate(reference(1), 40, true)?, &roots)?
                        .status
                        == Status::Ok,
                    "allocation did not recover space"
                );
            }
            ok(h, Command::fetch(live))?;
            ensure!(get(h, 2)? == Word::signed(123), "rooted object changed");
            Ok(format!("compacted {} to {} words; reclaimed {} objects including a cycle; continued allocation; rooted value=123",
                compact.cursor_before,compact.cursor_after,gc.objects_before-gc.objects_after))
        }
        "typed-access" => stage5::typed_access(h),
        "paged-dictionary" => stage5::paged_dictionary(h),
        "save-restore" => stage5::save_restore(h),
        _ => eyre::bail!("unknown example {name}; choices: {}", NAMES.join(", ")),
    }
}
