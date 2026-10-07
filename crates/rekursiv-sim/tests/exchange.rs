//! Exchange runs entirely in RTL, including body streaming and failure cleanup.
use eyre::Result;
use rekursiv_asm::*;
use rekursiv_model::store::Record;
use rekursiv_sim::{runtime, Harness, Timing};
fn r(id: u64) -> Word {
    Word::reference(id, true).unwrap()
}
fn good(h: &mut Harness<'_>, command: Command) -> Result<Word> {
    let response = h.execute(command)?;
    assert_eq!(response.status, Status::Ok);
    Ok(response.data)
}
fn fixture(
    h: &mut Harness<'_>,
    second: u64,
    resident: [bool; 2],
    lengths: [usize; 2],
) -> Result<[Record; 2]> {
    let records = [
        Record {
            reference: r(1),
            class: r(100),
            cond: false,
            body: vec![Word::signed(11); lengths[0]],
        },
        Record {
            reference: r(second),
            class: r(101),
            cond: true,
            body: vec![Word::signed(22); lengths[1]],
        },
    ];
    for (which, record) in records.iter().enumerate() {
        h.store
            .records
            .insert(record.reference.identity()?, record.clone());
        h.oracle
            .store
            .records
            .insert(record.reference.identity()?, record.clone());
        if resident[which] {
            h.install(
                record.reference,
                record.class,
                which as u32 * 32,
                &record.body,
            )?;
            // Preserve a distinct metadata condition along with dirty data.
            let mut entry = h.oracle.resolve(record.reference)?;
            entry.cond = record.cond;
            h.service(Service::Install(entry))?;
        }
    }
    if resident[0] && lengths[0] > 0 {
        good(h, Command::fetch(r(1)))?;
        good(h, Command::index(1)?)?;
        good(h, Command::write_field(Word::signed(99)))?;
    }
    good(
        h,
        Command {
            data: r(second),
            vr: 2,
            load_vr: true,
            ..Default::default()
        },
    )?;
    Ok(records)
}
fn exchange() -> Command {
    Command {
        pager: Pager::Exchange,
        data: r(1),
        vr: 2,
        ..Default::default()
    }
}

#[test]
fn exchange_handles_dirty_residency_collisions_and_bodies_larger_than_ram() -> Result<()> {
    let rt = runtime()?;
    for (second, resident, lengths) in [
        (2, [true, true], [2, 3]),
        (17, [true, false], [2, 3]),
        (17, [false, true], [2, 3]),
        (17, [false, false], [2, 600]),
        (2, [true, true], [0, 0]),
    ] {
        let mut h = Harness::new(
            &rt,
            Timing {
                request_delay: 3,
                memory_latency: 5,
                response_stall: 4,
            },
            None,
        )?;
        let mut before = fixture(&mut h, second, resident, lengths)?;
        if resident[0] && lengths[0] > 0 {
            before[0].body[0] = Word::signed(99);
        }
        let memory = h.backing.clone();
        let cursor = h.rtl.dbg_body_cursor_o;
        let next = h.rtl.dbg_next_identity_o;
        assert_eq!(good(&mut h, exchange())?, r(1));
        for which in 0..2 {
            let actual = &h.store.records[&before[which].reference.identity()?];
            let source = &before[1 - which];
            assert_eq!(actual.reference, before[which].reference);
            assert_eq!(
                (actual.class, actual.cond, &actual.body),
                (source.class, source.cond, &source.body)
            );
        }
        assert_eq!(h.backing, memory);
        assert_eq!(h.rtl.dbg_body_cursor_o, cursor);
        assert_eq!(h.rtl.dbg_next_identity_o, next);
        assert!(h.oracle.resolve(r(1)).is_err());
        assert!(h.oracle.resolve(r(second)).is_err());
        if lengths[1] < 100 {
            good(&mut h, Command::fetch(r(1)))?;
            assert_eq!(good(&mut h, Command::read(Read::Type))?, before[1].class);
        }
    }
    Ok(())
}

#[test]
fn every_exchange_io_failure_leaves_both_bindings_unchanged_and_can_retry() -> Result<()> {
    let rt = runtime()?;
    for resident in [[true, true], [false, false], [true, false]] {
        let mut baseline = Harness::new(&rt, Timing::default(), None)?;
        fixture(&mut baseline, 2, resident, [2, 3])?;
        let stores = baseline.store_requests.len();
        let reads = baseline.transfers.len();
        good(&mut baseline, exchange())?;
        let stores = baseline.store_requests.len() - stores;
        let reads = baseline.transfers.len() - reads;
        for fault in (0..stores)
            .map(|n| Faults {
                store_at: Some(n),
                memory_at: None,
            })
            .chain((0..reads).map(|n| Faults {
                store_at: None,
                memory_at: Some(n),
            }))
        {
            let mut h = Harness::new(
                &rt,
                Timing {
                    request_delay: 2,
                    memory_latency: 3,
                    response_stall: 5,
                },
                None,
            )?;
            fixture(&mut h, 2, resident, [2, 3])?;
            let disk = h.store.records.clone();
            let memory = h.backing.clone();
            let entries = h.oracle.entries.clone();
            let state = h.oracle.state.clone();
            let result = h.execute_faults(exchange().encode()?, fault)?;
            assert_eq!(
                result.status,
                if fault.memory_at.is_some() {
                    Status::MemoryError
                } else {
                    Status::ServiceError
                }
            );
            assert_eq!(h.store.records, disk);
            assert_eq!(h.backing, memory);
            assert_eq!(h.oracle.entries, entries);
            assert_eq!(h.oracle.state, state);
            assert_eq!(h.store.staged_record_count(), None);
            good(&mut h, exchange())?;
            assert_eq!(h.store.records[&1].class, r(101));
        }
    }
    Ok(())
}

#[test]
fn exchange_same_key_is_noop_and_invalid_operands_do_not_open_batches() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    fixture(&mut h, 2, [true, true], [2, 3])?;
    good(
        &mut h,
        Command {
            data: r(1),
            vr: 2,
            load_vr: true,
            ..Default::default()
        },
    )?;
    let before = h.oracle.clone();
    let transactions = h.store_requests.len();
    good(&mut h, exchange())?;
    assert_eq!(h.oracle, before);
    assert_eq!(transactions, h.store_requests.len());
    for (other, status) in [
        (Word::signed(1), Status::InvalidReference),
        (Word::reference(2, false)?, Status::BadValue),
        (r(99), Status::InvalidReference),
    ] {
        good(
            &mut h,
            Command {
                data: other,
                vr: 2,
                load_vr: true,
                ..Default::default()
            },
        )?;
        assert_eq!(h.execute(exchange())?.status, status);
        assert_eq!(h.store.staged_record_count(), None);
    }
    Ok(())
}
