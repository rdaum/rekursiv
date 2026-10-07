//! Command-boundary equivalence, simultaneous operands, and atomic failures.
use crate::*;
use rekursiv_asm::*;
fn setup() -> Model {
    let mut m = Model::new(4, 32);
    let e = Entry {
        reference: Word::reference(1, true).unwrap(),
        class: Word::reference(2, true).unwrap(),
        base: 4,
        size: 2,
        representation: Word::signed(17),
        new: false,
        modified: false,
        cond: false,
    };
    m.memory[4] = e.representation;
    m.memory[5] = Word::signed(18);
    assert_eq!(
        m.service(Service::Install(e), false).response.status,
        Status::Ok
    );
    m.execute(Command::probe(e.reference), false);
    m
}
#[test]
fn native_and_wire_commands_match_effects_and_complete_state() {
    let reference = |id| Word::reference(id, true).unwrap();
    // Two pager slots force dirty eviction and refill, in addition to the
    // resident, allocation, exchange, and directory command paths.
    let mut model = Model::new(2, 64);
    let mut commands = vec![
        Command::allocate(reference(100), 3, true).unwrap(),
        Command::index(1).unwrap(),
        Command::write_field(Word::ZERO),
        Command::index(2).unwrap(),
        Command::write_field(Word::signed(17)),
        Command {
            load_vr: true,
            vr: 7,
            data: reference(1),
            ..Command::default()
        },
        Command::allocate(reference(100), 3, true).unwrap(),
        Command::index(1).unwrap(),
        Command::write_field(Word::ZERO),
        Command::index(2).unwrap(),
        Command::write_field(Word::signed(18)),
        Command::allocate(reference(100), 3, true).unwrap(),
        Command::fetch(reference(1)),
        Command {
            pager: Pager::Exchange,
            data: reference(2),
            vr: 7,
            ..Command::default()
        },
        Command {
            pager: Pager::NextObject,
            ..Command::default()
        },
        Command {
            pager: Pager::FindObject,
            data: reference(1),
            ..Command::default()
        },
        Command::fetch(reference(1)),
        Command::index(2).unwrap(),
        Command::read_field(),
    ];
    for read in [
        Read::Vr,
        Read::Reference,
        Read::Size,
        Read::Type,
        Read::Base,
        Read::Representation,
        Read::Index,
        Read::IndexReg,
        Read::Flags,
        Read::FreeWords,
        Read::FreeIdentities,
    ] {
        commands.push(Command::read(read));
    }
    for command in commands {
        for memory_error in [true, false] {
            let mut native = model.clone();
            let mut wire = model.clone();
            let mut unrecorded = model.clone();
            let response = unrecorded.execute_response(command, memory_error);
            let actual = native.execute(command, memory_error);
            let expected = wire.execute_raw(command.encode().unwrap(), memory_error);
            assert_eq!(actual, expected, "{command:?}, fault={memory_error}");
            assert_eq!(native, wire, "{command:?}, fault={memory_error}");
            assert_eq!(
                response, actual.response,
                "{command:?}, fault={memory_error}"
            );
            assert_eq!(unrecorded, native, "{command:?}, fault={memory_error}");
            if !memory_error {
                assert_eq!(actual.response.status, Status::Ok, "{command:?}");
                model = native;
            }
        }
    }
    assert!(!model.store.records.is_empty());
}

#[test]
fn command_boundaries_preserve_validation_and_maintenance_error_priority() {
    for command in [
        Command {
            vr: 8,
            ..Command::default()
        },
        Command {
            expected_type: Some(Word::ZERO),
            ..Command::default()
        },
        Command {
            pager: Pager::Allocate,
            ..Command::default()
        },
        Command {
            alloc_size: ADDRESS_LIMIT,
            ..Command::default()
        },
    ] {
        let expected = command.validate().unwrap_err();
        for maintenance in [false, true] {
            let mut model = setup();
            model.maintenance = maintenance;
            let before = model.clone();
            assert_eq!(model.execute(command, true), Outcome::error(expected));
            assert_eq!(
                model.execute_response(command, true),
                Response::error(expected)
            );
            assert_eq!(model, before);
            // The public transfer entry point must still validate itself.
            assert_eq!(
                model.execute_transfer(command, Faults::default()),
                Outcome::error(expected)
            );
            assert_eq!(model, before);
        }
    }
    let mut model = setup();
    model.maintenance = true;
    let before = model.clone();
    let command = Command::read_field();
    assert_eq!(
        model.execute(command, false),
        Outcome::error(Status::BadCommand)
    );
    assert_eq!(
        model.execute_raw(command.encode().unwrap(), false),
        Outcome::error(Status::BadCommand)
    );
    // A malformed data word normally reports BadValue; maintenance wins at
    // the wire boundary, before any decode or side effect.
    let ports = Ports {
        data: 1 << 40,
        ..Ports::default()
    };
    assert_eq!(
        model.execute_raw(ports, false),
        Outcome::error(Status::BadCommand)
    );
    assert_eq!(
        model.execute_response(command, false),
        Response::error(Status::BadCommand)
    );
    assert_eq!(model, before);
    model.maintenance = false;
    assert_eq!(
        model.execute_raw(ports, false),
        Outcome::error(Status::BadValue)
    );
}

#[test]
fn old_state_and_atomic_failure() {
    let mut m = setup();
    m.execute(Command::index(1).unwrap(), false);
    let c = Command {
        index: Index::Increment,
        memory: Memory::Read,
        ..Command::default()
    };
    assert_eq!(m.execute(c, false).response.data, Word::signed(17));
    assert_eq!(m.state.index, 2);
    let before = m.state.clone();
    let c = Command {
        index: Index::Increment,
        load_vr: true,
        data: Word::signed(99),
        memory: Memory::Write,
        ..Command::default()
    };
    assert_eq!(m.execute(c, true).response.status, Status::MemoryError);
    assert_eq!(m.state, before);
    assert_eq!(m.memory[5], Word::signed(18));
}
#[test]
fn revalidation_and_compact_types() {
    let mut m = setup();
    let r = m.state.selected.unwrap().reference;
    m.service(Service::Invalidate(r), false);
    assert_eq!(
        m.execute(Command::read(Read::Size), false).response.status,
        Status::NotResident
    );
    assert_eq!(m.state.selected.unwrap().reference, r);
    assert_eq!(
        m.execute(Command::probe(Word::signed(-8)), false)
            .response
            .status,
        Status::BadValue
    );
    m.service(
        Service::CompactClass {
            code: 2,
            class: Word::reference(2, true).unwrap(),
        },
        false,
    );
    assert_eq!(
        m.execute(Command::probe(Word::signed(-8)), false)
            .response
            .status,
        Status::Ok
    );
    assert_eq!(m.state.selected.unwrap().representation.bits(), 0xfffffff8);
}

#[test]
fn combined_prepared_write_stages_registers_and_forwards_only_matching_selection() {
    for select_written_object in [false, true] {
        for observed in [false, true] {
            let mut model = setup();
            let a = model.state.selected.unwrap();
            let b = Entry {
                reference: Word::reference(3, true).unwrap(),
                base: 8,
                representation: Word::signed(41),
                ..a
            };
            model.memory[8] = b.representation;
            model.memory[9] = Word::signed(42);
            assert_eq!(
                model.service(Service::Install(b), false).response.status,
                Status::Ok
            );
            let selected = if select_written_object { a } else { b };
            model.state.vr[7] = selected.reference;
            model.state.index_reg = 5;
            assert_eq!(
                model
                    .execute_response(
                        Command {
                            index: Index::One,
                            prepare: true,
                            ..Command::default()
                        },
                        false
                    )
                    .status,
                Status::Ok
            );
            let before = model.clone();
            let command = Command {
                pager: Pager::ProbeVr,
                vr: 7,
                load_vr: true,
                data: Word::signed(99),
                index: Index::Increment,
                register: Register::FromIndex,
                prepare: true,
                prepared: true,
                memory: Memory::Write,
                expected_type: Some(a.class),
                ..Command::default()
            };
            let execute = |model: &mut Model, fault| {
                if observed {
                    model.execute(command, fault).response
                } else {
                    model.execute_response(command, fault)
                }
            };
            // The RAM fault comes after preparation, selection, and register
            // calculations. None may escape the command's failure boundary.
            assert_eq!(execute(&mut model, true).status, Status::MemoryError);
            assert_eq!(model, before);
            assert_eq!(execute(&mut model, false), Response::ok(Word::signed(99)));
            assert_eq!(model.state.index, 2);
            assert_eq!(model.state.index_reg, 1); // old index, not incremented index
            assert_eq!(model.state.vr[7], Word::signed(99));
            assert_eq!(&model.state.vr[..7], &before.state.vr[..7]);
            // New preparation uses old selection A and new index two. The RAM
            // write consumes the previous preparation of A's first word.
            assert_eq!(
                model.state.prepared,
                PreparedAccess {
                    reference: a.reference,
                    index: 2,
                    address: 5,
                    status: Status::Ok,
                }
            );
            assert_eq!(model.memory[4], Word::signed(99));
            assert_eq!(model.memory[5], before.memory[5]);
            assert_eq!(&model.memory[8..10], &before.memory[8..10]);
            let written = model.resolve(a.reference).unwrap();
            assert!(written.modified);
            assert_eq!(written.representation, Word::signed(99));
            // ProbeVr consumes the old VR. Forward write metadata if it selected
            // A, but never replace a simultaneous selection of B with A.
            assert_eq!(
                model.state.selected,
                Some(if select_written_object { written } else { b })
            );
        }
    }
}
