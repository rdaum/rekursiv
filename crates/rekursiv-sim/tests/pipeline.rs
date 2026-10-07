//! Prepared-address semantics, checked against the native OBJEKT model after
//! every command, including metadata, latches, RAM traffic, and fault rollback.
use eyre::Result;
use rekursiv_asm::*;
use rekursiv_sim::{runtime, Harness, Timing};

fn r(id: u64) -> Word {
    Word::reference(id, true).unwrap()
}
fn good(h: &mut Harness<'_>, c: Command) -> Word {
    let out = h.execute(c).unwrap();
    assert_eq!(out.status, Status::Ok, "{c:?}");
    out.data
}
fn prepare(index: Index) -> Command {
    Command {
        prepare: true,
        index,
        ..Default::default()
    }
}
fn read() -> Command {
    Command {
        prepared: true,
        memory: Memory::Read,
        ..Default::default()
    }
}
fn fixture(h: &mut Harness<'_>) -> Result<()> {
    h.install(
        r(1),
        r(8),
        16,
        &[Word::unsigned(11), Word::unsigned(12), Word::unsigned(13)],
    )?;
    h.install(r(2), r(8), 24, &[Word::unsigned(21), Word::unsigned(22)])?;
    good(h, Command::probe(r(1)));
    Ok(())
}

#[test]
fn new_index_prepares_while_old_address_is_consumed_and_selection_changes() -> Result<()> {
    let rt = runtime()?;
    for timing in [
        Timing::default(),
        Timing {
            request_delay: 5,
            memory_latency: 9,
            response_stall: 7,
        },
    ] {
        let mut h = Harness::new(&rt, timing, None)?;
        fixture(&mut h)?;
        good(&mut h, prepare(Index::One));
        let requests = h.transfers.len();
        assert_eq!(
            good(
                &mut h,
                Command {
                    prepare: true,
                    index: Index::Increment,
                    ..read()
                }
            ),
            Word::unsigned(11)
        );
        assert_eq!(
            h.transfers.len(),
            requests,
            "first component uses pager cache"
        );
        assert_eq!(h.oracle.state.prepared.address, 17);
        assert_eq!(
            good(
                &mut h,
                Command {
                    pager: Pager::ProbeBus,
                    data: r(2),
                    index: Index::Increment,
                    prepare: true,
                    ..read()
                }
            ),
            Word::unsigned(12)
        );
        assert_eq!(h.oracle.state.selected.unwrap().reference, r(2));
        assert_eq!(h.oracle.state.prepared.reference, r(1));
        good(&mut h, Command::index(99)?);
        assert_eq!(good(&mut h, read()), Word::unsigned(13));
        // Now prepare against B and consume the last address from A together.
        assert_eq!(
            good(
                &mut h,
                Command {
                    index: Index::Two,
                    prepare: true,
                    ..read()
                }
            ),
            Word::unsigned(13)
        );
        assert_eq!(good(&mut h, read()), Word::unsigned(22));
    }
    Ok(())
}

#[test]
fn writes_update_the_prepared_object_and_forward_first_component_cache() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 3,
            memory_latency: 7,
            response_stall: 5,
        },
        None,
    )?;
    fixture(&mut h)?;
    good(&mut h, prepare(Index::One));
    // D is shared by the pager and write, so this writes B's reference into A.
    good(
        &mut h,
        Command {
            pager: Pager::ProbeBus,
            data: r(2),
            prepared: true,
            memory: Memory::Write,
            ..Default::default()
        },
    );
    assert_eq!(h.oracle.state.selected.unwrap().reference, r(2));
    assert_eq!(
        h.oracle.state.selected.unwrap().representation,
        Word::unsigned(21)
    );
    assert_eq!(h.oracle.entries[1].unwrap().representation, r(2));
    assert!(h.oracle.entries[1].unwrap().modified);
    let requests = h.transfers.len();
    assert_eq!(good(&mut h, read()), r(2));
    assert_eq!(h.transfers.len(), requests);
    Ok(())
}

#[test]
fn deferred_bounds_and_memory_faults_preserve_the_pipeline() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 2,
            memory_latency: 5,
            response_stall: 3,
        },
        None,
    )?;
    assert_eq!(h.execute(read())?.status, Status::NoSelection);
    fixture(&mut h)?;
    good(&mut h, prepare(Index::Two));
    let command = Command {
        prepare: true,
        index: Index::Increment,
        load_vr: true,
        vr: 7,
        data: Word::signed(99),
        memory: Memory::Write,
        prepared: true,
        ..Default::default()
    };
    let before = h.oracle.clone();
    assert_eq!(
        h.execute_raw(command.encode()?, true)?.status,
        Status::MemoryError
    );
    assert_eq!(h.oracle, before);
    good(&mut h, command);
    assert_eq!(h.oracle.state.prepared.address, 18);
    assert_eq!(
        good(
            &mut h,
            Command {
                prepare: true,
                index: Index::Increment,
                ..read()
            }
        ),
        Word::unsigned(13)
    );
    assert_eq!(h.oracle.state.prepared.status, Status::BoundsError);
    let before = h.oracle.clone();
    assert_eq!(
        h.execute(Command {
            pager: Pager::ProbeBus,
            data: r(2),
            prepare: true,
            index: Index::One,
            ..read()
        })?
        .status,
        Status::BoundsError
    );
    assert_eq!(h.oracle, before);
    good(&mut h, prepare(Index::One));
    assert_eq!(good(&mut h, read()), Word::unsigned(11));
    Ok(())
}

#[test]
fn mapping_changes_invalidate_addresses_instead_of_accessing_reused_ram() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    fixture(&mut h)?;
    good(&mut h, prepare(Index::Two));
    h.service(Service::Invalidate(r(1)))?;
    assert_eq!(h.execute(read())?.status, Status::NotResident);
    h.install(r(1), r(8), 32, &[Word::unsigned(31), Word::unsigned(32)])?;
    assert_eq!(h.execute(read())?.status, Status::NotResident);
    good(&mut h, prepare(Index::Two));
    assert_eq!(good(&mut h, read()), Word::unsigned(32));
    // A colliding pager publication also kills the prepared address.
    h.install(r(17), r(8), 40, &[Word::unsigned(41), Word::unsigned(42)])?;
    assert_eq!(h.execute(read())?.status, Status::NotResident);
    Ok(())
}

#[test]
fn launched_write_overlaps_local_retirement_but_halt_drains_errors_and_backpressure() -> Result<()>
{
    let rt = runtime()?;
    for fail in [false, true] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        fixture(&mut h)?;
        let a = rekursiv_asm::text::assemble(
            "idx=Two, prepare\n\
            d=99, mem=Write, prepared, prepare, idx=Increment, launch\n\
            d=17, r=Bus, rb=7, ldrb\n\
            d=23, r=Bus, rb=8, ldrb\n\
            halt",
            0,
            &[],
        )?;
        let image = rekursiv_model::processor::Image::from_assembly(&a)?;
        h.load_processor(&image)?;
        h.timing = Timing {
            request_delay: 9,
            memory_latency: 23,
            response_stall: 0,
        };
        h.fail_next_memory = fail;
        let before = h.stats;
        h.start_processor(0)?;
        let mut overlapped = false;
        for cycle in 0..500 {
            if h.rtl.cpu_halted_o != 0 {
                break;
            }
            h.rtl.cpu_command_enable_i = (cycle % 7 > 2) as u8;
            h.rtl.cpu_response_enable_i = (cycle % 19 > 13) as u8;
            h.tick()?;
            if h.rtl.cpu_retire_o != 0 && h.rtl.cpu_pc_o == 3 {
                // R7 retired before the memory write completed, even when that
                // write will fail. Halt must still wait for its completion.
                assert_eq!(h.stats.writes, before.writes);
                assert_eq!(h.stats.responses, before.responses + 1);
                overlapped = true;
            }
        }
        assert!(overlapped);
        assert_ne!(h.rtl.cpu_halted_o, 0);
        assert_eq!(h.rtl.cpu_pc_o, 4);
        assert_eq!(h.rtl.cpu_fault_o, if fail { 4 } else { 0 });
        assert_eq!(
            h.rtl.cpu_last_status_o,
            if fail { Status::MemoryError as u8 } else { 0 }
        );
        for (register, value) in [(7, 17), (8, 23)] {
            h.rtl.cpu_dbg_addr_i = register;
            h.rtl.eval();
            assert_eq!(h.rtl.cpu_dbg_rf_o, value);
        }
        h.oracle.execute(prepare(Index::Two), false);
        h.oracle.execute(
            Command {
                prepared: true,
                prepare: true,
                index: Index::Increment,
                ..Command::write_field(Word::raw(99)?)
            },
            fail,
        );
        h.compare_state()?;
        assert_eq!(h.stats.commands, before.commands + 2);
        assert_eq!(h.stats.responses, before.responses + 2);
        assert_eq!(h.stats.writes, before.writes + u64::from(!fail));
    }
    Ok(())
}

#[test]
fn service_compaction_invalidates_prepared_addresses_and_allows_reprepare() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    fixture(&mut h)?;
    good(&mut h, prepare(Index::Two));
    h.recover(RecoveryMode::Compact, &Roots::default())?;
    assert_eq!(h.execute(read())?.status, Status::NotResident);
    good(&mut h, prepare(Index::Two));
    assert_eq!(good(&mut h, read()), Word::unsigned(12));
    Ok(())
}

#[test]
fn refill_and_exchange_invalidate_only_on_successful_publication() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    fixture(&mut h)?;
    let record = rekursiv_model::store::Record {
        reference: r(17),
        class: r(8),
        cond: false,
        body: vec![Word::unsigned(41), Word::unsigned(42)],
    };
    h.store.records.insert(17, record.clone());
    h.oracle.store.records.insert(17, record);
    good(&mut h, prepare(Index::Two));
    let before = h.oracle.state.prepared;
    assert_eq!(
        h.execute_raw(Command::fetch(r(17)).encode()?, true)?.status,
        Status::MemoryError
    );
    assert_eq!(h.oracle.state.prepared, before);
    assert_eq!(good(&mut h, read()), Word::unsigned(12));
    good(&mut h, Command::fetch(r(17)));
    assert_eq!(h.execute(read())?.status, Status::NotResident);
    good(&mut h, Command::fetch(r(1)));
    assert_eq!(h.execute(read())?.status, Status::NotResident);
    good(&mut h, prepare(Index::Two));
    good(
        &mut h,
        Command {
            load_vr: true,
            vr: 7,
            data: r(1),
            ..Default::default()
        },
    );
    let exchange = Command {
        pager: Pager::Exchange,
        vr: 7,
        data: r(2),
        ..Default::default()
    };
    let before = h.oracle.state.prepared;
    assert_eq!(
        h.execute_faults(
            exchange.encode()?,
            Faults {
                store_at: Some(0),
                memory_at: None
            }
        )?
        .status,
        Status::ServiceError
    );
    assert_eq!(h.oracle.state.prepared, before);
    good(&mut h, exchange);
    assert_eq!(h.execute(read())?.status, Status::NotResident);
    good(&mut h, Command::fetch(r(1)));
    good(&mut h, prepare(Index::Two));
    assert_eq!(good(&mut h, read()), Word::unsigned(22));
    Ok(())
}
