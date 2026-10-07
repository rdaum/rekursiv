use eyre::Result;
use proptest::prelude::*;
use rekursiv_asm::*;
use rekursiv_sim::{programs, runtime, Harness, Timing};
fn r(id: u64) -> Word {
    Word::reference(id, true).unwrap()
}
fn good(h: &mut Harness<'_>, c: Command) -> Word {
    let out = h.execute(c).unwrap();
    assert_eq!(out.status, Status::Ok, "{c}");
    out.data
}
fn entry(id: u64, base: u32, size: u32) -> Entry {
    Entry {
        reference: r(id),
        class: r(8),
        base,
        size,
        representation: if size == 0 {
            Word::NIL
        } else {
            Word::unsigned(id as u32)
        },
        new: false,
        modified: false,
        cond: false,
    }
}
fn fixture(h: &mut Harness<'_>) -> Result<()> {
    for code in 0..4 {
        assert_eq!(
            h.service(Service::CompactClass {
                code,
                class: r(8 + code as u64)
            })?
            .status,
            Status::Ok
        );
    }
    h.install(
        r(1),
        r(8),
        16,
        &[
            Word::unsigned(1),
            Word::unsigned(2),
            Word::unsigned(3),
            Word::unsigned(4),
        ],
    )?;
    h.install(r(2), r(8), 24, &[])?;
    h.install(r(3), r(9), 32, &[Word::unsigned(7), r(1)])?;
    good(h, Command::probe(r(1)));
    Ok(())
}

#[test]
fn capacity_reads_follow_allocator_reservations_without_object_selection_or_io() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 3,
            memory_latency: 5,
            response_stall: 2,
        },
        None,
    )?;
    assert_eq!(good(&mut h, Command::read(Read::FreeWords)).bits(), 512);
    assert_eq!(
        good(&mut h, Command::read(Read::FreeIdentities)).bits(),
        ID_MASK
    );
    assert!(h.oracle.state.selected.is_none());
    h.install(r(1), r(1), 0, &[Word::ZERO; 4])?;
    good(&mut h, Command::allocate(r(1), 5, true)?);
    assert_eq!(good(&mut h, Command::read(Read::FreeWords)).bits(), 503);
    let before = (h.oracle.body_cursor, h.oracle.next_identity);
    let failed = h.execute_raw(Command::allocate(r(1), 2, true)?.encode()?, true)?;
    assert_eq!(failed.status, Status::MemoryError);
    // A transfer failure retains its reserved identity/body range by contract.
    assert_eq!(
        (h.oracle.body_cursor, h.oracle.next_identity),
        (before.0 + 2, before.1 + 1)
    );
    let reserved = (h.oracle.body_cursor, h.oracle.next_identity);
    assert_eq!(
        h.execute(Command::allocate(r(1), 502, true)?)?.status,
        Status::OutOfSpace
    );
    assert_eq!((h.oracle.body_cursor, h.oracle.next_identity), reserved);
    h.service(Service::ReserveIdentities(ID_MASK - 1))?;
    for remaining in [2, 1, 0] {
        let state = h.oracle.state.clone();
        let traffic = (h.transfers.len(), h.store_requests.len());
        assert_eq!(
            good(&mut h, Command::read(Read::FreeIdentities)).bits(),
            remaining
        );
        assert_eq!(good(&mut h, Command::read(Read::FreeWords)).bits(), 501);
        assert_eq!(h.oracle.state, state);
        assert_eq!((h.transfers.len(), h.store_requests.len()), traffic);
        if remaining != 0 {
            good(&mut h, Command::allocate(r(1), 0, true)?);
        }
    }
    Ok(())
}
#[test]
fn all_examples_with_delays_and_backpressure() -> Result<()> {
    let runtime = runtime()?;
    for timing in [
        Timing::default(),
        Timing {
            request_delay: 3,
            memory_latency: 7,
            response_stall: 5,
        },
    ] {
        let mut h = Harness::new(&runtime, timing, None)?;
        for name in programs::NAMES {
            h.reset()?;
            programs::run(&mut h, name)?;
        }
    }
    Ok(())
}
#[test]
fn bounds_compacts_collisions_and_rollback() -> Result<()> {
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
    fixture(&mut h)?;
    for index in [0, -1, 5, INDEX_MIN, INDEX_MAX] {
        good(&mut h, Command::index(index)?);
        let before = h.oracle.state.clone();
        let transfers = h.transfers.len();
        for memory in [Memory::Read, Memory::Write] {
            let c = Command {
                memory,
                load_vr: true,
                data: Word::unsigned(88),
                ..Command::default()
            };
            assert_eq!(h.execute(c)?.status, Status::BoundsError);
            assert_eq!(h.oracle.state, before);
        }
        assert_eq!(h.transfers.len(), transfers);
    }
    good(&mut h, Command::index(4)?);
    assert_eq!(good(&mut h, Command::read_field()), Word::unsigned(4));
    good(&mut h, Command::write_field(Word::unsigned(44)));
    assert_eq!(good(&mut h, Command::read_field()), Word::unsigned(44));
    for missing in [r(17), r(7), Word::reference(1, false)?] {
        let selected = h.oracle.state.selected;
        assert_eq!(
            h.execute(Command::probe(missing))?.status,
            Status::NotResident
        );
        assert_eq!(h.oracle.state.selected, selected);
    }
    assert_eq!(
        h.execute(Command::probe(Word::from_bits(1 << 39)?))?.status,
        Status::InvalidReference
    );
    for bits in [0xc400000000, 0xc000000001, 0xc100000002] {
        assert_eq!(
            h.execute(Command::probe(Word::from_bits(bits)?))?.status,
            Status::BadValue
        );
    }
    for v in [
        Word::NIL,
        Word::boolean(false),
        Word::boolean(true),
        Word::signed(i32::MIN),
        Word::signed(i32::MAX),
        Word::unsigned(u32::MAX),
    ] {
        let n = h.transfers.len();
        good(&mut h, Command::probe(v));
        assert_eq!(
            good(&mut h, Command::read(Read::Type)),
            r(8 + v.compact_parts()?.0 as u64)
        );
        good(&mut h, Command::index(1)?);
        assert_eq!(
            h.execute(Command::read_field())?.status,
            Status::BoundsError
        );
        assert_eq!(h.transfers.len(), n);
    }
    good(&mut h, Command::probe(r(2)));
    good(&mut h, Command::index(0)?);
    assert_eq!(
        h.execute(Command {
            index: Index::Next,
            ..Command::default()
        })?
        .status,
        Status::BoundsError
    );
    good(&mut h, Command::probe(r(1)));
    good(&mut h, Command::index(0)?);
    for expect in [1, 2, 3, 4, 1] {
        good(
            &mut h,
            Command {
                index: Index::Next,
                ..Command::default()
            },
        );
        assert_eq!(h.oracle.state.index, expect);
    }
    for (idx, op) in [(INDEX_MAX, Index::Increment), (INDEX_MIN, Index::Decrement)] {
        good(&mut h, Command::index(idx)?);
        let before = h.oracle.state.clone();
        assert_eq!(
            h.execute(Command {
                index: op,
                ..Command::default()
            })?
            .status,
            Status::IndexOverflow
        );
        assert_eq!(h.oracle.state, before);
    }
    // Invalid physical spans are rejected at use before truncated addresses reach memory.
    for (base, size, index) in [(ADDRESS_LIMIT - 1, 2, 2), (511, 2, 2), (512, 1, 1)] {
        assert_eq!(
            h.service(Service::Install(entry(4, base, size)))?.status,
            Status::Ok
        );
        good(&mut h, Command::probe(r(4)));
        good(&mut h, Command::index(index)?);
        assert_eq!(
            h.execute(Command::read_field())?.status,
            Status::BoundsError
        );
    }
    Ok(())
}
#[test]
fn combined_fields_and_memory_errors() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(
        &runtime,
        Timing {
            request_delay: 4,
            memory_latency: 5,
            response_stall: 7,
        },
        None,
    )?;
    fixture(&mut h)?;
    good(&mut h, Command::index(1)?);
    let c = Command {
        index: Index::Increment,
        register: Register::FromIndex,
        load_vr: true,
        vr: 4,
        data: r(3),
        memory: Memory::Read,
        ..Command::default()
    };
    assert_eq!(good(&mut h, c), Word::unsigned(1));
    assert_eq!(h.oracle.state.index, 2);
    assert_eq!(h.oracle.state.index_reg, 1);
    let c = Command {
        pager: Pager::ProbeVr,
        load_vr: true,
        vr: 4,
        data: r(2),
        read: Read::Type,
        ..Command::default()
    };
    assert_eq!(good(&mut h, c), r(8));
    assert_eq!(h.oracle.state.selected.unwrap().reference, r(3));
    assert_eq!(h.oracle.state.vr[4], r(2));
    good(&mut h, Command::probe(r(1)));
    good(&mut h, Command::index(2)?);
    for memory in [Memory::Read, Memory::Write] {
        let c = Command {
            index: Index::Increment,
            register: Register::Increment,
            load_vr: true,
            vr: 5,
            data: Word::signed(-100),
            memory,
            ..Command::default()
        };
        let before = h.oracle.state.clone();
        assert_eq!(
            h.execute_raw(c.encode()?, true)?.status,
            Status::MemoryError
        );
        assert_eq!(h.oracle.state, before);
    }
    let c = Command {
        memory: Memory::Write,
        data: Word::unsigned(10),
        expected_type: Some(r(9)),
        ..Command::default()
    };
    let n = h.transfers.len();
    assert_eq!(h.execute(c)?.status, Status::TypeError);
    assert_eq!(h.transfers.len(), n);
    good(
        &mut h,
        Command {
            expected_type: Some(r(8)),
            ..c
        },
    );
    for p in [
        Ports {
            pager: 5,
            index: 1,
            ..Ports::default()
        },
        Ports {
            pager: 6,
            index: 1,
            ..Ports::default()
        },
        Ports {
            pager: 15,
            ..Ports::default()
        },
        Ports {
            index: 15,
            ..Ports::default()
        },
        Ports {
            register: 7,
            ..Ports::default()
        },
        Ports {
            memory: 3,
            ..Ports::default()
        },
        Ports {
            read: 15,
            ..Ports::default()
        },
        Ports {
            pager: 1,
            memory: 1,
            ..Ports::default()
        },
        Ports {
            pager: 1,
            index: 8,
            ..Ports::default()
        },
        Ports {
            memory: 1,
            read: 1,
            ..Ports::default()
        },
    ] {
        assert_eq!(h.execute_raw(p, false)?.status, Status::BadCommand);
    }
    Ok(())
}
#[test]
fn service_revalidation_and_coherent_reinstall() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    fixture(&mut h)?;
    assert_eq!(
        h.service(Service::WriteMemory {
            address: 16,
            value: Word::unsigned(999)
        })?
        .status,
        Status::BadCommand
    );
    assert_eq!(h.service(Service::Invalidate(r(1)))?.status, Status::Ok);
    assert_eq!(
        h.execute(Command::read(Read::Representation))?.status,
        Status::NotResident
    );
    h.install(r(1), r(9), 64, &[Word::signed(99)])?;
    assert_eq!(
        good(&mut h, Command::read(Read::Representation)),
        Word::signed(99)
    );
    assert_eq!(good(&mut h, Command::read(Read::Type)), r(9));
    // Same slot, different identity must not make a stale selected object accessible.
    assert_eq!(
        h.service(Service::Install(entry(17, 100, 1)))?.status,
        Status::Ok
    );
    good(&mut h, Command::index(1)?);
    assert_eq!(
        h.execute(Command::read_field())?.status,
        Status::NotResident
    );
    good(&mut h, Command::probe(Word::signed(4)));
    assert_eq!(
        h.service(Service::CompactClass {
            code: 2,
            class: r(13)
        })?
        .status,
        Status::Ok
    );
    assert_eq!(good(&mut h, Command::read(Read::Type)), r(13));
    assert_eq!(
        h.service_with_error(
            Service::WriteMemory {
                address: 200,
                value: Word::NIL
            },
            true
        )?
        .status,
        Status::MemoryError
    );
    assert_eq!(
        h.service(Service::CompactClass {
            code: 4,
            class: r(8)
        })?
        .status,
        Status::BadValue
    );
    assert_eq!(
        h.service(Service::Install(Entry {
            reference: Word::NIL,
            ..entry(5, 80, 1)
        }))?
        .status,
        Status::InvalidReference
    );
    h.reset()?;
    assert_eq!(
        h.execute(Command::read_field())?.status,
        Status::NoSelection
    );
    assert_eq!(h.execute(Command::probe(r(1)))?.status, Status::NotResident);
    assert_eq!(
        h.execute(Command::probe(Word::signed(1)))?.status,
        Status::BadValue
    );
    Ok(())
}
#[test]
fn metadata_chains_and_signed_index_registers() -> Result<()> {
    let runtime = runtime()?;
    let mut h = Harness::new(&runtime, Timing::default(), None)?;
    fixture(&mut h)?;
    h.install(r(8), r(9), 80, &[r(3)])?;
    assert_eq!(
        good(
            &mut h,
            Command {
                pager: Pager::ProbeType,
                ..Command::default()
            }
        ),
        r(8)
    );
    assert_eq!(
        good(
            &mut h,
            Command {
                pager: Pager::ProbeRepresentation,
                ..Command::default()
            }
        ),
        r(3)
    );
    assert_eq!(good(&mut h, Command::read(Read::Size)), Word::raw(2)?);
    assert_eq!(good(&mut h, Command::read(Read::Base)), Word::raw(32)?);
    for (value, op) in [
        (INDEX_MAX, Register::Increment),
        (INDEX_MIN, Register::Decrement),
    ] {
        good(
            &mut h,
            Command {
                register: Register::Load,
                data: Word::index(value)?,
                ..Command::default()
            },
        );
        let before = h.oracle.state.clone();
        assert_eq!(
            h.execute(Command {
                register: op,
                index: Index::Clear,
                load_vr: true,
                ..Command::default()
            })?
            .status,
            Status::IndexOverflow
        );
        assert_eq!(h.oracle.state, before);
        good(
            &mut h,
            Command {
                index: Index::FromReg,
                ..Command::default()
            },
        );
        assert_eq!(
            good(&mut h, Command::read(Read::Index)),
            Word::index(value)?
        );
        let delta = if value > 0 { 1 } else { -1 };
        assert_eq!(
            h.execute(Command {
                index: Index::Step,
                data: Word::index(delta)?,
                ..Command::default()
            })?
            .status,
            Status::IndexOverflow
        );
    }
    good(&mut h, Command::index(-4)?);
    good(
        &mut h,
        Command {
            index: Index::Step,
            data: Word::index(7)?,
            ..Command::default()
        },
    );
    assert_eq!(good(&mut h, Command::read(Read::Index)), Word::index(3)?);
    // The retained reference remains inspectable after invalidation, but its metadata does not.
    assert_eq!(h.service(Service::Invalidate(r(3)))?.status, Status::Ok);
    assert_eq!(good(&mut h, Command::read(Read::Reference)), r(3));
    assert_eq!(
        h.execute(Command {
            pager: Pager::ProbeType,
            ..Command::default()
        })?
        .status,
        Status::NotResident
    );
    let before = h.oracle.state.clone();
    assert_eq!(
        h.execute_raw(
            Ports {
                check_type: 1,
                expected_type: Word::NIL.bits(),
                ..Ports::default()
            },
            false
        )?
        .status,
        Status::BadValue
    );
    assert_eq!(h.oracle.state, before);
    Ok(())
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn randomized_horizontal_commands(steps in prop::collection::vec((any::<u8>(),any::<u8>(),any::<u32>(),0u32..5,0u32..6,0u32..5),1..120)) {
        let runtime=runtime().unwrap();let mut h=Harness::new(&runtime,Timing::default(),None).unwrap();fixture(&mut h).unwrap();
        for (op,arg,value,request_delay,memory_latency,response_stall) in steps {
            h.timing=Timing{request_delay,memory_latency,response_stall};
            let data=match arg%6 {0=>r(1+(value as u64%4)),1=>Word::signed(value as i32),2=>Word::index(value as i32 as i64).unwrap(),3=>Word::index((value%8) as i64-2).unwrap(),4=>r(17),_=>Word::NIL};
            let mut c=match op%10 {
                0=>Command::probe(data),1=>Command::index((arg%9) as i64-2).unwrap(),
                2=>Command::read_field(),3=>Command::write_field(data),
                4=>Command{index:Index::try_from(arg%10).unwrap(),data,..Command::default()},
                5=>Command{register:Register::try_from(arg%5).unwrap(),data,..Command::default()},
                6=>Command::read(Read::try_from(arg%10).unwrap()),
                7=>Command{pager:Pager::try_from(arg%5).unwrap(),vr:arg%8,data,..Command::default()},
                8=>Command{index:Index::Increment,memory:Memory::Read,..Command::default()},
                _=>Command{register:Register::FromIndex,index:Index::Load,read:Read::Index,data,..Command::default()},
            };
            if arg&16!=0 {c.load_vr=true;c.vr=arg%8;}
            if arg&32!=0 {c.expected_type=Some(r(8+(arg as u64%4)));}
            h.execute_raw(c.encode().unwrap(),op&128!=0).unwrap();
        }
    }

    #[test]
    fn randomized_resident_replacement(body in prop::collection::vec(any::<u32>(),1..17),base in 0u32..400,
        flags in 0u8..8,scan in any::<bool>(),delay in 0u32..8) {
        let runtime=runtime().unwrap();let mut h=Harness::new(&runtime,Timing{request_delay:delay,memory_latency:delay,response_stall:delay},None).unwrap();
        let reference=Word::reference(1,scan).unwrap();
        let values:Vec<_>=body.iter().map(|v|Word::signed(*v as i32)).collect();
        h.install(reference,r(8),base,&values).unwrap();
        good(&mut h,Command::probe(reference));
        let e=h.oracle.state.selected.unwrap();
        let e=Entry{new:flags&1!=0,modified:flags&2!=0,cond:flags&4!=0,..e};
        assert_eq!(h.service(Service::Install(e)).unwrap().status,Status::Ok);
        assert_eq!(good(&mut h,Command::read(Read::Flags)).bits(),flags as u64);
        for (i,value) in values.iter().enumerate() {
            good(&mut h,Command::index(i as i64+1).unwrap());
            assert_eq!(good(&mut h,Command::read_field()),*value);
            good(&mut h,Command::write_field(Word::unsigned(!body[i])));
        }
        assert_eq!(good(&mut h,Command::read(Read::Flags)).bits(),(flags|2) as u64);
        let collision=Entry{reference:Word::reference(17,scan).unwrap(),..h.oracle.state.selected.unwrap()};
        assert_eq!(h.service(Service::Install(collision)).unwrap().status,Status::Ok);
        assert_eq!(h.execute(Command::read_field()).unwrap().status,Status::NotResident);
        assert_eq!(h.service(Service::Invalidate(reference)).unwrap().status,Status::NotResident);
        good(&mut h,Command::probe(collision.reference));
        good(&mut h,Command::index(1).unwrap());
        assert_eq!(good(&mut h,Command::read_field()),Word::unsigned(!body[0]));
    }
}
