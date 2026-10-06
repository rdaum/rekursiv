use eyre::Result;
use proptest::prelude::*;
use rekursiv_asm::*;
use rekursiv_sim::{runtime, Harness, Timing};
use std::collections::BTreeSet;
fn r(id: u64) -> Word {
    Word::reference(id, true).unwrap()
}
fn good(h: &mut Harness<'_>, c: Command) -> Word {
    let out = h.execute(c).unwrap();
    assert_eq!(out.status, Status::Ok, "{c}");
    out.data
}
fn bootstrap(h: &mut Harness<'_>) -> Result<()> {
    h.install(r(1), r(1), 0, &[])
}
fn get(h: &mut Harness<'_>, reference: Word, index: i64) -> Word {
    good(h, Command::fetch(reference));
    good(h, Command::index(index).unwrap());
    good(h, Command::read_field())
}
fn sparse(h: &mut Harness<'_>) -> Result<()> {
    bootstrap(h)?;
    h.install(r(2), r(1), 30, &[Word::signed(11), Word::signed(22)])?;
    h.install(r(3), r(1), 80, &[Word::signed(33)])?;
    h.install(r(4), r(1), 100, &[])?;
    good(h, Command::probe(r(2)));
    good(h, Command::index(2)?);
    Ok(())
}
#[test]
fn compaction_preserves_identity_flags_registers_and_cached_fields() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(
        &runtime,
        Timing {
            request_delay: 2,
            memory_latency: 3,
            response_stall: 2,
        },
        None,
    )?;
    sparse(&mut h)?;
    let before = h.oracle.state.clone();
    let identity = h.oracle.next_identity;
    let records = h.store.records.clone();
    let report = h.recover(RecoveryMode::Compact, &Roots::default())?;
    assert_eq!((report.cursor_before, report.cursor_after), (100, 3));
    assert_eq!(report.objects_before, report.objects_after);
    assert_eq!(h.store.records, records);
    assert_eq!(h.oracle.next_identity, identity);
    assert_eq!(h.oracle.state.vr, before.vr);
    assert_eq!(h.oracle.state.index, before.index);
    assert_eq!(h.oracle.state.selected.unwrap().base, 0);
    assert_eq!(good(&mut h, Command::read_field()), Word::signed(22));
    good(&mut h, Command::allocate(r(1), 509, false)?);
    assert_eq!(get(&mut h, r(2), 1), Word::signed(11));
    assert_eq!(get(&mut h, r(3), 1), Word::signed(33));
    Ok(())
}
#[test]
fn complete_graph_roots_opaque_bodies_dirty_overrides_and_dead_cycles() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    bootstrap(&mut h)?;
    let opaque = Word::reference(4, false)?;
    h.install(r(2), r(1), 20, &[r(3), opaque])?;
    h.install(r(3), r(5), 30, &[r(13)])?;
    h.install(opaque, r(1), 40, &[r(6)])?;
    h.install(r(5), r(1), 50, &[r(7)])?;
    for id in 6..=12 {
        h.install(r(id), r(1), id as u32 * 10, &[Word::NIL])?;
    }
    h.install(r(13), r(1), 130, &[r(14)])?;
    h.install(r(14), r(1), 140, &[r(13)])?;
    h.install(r(15), r(1), 150, &[Word::raw(6)?])?;
    h.install(r(16), r(1), 160, &[Word::NIL])?;
    h.service(Service::Invalidate(r(13)))?;
    h.service(Service::Invalidate(r(14)))?;
    good(&mut h, Command::probe(r(3)));
    good(&mut h, Command::index(1)?);
    good(&mut h, Command::write_field(r(2)));
    // r2 becomes nonresident; tracing it must stream its backing record without paging it in.
    h.install(r(18), r(1), 180, &[Word::NIL])?;
    good(
        &mut h,
        Command {
            load_vr: true,
            vr: 2,
            data: r(8),
            ..Default::default()
        },
    );
    h.service(Service::CompactClass {
        code: 0,
        class: r(9),
    })?;
    good(&mut h, Command::probe(r(1)));
    let roots = Roots {
        driver: vec![r(2), r(15)],
        service: vec![r(11)],
        language: vec![r(10)],
        pending: vec![
            Command::fetch(r(12)),
            Command {
                read: Read::Type,
                expected_type: Some(r(16)),
                ..Default::default()
            },
        ],
    };
    let report = h.recover(RecoveryMode::Collect, &roots)?;
    let expected: BTreeSet<_> = [1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12, 15, 16]
        .into_iter()
        .collect();
    assert_eq!(
        h.store.records.keys().copied().collect::<BTreeSet<_>>(),
        expected
    );
    assert_eq!(report.objects_after, expected.len());
    assert!(report.cursor_after < report.cursor_before);
    assert!(
        h.oracle.entries[2].is_none(),
        "collector paged in a nonresident object"
    );
    assert!(h.oracle.entries[3].unwrap().modified);
    assert_eq!(get(&mut h, r(2), 1), r(3));
    assert_eq!(get(&mut h, r(3), 1), r(2));
    assert_eq!(get(&mut h, opaque, 1), r(6)); // Opaque words survive without keeping their referents.
    for id in [6, 13, 14, 18] {
        assert_eq!(
            h.execute(Command::fetch(r(id)))?.status,
            Status::InvalidReference
        );
    }
    Ok(())
}
#[test]
fn every_recovery_memory_failure_is_safe_and_resumable() -> Result<()> {
    let runtime = runtime()?;
    // Three snapshot reads followed by three packed writes.
    for fail_at in 0..6 {
        let mut h = Harness::new(
            &runtime,
            Timing {
                request_delay: 2,
                memory_latency: 3,
                response_stall: 4,
            },
            None,
        )?;
        sparse(&mut h)?;
        let before = h.oracle.clone();
        assert!(h
            .recover_with_fault(RecoveryMode::Compact, &Roots::default(), Some(fail_at))
            .is_err());
        if fail_at < 3 {
            assert!(!h.recovery_pending());
            assert_eq!(h.rtl.dbg_maintenance_o, 0);
            assert_eq!(h.oracle, before);
            h.recover(RecoveryMode::Compact, &Roots::default())?;
        } else {
            assert!(h.recovery_pending());
            assert_eq!(h.rtl.dbg_maintenance_o, 1);
            assert!(h.execute(Command::index(99)?).is_err());
            h.drive(Command::index(99)?.encode()?)?;
            h.rtl.run_i = 1;
            h.rtl.cmd_valid_i = 1;
            for _ in 0..8 {
                assert!(!h.tick()?.command);
                assert_eq!(h.rtl.cmd_ready_o, 0);
            }
            h.rtl.cmd_valid_i = 0;
            let report = h.resume_recovery()?;
            assert_eq!(report.cursor_after, 3);
        }
        assert_eq!(get(&mut h, r(2), 1), Word::signed(11));
        assert_eq!(get(&mut h, r(3), 1), Word::signed(33));
        assert_eq!(h.rtl.dbg_maintenance_o, 0);
        assert!(!h.recovery_pending());
    }
    Ok(())
}
#[test]
fn maintenance_gate_cursor_validation_and_service_reads() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    sparse(&mut h)?;
    assert_eq!(
        h.service(Service::SetBodyCursor(0))?.status,
        Status::BadCommand
    );
    assert_eq!(h.service(Service::EndRecovery)?.status, Status::BadCommand);
    assert_eq!(
        h.service(Service::ReadMemory { address: 30 })?.data,
        Word::signed(11)
    );
    assert_eq!(
        h.service_with_error(Service::ReadMemory { address: 30 }, true)?
            .status,
        Status::MemoryError
    );
    assert_eq!(
        h.service(Service::ReadMemory { address: 512 })?.status,
        Status::BoundsError
    );
    h.service(Service::BeginRecovery)?;
    assert_eq!(
        h.service(Service::BeginRecovery)?.status,
        Status::BadCommand
    );
    for cursor in [0, 80, 99, 513, u32::MAX] {
        assert_eq!(
            h.service(Service::SetBodyCursor(cursor))?.status,
            Status::BoundsError
        );
    }
    assert_eq!(h.oracle.body_cursor, 100);
    h.service(Service::EndRecovery)?;
    assert_eq!(good(&mut h, Command::read_field()), Word::signed(22));
    Ok(())
}
#[test]
fn allocation_pressure_collects_garbage_without_reusing_identities() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    bootstrap(&mut h)?;
    let mut previous = 1;
    for _ in 0..70 {
        let out = h.execute_recovering(Command::allocate(r(1), 40, true)?, &Roots::default())?;
        assert_eq!(out.status, Status::Ok);
        let id = out.data.identity()?;
        assert!(id > previous);
        previous = id;
        assert_eq!(get(&mut h, out.data, 40), Word::NIL);
    }
    assert!(h.stats.compactions > 0 && h.stats.collections > 0);
    assert_eq!(
        h.execute(Command::fetch(r(2)))?.status,
        Status::InvalidReference
    );
    // A genuinely full live set must still return OutOfSpace.
    h.reset()?;
    bootstrap(&mut h)?;
    good(&mut h, Command::allocate(r(1), 512, false)?);
    let next = h.oracle.next_identity;
    assert_eq!(
        h.execute_recovering(Command::allocate(r(1), 1, true)?, &Roots::default())?
            .status,
        Status::OutOfSpace
    );
    assert_eq!(h.oracle.next_identity, next);
    Ok(())
}
#[test]
fn recovery_keeps_pending_fetch_target_and_allocation_class() -> Result<()> {
    let runtime = runtime()?;
    for fetch in [false, true] {
        let mut h = Harness::new(&runtime, Timing::default(), None)?;
        bootstrap(&mut h)?;
        let body = if fetch {
            vec![Word::signed(77); 40]
        } else {
            vec![Word::NIL]
        };
        h.install(r(2), r(1), 0, &body)?;
        h.service(Service::Invalidate(r(2)))?;
        for id in 3..=14 {
            h.install(r(id), r(1), (id as u32 - 3) * 40, &vec![Word::NIL; 40])?;
        }
        good(&mut h, Command::probe(r(1)));
        let command = if fetch {
            Command::fetch(r(2))
        } else {
            Command::allocate(r(2), 40, true)?
        };
        let out = h.execute_recovering(command, &Roots::default())?;
        assert_eq!(out.status, Status::Ok);
        assert!(h.stats.collections > 0);
        assert!(h.store.records.contains_key(&2));
        if fetch {
            assert_eq!(get(&mut h, r(2), 1), Word::signed(77));
        } else {
            assert_eq!(h.oracle.state.selected.unwrap().class, r(2));
        }
    }
    Ok(())
}
#[test]
fn bad_graphs_abort_before_mutation_and_reset_discards_a_paused_recovery() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    sparse(&mut h)?;
    for bad in [r(99), Word::reference(2, false)?, Word::from_bits(1 << 39)?] {
        let before = h.oracle.clone();
        let roots = Roots {
            driver: vec![bad],
            ..Default::default()
        };
        assert!(h.recover(RecoveryMode::Collect, &roots).is_err());
        assert_eq!(h.oracle, before);
        assert_eq!(h.rtl.dbg_maintenance_o, 0);
    }
    assert!(h
        .recover_with_fault(RecoveryMode::Compact, &Roots::default(), Some(4))
        .is_err());
    assert!(h.recovery_pending());
    h.reset()?;
    assert!(!h.recovery_pending());
    for _ in 0..20 {
        h.tick()?;
        assert_eq!(h.rtl.mem_valid_o, 0);
        assert_eq!(h.rtl.store_valid_o, 0);
    }
    bootstrap(&mut h)?;
    assert_eq!(good(&mut h, Command::allocate(r(1), 1, true)?), r(2));
    Ok(())
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn random_graphs_survive_collection(nodes in prop::collection::vec((any::<bool>(),any::<u8>(),any::<u8>(),any::<bool>(),any::<bool>()),1..13),delay in 0u32..4) {
        let runtime=runtime().unwrap();let mut h=Harness::new(&runtime,Timing{request_delay:delay,memory_latency:delay,response_stall:delay},None).unwrap();bootstrap(&mut h).unwrap();
        let refs:Vec<_>=nodes.iter().enumerate().map(|(i,n)|Word::reference(i as u64+2,n.0).unwrap()).collect();
        for (i,(_,a,b,_,_)) in nodes.iter().enumerate() {
            h.install(refs[i],r(1),20+i as u32*5,&[refs[*a as usize%nodes.len()],refs[*b as usize%nodes.len()]]).unwrap();
        }
        for (i,n) in nodes.iter().enumerate() {if n.4 {h.service(Service::Invalidate(refs[i])).unwrap();}}
        good(&mut h,Command::probe(r(1)));
        let roots=Roots{driver:nodes.iter().enumerate().filter(|(_,n)|n.3).map(|(i,_)|refs[i]).collect(),..Default::default()};
        let mut expected=BTreeSet::from([1]);let mut todo:Vec<_>=nodes.iter().enumerate().filter(|(_,n)|n.3).map(|(i,_)|i).collect();
        while let Some(i)=todo.pop() {
            if !expected.insert(i as u64+2) {continue;}
            if nodes[i].0 {todo.push(nodes[i].1 as usize%nodes.len());todo.push(nodes[i].2 as usize%nodes.len());}
        }
        let report=h.recover(RecoveryMode::Collect,&roots).unwrap();
        assert_eq!(h.store.records.keys().copied().collect::<BTreeSet<_>>(),expected);assert_eq!(report.objects_after,expected.len());
        let again=h.recover(RecoveryMode::Collect,&roots).unwrap();assert_eq!(again.cursor_before,again.cursor_after);assert_eq!(again.objects_before,again.objects_after);
        for (i,reference) in refs.iter().enumerate() {
            let response=h.execute(Command::fetch(*reference)).unwrap();
            if expected.contains(&(i as u64+2)) {
                assert_eq!(response.status,Status::Ok);good(&mut h,Command::index(1).unwrap());
                assert_eq!(good(&mut h,Command::read_field()),refs[nodes[i].1 as usize%nodes.len()]);
            } else {assert_eq!(response.status,Status::InvalidReference);}
        }
    }
}

#[test]
fn failed_reservations_are_reclaimed_without_identity_reuse() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    bootstrap(&mut h)?;
    good(&mut h, Command::probe(r(1)));
    let out = h.execute_faults(
        Command::allocate(r(1), 100, true)?.encode()?,
        Faults {
            memory_at: Some(20),
            store_at: None,
        },
    )?;
    assert_eq!(out.status, Status::MemoryError);
    assert_eq!(h.oracle.body_cursor, 100);
    assert_eq!(h.oracle.next_identity, 3);
    let report = h.recover(RecoveryMode::Compact, &Roots::default())?;
    assert_eq!(report.cursor_after, 0);
    assert_eq!(h.oracle.next_identity, 3);
    assert_eq!(good(&mut h, Command::allocate(r(1), 512, true)?), r(3));
    assert_eq!(get(&mut h, r(3), 512), Word::NIL);
    Ok(())
}

#[test]
fn selected_nonresident_object_is_a_root_and_preparation_rejects_bad_layouts() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    sparse(&mut h)?;
    h.service(Service::Invalidate(r(2)))?;
    h.recover(RecoveryMode::Collect, &Roots::default())?;
    assert_eq!(
        h.store.records.keys().copied().collect::<BTreeSet<_>>(),
        BTreeSet::from([1, 2])
    );
    assert_eq!(get(&mut h, r(2), 2), Word::signed(22));
    let selected = h.oracle.state.selected.unwrap();
    // The maintenance interface trusts metadata installation. Recovery validates before moving.
    h.service(Service::Install(Entry {
        representation: Word::NIL,
        ..selected
    }))?;
    let before = h.oracle.clone();
    assert!(h.recover(RecoveryMode::Compact, &Roots::default()).is_err());
    assert_eq!(h.oracle, before);
    h.service(Service::Install(Entry {
        base: 511,
        size: 2,
        ..selected
    }))?;
    let before = h.oracle.clone();
    assert!(h.recover(RecoveryMode::Compact, &Roots::default()).is_err());
    assert_eq!(h.oracle, before);
    Ok(())
}

#[test]
fn collection_does_not_reset_exhausted_identity_space() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    bootstrap(&mut h)?;
    good(&mut h, Command::probe(r(1)));
    h.service(Service::Install(Entry {
        reference: r(ID_MASK),
        class: r(1),
        base: 0,
        size: 0,
        representation: Word::NIL,
        new: true,
        modified: false,
        cond: false,
    }))?;
    let report = h.recover(RecoveryMode::Collect, &Roots::default())?;
    assert_eq!(report.objects_after, 1);
    assert_eq!(h.oracle.next_identity, ID_MASK + 1);
    assert_eq!(
        h.execute(Command::allocate(r(1), 0, true)?)?.status,
        Status::IdentityExhausted
    );
    Ok(())
}
