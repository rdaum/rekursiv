//! Catalog operations must not fetch bodies or apply language class filtering.
use eyre::Result;
use rekursiv_asm::*;
use rekursiv_model::store::Record;
use rekursiv_sim::{runtime, Harness, Timing};
fn r(id: u64) -> Word {
    Word::reference(id, true).unwrap()
}
fn request(pager: Pager, cursor: Word) -> Command {
    Command {
        pager,
        data: cursor,
        vr: 4,
        ..Default::default()
    }
}
fn populate(h: &mut Harness<'_>) -> Result<()> {
    for id in [1, 17, 65, 1 << 32, ID_MASK] {
        let record = Record {
            reference: Word::reference(id, id != 65)?,
            class: r(100),
            cond: false,
            body: vec![Word::ZERO; 600],
        };
        h.store.records.insert(id, record.clone());
        h.oracle.store.records.insert(id, record);
    }
    // A duplicate resident key has newer metadata; key 2 has never been saved.
    h.install(r(17), r(200), 0, &[])?;
    h.install(r(2), r(201), 0, &[])?;
    Ok(())
}
#[test]
fn directory_merges_keys_and_prefers_resident_metadata_without_body_io() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 4,
            memory_latency: 7,
            response_stall: 5,
        },
        None,
    )?;
    populate(&mut h)?;
    let before = (
        h.oracle.state.selected,
        h.oracle.next_identity,
        h.oracle.body_cursor,
        h.stats.reads,
        h.stats.writes,
    );
    let mut cursor = Word::ZERO;
    for (id, class, scan) in [
        (1, 100, true),
        (2, 201, true),
        (17, 200, true),
        (65, 100, false),
        (1 << 32, 100, true),
        (ID_MASK, 100, true),
    ] {
        let response = h.execute(request(Pager::NextObject, cursor))?;
        cursor = Word::reference(id, scan)?;
        assert_eq!(response, Response::ok(cursor));
        assert_eq!(h.oracle.state.vr[4], r(class));
        assert_eq!(
            h.execute(request(Pager::FindObject, Word::raw(id)?))?,
            Response::ok(cursor)
        );
        assert_eq!(h.oracle.state.vr[4], r(class));
    }
    assert_eq!(
        h.execute(request(Pager::NextObject, cursor))?,
        Response::ok(Word::NIL)
    );
    assert_eq!(h.oracle.state.vr[4], Word::NIL);
    assert_eq!(
        h.execute(request(Pager::FindObject, Word::ZERO))?,
        Response::ok(Word::NIL)
    );
    assert_eq!(
        h.execute(request(Pager::FindObject, Word::raw(3)?))?,
        Response::ok(Word::NIL)
    );
    assert_eq!(
        (
            h.oracle.state.selected,
            h.oracle.next_identity,
            h.oracle.body_cursor,
            h.stats.reads,
            h.stats.writes
        ),
        before
    );
    assert!(h
        .store_requests
        .iter()
        .all(|r| matches!(r.op, StoreOp::NextRecord | StoreOp::FindRecord)));
    Ok(())
}
#[test]
fn directory_failures_preserve_state_and_can_retry() -> Result<()> {
    let rt = runtime()?;
    for pager in [Pager::NextObject, Pager::FindObject] {
        let mut h = Harness::new(
            &rt,
            Timing {
                request_delay: 2,
                memory_latency: 3,
                response_stall: 4,
            },
            None,
        )?;
        populate(&mut h)?;
        let cmd = request(
            pager,
            Word::raw(if pager == Pager::FindObject { 65 } else { 17 })?,
        );
        let before = h.oracle.state.clone();
        let failed = h.execute_faults(
            cmd.encode()?,
            Faults {
                store_at: Some(0),
                ..Default::default()
            },
        )?;
        assert_eq!(failed.status, Status::ServiceError);
        assert_eq!(h.oracle.state, before);
        assert_eq!(h.execute(cmd)?.data, Word::reference(65, false)?);
        for value in [Word::NIL, Word::signed(7), Word::raw(1 << 37)?] {
            let before = h.oracle.state.clone();
            assert_eq!(
                h.execute_raw(
                    Ports {
                        data: value.bits(),
                        ..request(pager, Word::ZERO).encode()?
                    },
                    false
                )?
                .status,
                Status::BadValue
            );
            assert_eq!(h.oracle.state, before);
        }
    }
    Ok(())
}

#[test]
fn directory_rejects_malformed_device_metadata_without_publishing_vr() -> Result<()> {
    let rt = runtime()?;
    for (pager, cursor, reference, class) in [
        (Pager::FindObject, 7, r(8), r(100)),
        (Pager::NextObject, 6, r(5), r(100)),
        (Pager::FindObject, 7, Word::signed(7), r(100)),
        (Pager::FindObject, 7, r(7), Word::signed(100)),
        (Pager::FindObject, 7, Word::from_bits(0x8000000000)?, r(100)),
        (Pager::FindObject, 7, r(7), Word::from_bits(0x8000000000)?),
    ] {
        let mut h = Harness::new(
            &rt,
            Timing {
                request_delay: 2,
                memory_latency: 3,
                response_stall: 4,
            },
            None,
        )?;
        let corrupt = Record {
            reference,
            class,
            cond: false,
            body: vec![],
        };
        h.store.records.insert(7, corrupt.clone());
        h.oracle.store.records.insert(7, corrupt);
        h.execute(Command {
            load_vr: true,
            vr: 4,
            data: r(123),
            ..Default::default()
        })?;
        let before = h.oracle.state.clone();
        assert_eq!(
            h.execute(request(pager, Word::raw(cursor)?))?.status,
            Status::ServiceError
        );
        assert_eq!(h.oracle.state, before);
        assert!(h.transfers.is_empty());
    }
    Ok(())
}
