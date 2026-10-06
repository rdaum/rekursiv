use eyre::Result;
use proptest::prelude::*;
use rekursiv_asm::*;
use rekursiv_sim::{runtime, Harness, Timing};
fn r(id: u64) -> Word {
    Word::reference(id, true).unwrap()
}
fn good(h: &mut Harness<'_>, c: Command) -> Word {
    let response = h.execute(c).unwrap();
    assert_eq!(response.status, Status::Ok, "{c}");
    response.data
}
fn bootstrap(h: &mut Harness<'_>) -> Result<()> {
    h.install(r(1), r(1), 0, &[])
}
fn allocate(h: &mut Harness<'_>, size: u32, scan: bool) -> Word {
    good(h, Command::allocate(r(1), size, scan).unwrap())
}
fn field(h: &mut Harness<'_>, index: i64) -> Word {
    good(h, Command::index(index).unwrap());
    good(h, Command::read_field())
}
fn dirty_victim(h: &mut Harness<'_>) -> Result<Word> {
    bootstrap(h)?;
    let victim = allocate(h, 3, true);
    good(h, Command::index(1)?);
    good(h, Command::write_field(Word::signed(-123)));
    for _ in 0..15 {
        allocate(h, 0, false);
    }
    good(h, Command::probe(victim));
    Ok(victim)
}
#[test]
fn initialized_allocation_dirty_writeback_and_refill() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(
        &runtime,
        Timing {
            request_delay: 2,
            memory_latency: 4,
            response_stall: 3,
        },
        None,
    )?;
    let victim = dirty_victim(&mut h)?;
    let old = h.oracle.state.selected.unwrap();
    let replacement = allocate(&mut h, 2, false);
    assert_eq!(replacement.identity()?, victim.identity()? + 16);
    assert_eq!(field(&mut h, 1), Word::ZERO);
    assert_eq!(field(&mut h, 2), Word::ZERO);
    assert_eq!(
        h.store.records[&victim.identity()?].body,
        [Word::signed(-123), Word::NIL, Word::NIL]
    );
    assert_eq!(
        h.execute(Command::probe(victim))?.status,
        Status::NotResident
    );
    good(&mut h, Command::fetch(victim));
    assert_eq!(field(&mut h, 1), Word::signed(-123));
    assert_eq!(field(&mut h, 3), Word::NIL);
    let restored = h.oracle.state.selected.unwrap();
    assert!(restored.base > old.base);
    assert!(!restored.new && !restored.modified);
    assert_eq!(restored.representation, Word::signed(-123));
    let requests = h.store_requests.len();
    good(&mut h, Command::fetch(victim));
    assert_eq!(requests, h.store_requests.len());
    assert!(h.stats.saved_objects >= 2);
    Ok(())
}
#[test]
fn every_allocation_transfer_failure_preserves_the_victim() -> Result<()> {
    let runtime = runtime()?;
    // Three victim reads, two initialization writes; begin, three puts, commit.
    for faults in (0..5)
        .map(|n| Faults {
            memory_at: Some(n),
            store_at: None,
        })
        .chain((0..5).map(|n| Faults {
            memory_at: None,
            store_at: Some(n),
        }))
    {
        let mut h = Harness::new(
            &runtime,
            Timing {
                request_delay: 1,
                memory_latency: 2,
                response_stall: 4,
            },
            None,
        )?;
        let victim = dirty_victim(&mut h)?;
        let before = h.oracle.state.clone();
        let next = h.oracle.next_identity;
        let cursor = h.oracle.body_cursor;
        let c = Command::allocate(r(1), 2, true)?;
        let response = h.execute_faults(c.encode()?, faults)?;
        assert_eq!(
            response.status,
            if faults.memory_at.is_some() {
                Status::MemoryError
            } else {
                Status::ServiceError
            }
        );
        assert_eq!(h.oracle.state, before);
        assert_eq!(h.oracle.next_identity, next + 1);
        assert_eq!(h.oracle.body_cursor, cursor + 2);
        assert_eq!(field(&mut h, 1), Word::signed(-123));
        assert_eq!(h.oracle.state.selected.unwrap().reference, victim);
        if let Some(record) = h.store.records.get(&victim.identity()?) {
            assert_eq!(record.body, [Word::signed(-123), Word::NIL, Word::NIL]);
        }
        // A later save must replace any abandoned staging transaction cleanly.
        for _ in 0..16 {
            allocate(&mut h, 0, false);
        }
        good(&mut h, Command::fetch(victim));
        assert_eq!(field(&mut h, 1), Word::signed(-123));
    }
    Ok(())
}
#[test]
fn every_refill_failure_preserves_selected_state_and_published_records() -> Result<()> {
    let runtime = runtime()?;
    for faults in (0..2)
        .map(|n| Faults {
            memory_at: Some(n),
            store_at: None,
        })
        .chain((0..3).map(|n| Faults {
            memory_at: None,
            store_at: Some(n),
        }))
    {
        let mut h = Harness::new(&runtime, Timing::default(), None)?;
        bootstrap(&mut h)?;
        h.install(r(2), r(1), 0, &[Word::signed(10), Word::signed(20)])?;
        h.service(Service::Invalidate(r(2)))?;
        good(&mut h, Command::probe(r(1)));
        let before = h.oracle.state.clone();
        let records = h.store.records.clone();
        let response = h.execute_faults(Command::fetch(r(2)).encode()?, faults)?;
        assert_eq!(
            response.status,
            if faults.memory_at.is_some() {
                Status::MemoryError
            } else {
                Status::ServiceError
            }
        );
        assert_eq!(h.oracle.state, before);
        assert_eq!(h.store.records, records);
        assert!(h.oracle.entries[2].is_none());
        good(&mut h, Command::fetch(r(2)));
        assert_eq!(field(&mut h, 2), Word::signed(20));
    }
    Ok(())
}
#[test]
fn limits_unknown_references_and_empty_objects() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    bootstrap(&mut h)?;
    assert_eq!(
        h.execute(Command::fetch(r(99)))?.status,
        Status::InvalidReference
    );
    h.install(r(2), r(1), 0, &[Word::NIL])?;
    h.service(Service::Invalidate(r(2)))?;
    assert_eq!(
        h.execute(Command::fetch(Word::reference(2, false)?))?
            .status,
        Status::InvalidReference
    );
    let cursor = h.oracle.body_cursor;
    let next = h.oracle.next_identity;
    assert_eq!(
        h.execute(Command::allocate(r(1), 512, true)?)?.status,
        Status::OutOfSpace
    );
    assert_eq!(h.oracle.body_cursor, cursor);
    assert_eq!(h.oracle.next_identity, next);
    allocate(&mut h, 511, false);
    let empty = allocate(&mut h, 0, true);
    assert_eq!(h.oracle.body_cursor, 512);
    assert_eq!(h.oracle.state.selected.unwrap().representation, Word::NIL);
    good(&mut h, Command::index(1)?);
    assert_eq!(
        h.execute(Command::read_field())?.status,
        Status::BoundsError
    );
    assert_eq!(
        h.execute(Command::allocate(r(1), 1, false)?)?.status,
        Status::OutOfSpace
    );
    for _ in 0..16 {
        allocate(&mut h, 0, true);
    }
    good(&mut h, Command::fetch(empty));
    assert_eq!(h.oracle.state.selected.unwrap().size, 0);
    // The maximum identity advances the high-water mark without wrapping.
    h.service(Service::Install(Entry {
        reference: r(ID_MASK),
        class: r(1),
        size: 0,
        base: 0,
        representation: Word::NIL,
        new: true,
        modified: false,
        cond: false,
    }))?;
    assert_eq!(
        h.execute(Command::allocate(r(1), 0, true)?)?.status,
        Status::IdentityExhausted
    );
    assert_eq!(h.oracle.next_identity, ID_MASK + 1);
    Ok(())
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]
    #[test]
    fn random_allocation_and_fetch_sequences(steps in prop::collection::vec((any::<u8>(),any::<u32>(),0u32..4,0u32..4),20..80)) {
        let runtime=runtime().unwrap();let mut h=Harness::new(&runtime,Timing::default(),None).unwrap();bootstrap(&mut h).unwrap();
        let mut objects=Vec::new();
        for (op,value,delay,latency) in steps {
            h.timing=Timing{request_delay:delay,memory_latency:latency,response_stall:delay};
            h.store_timing=Timing{request_delay:latency,memory_latency:delay,response_stall:0};
            if objects.is_empty() || op%3==0 {
                let c=Command::allocate(r(1),1+value%4,op&1!=0).unwrap();
                let faults=Faults{memory_at:(op&16!=0).then_some((op%6) as usize),store_at:(op&32!=0).then_some((op%5) as usize)};
                let out=h.execute_faults(c.encode().unwrap(),faults).unwrap();
                if out.status==Status::Ok {objects.push(out.data);}
            } else {
                let reference=objects[value as usize%objects.len()];
                let faults=Faults{memory_at:(op&16!=0).then_some(0),store_at:(op&32!=0).then_some(0)};
                let out=h.execute_faults(Command::fetch(reference).encode().unwrap(),faults).unwrap();
                if out.status==Status::Ok {
                    good(&mut h,Command::index(1).unwrap());
                    if op%3==1 {good(&mut h,Command::write_field(Word::unsigned(value)));}
                    else {good(&mut h,Command::read_field());}
                }
            }
        }
    }
}

#[test]
fn transfer_latches_inputs_and_publishes_after_save_and_initialization() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(
        &runtime,
        Timing {
            request_delay: 3,
            memory_latency: 5,
            response_stall: 0,
        },
        None,
    )?;
    let victim = dirty_victim(&mut h)?;
    let before = h.oracle.state.clone();
    let count = h.stats;
    let command = Command::allocate(r(1), 2, true)?;
    h.drive(command.encode()?)?;
    h.rtl.cmd_valid_i = 1;
    h.rtl.run_i = 1;
    assert!(h.tick()?.command);
    h.drive(Ports {
        pager: 15,
        data: WORD_MASK,
        ..Default::default()
    })?;
    h.rtl.run_i = 0;
    h.rtl.svc_valid_i = 1;
    h.rtl.svc_op_i = 1;
    h.rtl.svc_ref_i = victim.bits();
    let mut saw_fill = false;
    for _ in 0..1000 {
        if h.rtl.rsp_valid_o != 0 {
            break;
        }
        assert_eq!(h.snapshot()?, before);
        assert_eq!(h.rtl.cmd_ready_o, 0);
        assert_eq!(h.rtl.svc_ready_o, 0);
        h.rtl.dbg_slot_i = (victim.identity()? & 15) as u8;
        h.rtl.eval();
        assert_eq!(h.rtl.dbg_entry_ref_o, victim.bits());
        assert_eq!(h.rtl.dbg_valid_o, 1);
        if h.rtl.mem_valid_o != 0 && h.rtl.mem_write_o != 0 {
            saw_fill = true;
            assert_eq!(
                h.store.records[&victim.identity()?].body,
                [Word::signed(-123), Word::NIL, Word::NIL]
            );
        }
        let edge = h.tick()?;
        assert!(!edge.command && !edge.service && !edge.response);
    }
    assert!(saw_fill);
    assert_eq!(h.rtl.rsp_valid_o, 1);
    let expected = h.oracle.execute(command, false);
    assert_eq!(expected.response.status, Status::Ok);
    assert_eq!(h.rtl.rsp_data_o, expected.response.data.bits());
    h.compare_state()?;
    let completed = h.stats;
    for _ in 0..20 {
        h.tick()?;
        h.compare_state()?;
    }
    assert_eq!(h.stats.writes, completed.writes);
    assert_eq!(h.stats.saved_objects, completed.saved_objects);
    assert_eq!(h.stats.store_transactions, completed.store_transactions);
    assert_eq!(h.stats.responses, count.responses);
    assert_eq!(h.stats.commands, count.commands + 1);
    assert_eq!(h.stats.services, count.services);
    h.rtl.cmd_valid_i = 0;
    h.rtl.svc_valid_i = 0;
    h.rtl.rsp_ready_i = 1;
    assert!(h.tick()?.response);
    assert!(!h.tick()?.response);
    assert_eq!(h.stats.responses, count.responses + 1);
    Ok(())
}

#[test]
fn cold_reset_cancels_each_transfer_phase() -> Result<()> {
    let runtime = runtime()?;
    for point in 0..7 {
        for extra_ticks in [0, 1, 3] {
            let mut h = Harness::new(
                &runtime,
                Timing {
                    request_delay: 2,
                    memory_latency: 3,
                    response_stall: 0,
                },
                None,
            )?;
            dirty_victim(&mut h)?;
            h.drive(Command::allocate(r(1), 2, true)?.encode()?)?;
            h.rtl.run_i = 1;
            h.rtl.cmd_valid_i = 1;
            assert!(h.tick()?.command);
            h.rtl.cmd_valid_i = 0;
            let mut reached = false;
            for _ in 0..1000 {
                reached = match point {
                    0 => h.rtl.store_valid_o != 0 && h.rtl.store_op_o == 2,
                    1 => h.rtl.store_rsp_ready_o != 0,
                    2 => h.rtl.mem_valid_o != 0 && h.rtl.mem_write_o == 0,
                    3 => h.rtl.store_valid_o != 0 && h.rtl.store_op_o == 3,
                    4 => h.rtl.store_valid_o != 0 && h.rtl.store_op_o == 4,
                    5 => h.rtl.mem_valid_o != 0 && h.rtl.mem_write_o != 0,
                    _ => h.rtl.rsp_valid_o != 0,
                };
                if reached {
                    break;
                }
                h.tick()?;
            }
            assert!(reached, "reset point {point} was not reached");
            for _ in 0..extra_ticks {
                h.tick()?;
            }
            h.reset()?;
            for _ in 0..30 {
                h.tick()?;
                assert_eq!(h.rtl.store_valid_o, 0);
                assert_eq!(h.rtl.mem_valid_o, 0);
                assert_eq!(h.rtl.rsp_valid_o, 0);
            }
            h.compare_state()?;
            bootstrap(&mut h)?;
            assert_eq!(allocate(&mut h, 1, true), r(2));
            assert_eq!(field(&mut h, 1), Word::NIL);
        }
    }
    Ok(())
}

#[test]
fn clean_eviction_compact_fetch_and_bad_transfer_commands() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    bootstrap(&mut h)?;
    h.install(r(2), r(1), 0, &[Word::signed(44)])?;
    for _ in 0..16 {
        allocate(&mut h, 0, true);
    }
    // The preinstalled clean object already has a backing record and needs no save.
    assert!(!h
        .store_requests
        .iter()
        .any(|req| req.reference == r(2) && req.op == StoreOp::BeginSave));
    good(&mut h, Command::fetch(r(2)));
    assert_eq!(field(&mut h, 1), Word::signed(44));
    h.service(Service::CompactClass {
        code: 0,
        class: r(1),
    })?;
    let count = h.store_requests.len();
    good(&mut h, Command::fetch(Word::NIL));
    assert_eq!(h.store_requests.len(), count);
    let before = h.oracle.state.clone();
    for p in [
        Ports {
            pager: 6,
            ..Default::default()
        },
        Ports {
            pager: 5,
            ..Default::default()
        },
    ] {
        assert_eq!(h.execute_raw(p, false)?.status, Status::InvalidReference);
    }
    for p in [
        Ports {
            pager: 6,
            check_type: 1,
            expected_type: Word::NIL.bits(),
            ..Default::default()
        },
        Ports {
            pager: 5,
            load_vr: 1,
            ..Default::default()
        },
    ] {
        assert_eq!(h.execute_raw(p, false)?.status, Status::BadCommand);
    }
    assert_eq!(h.oracle.state, before);
    Ok(())
}

#[test]
fn failed_refill_with_dirty_collision_preserves_both_objects() -> Result<()> {
    let runtime = runtime()?;
    // Fetch metadata, begin save, two puts, commit, three incoming reads.
    for faults in (0..5)
        .map(|n| Faults {
            memory_at: Some(n),
            store_at: None,
        })
        .chain((0..8).map(|n| Faults {
            memory_at: None,
            store_at: Some(n),
        }))
    {
        let mut h = Harness::new(
            &runtime,
            Timing {
                request_delay: 2,
                memory_latency: 1,
                response_stall: 3,
            },
            None,
        )?;
        let incoming = dirty_victim(&mut h)?;
        let victim = allocate(&mut h, 2, false);
        good(&mut h, Command::index(2)?);
        good(&mut h, Command::write_field(Word::signed(456)));
        let e = h.oracle.state.selected.unwrap();
        h.service(Service::Install(Entry { cond: true, ..e }))?;
        let before = h.oracle.state.clone();
        let original = h.store.records[&incoming.identity()?].clone();
        let response = h.execute_faults(Command::fetch(incoming).encode()?, faults)?;
        assert_eq!(
            response.status,
            if faults.memory_at.is_some() {
                Status::MemoryError
            } else {
                Status::ServiceError
            }
        );
        assert_eq!(h.oracle.state, before);
        assert_eq!(h.store.records[&incoming.identity()?], original);
        assert_eq!(field(&mut h, 2), Word::signed(456));
        good(&mut h, Command::fetch(incoming));
        assert_eq!(field(&mut h, 1), Word::signed(-123));
        good(&mut h, Command::fetch(victim));
        assert_eq!(field(&mut h, 2), Word::signed(456));
        assert!(h.oracle.state.selected.unwrap().cond);
    }
    Ok(())
}

#[test]
fn modified_clean_object_replaces_backing_record_only_after_commit() -> Result<()> {
    let runtime = runtime()?;
    for fail_commit in [false, true] {
        let mut h = Harness::new(&runtime, Timing::default(), None)?;
        bootstrap(&mut h)?;
        h.install(r(2), r(1), 0, &[Word::signed(11), Word::signed(22)])?;
        good(&mut h, Command::probe(r(2)));
        good(&mut h, Command::index(1)?);
        good(&mut h, Command::write_field(Word::signed(99)));
        assert!(!h.oracle.state.selected.unwrap().new);
        assert!(h.oracle.state.selected.unwrap().modified);
        for _ in 0..15 {
            allocate(&mut h, 0, false);
        }
        good(&mut h, Command::probe(r(2)));
        let result = h.execute_faults(
            Command::allocate(r(1), 0, false)?.encode()?,
            Faults {
                memory_at: None,
                store_at: fail_commit.then_some(3),
            },
        )?;
        if fail_commit {
            assert_eq!(result.status, Status::ServiceError);
            assert_eq!(
                h.store.records[&2].body,
                [Word::signed(11), Word::signed(22)]
            );
            assert_eq!(field(&mut h, 1), Word::signed(99));
        } else {
            assert_eq!(result.status, Status::Ok);
            assert_eq!(
                h.store.records[&2].body,
                [Word::signed(99), Word::signed(22)]
            );
            good(&mut h, Command::fetch(r(2)));
            assert_eq!(field(&mut h, 1), Word::signed(99));
            assert!(!h.oracle.state.selected.unwrap().modified);
        }
    }
    Ok(())
}
