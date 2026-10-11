//! Small model/native comparisons: compile one command at a time, never an image VM.
use eyre::Result;
use rekursiv_asm::{
    processor::{Bus, Instruction},
    Command, Entry, Index, Memory, Pager, Read, Register, Status, Word, INDEX_MAX, INDEX_MIN,
};
use rekursiv_emulator::Machine;
use rekursiv_model::{processor::Image, PreparedAccess};

#[test]
fn resident_datapath_preserves_lookup_flags_bounds_and_old_operands() -> Result<()> {
    let class = Word::reference(100, true)?;
    let a = Word::reference(1, true)?;
    let b = Word::reference(2, false)?;
    let mut commands = Vec::new();
    for pager in [
        Pager::None,
        Pager::ProbeBus,
        Pager::ProbeVr,
        Pager::ProbeType,
        Pager::ProbeRepresentation,
    ] {
        for read in [
            Read::None,
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
            commands.push(Command {
                pager,
                read,
                index: Index::Increment,
                register: Register::FromIndex,
                prepare: true,
                load_vr: true,
                vr: 3,
                ..Command::default()
            });
        }
    }
    commands.push(Command {
        pager: Pager::Fetch,
        ..Command::default()
    });
    for memory in [Memory::Read, Memory::Write] {
        for prepared in [false, true] {
            for prepare in [false, true] {
                for index in [Index::None, Index::Load, Index::Next, Index::FromReg] {
                    commands.push(Command {
                        memory,
                        prepared,
                        prepare,
                        index,
                        register: Register::Increment,
                        expected_type: Some(class),
                        load_vr: true,
                        vr: 3,
                        ..Command::default()
                    });
                }
            }
        }
        for pager in [
            Pager::ProbeBus,
            Pager::ProbeVr,
            Pager::ProbeType,
            Pager::ProbeRepresentation,
        ] {
            commands.push(Command {
                memory,
                pager,
                prepared: true,
                prepare: true,
                read: Read::None,
                index: Index::Step,
                load_vr: true,
                vr: 3,
                ..Command::default()
            });
        }
    }
    for (n, command) in commands.into_iter().enumerate() {
        command.check()?;
        let image = Image::program(&[
            Instruction {
                bus: Bus::Symbol,
                object: Some(command),
                ..Instruction::default()
            },
            Instruction::halt(),
        ]);
        let mut native = Machine::new(image.clone(), 0, 16, 64)?;
        native.enable_jit_eager()?;
        for case in 0..40 {
            let mut reference = Machine::new(image.clone(), 0, 16, 64)?;
            let mut first = Entry {
                reference: a,
                class,
                size: 2,
                base: 1,
                representation: b,
                new: case % 2 == 0,
                modified: case % 3 == 0,
                cond: case % 4 == 0,
            };
            let second = Entry {
                reference: b,
                class,
                size: 3,
                base: 4,
                representation: Word::raw(19)?,
                new: false,
                modified: false,
                cond: true,
            };
            if case == 6 {
                first.base = 63;
            }
            if case == 38 {
                first.class = Word::reference(101, true)?;
            }
            if case == 7 {
                first.size = 0;
            }
            reference.objekt.entries[1] = Some(first).into();
            reference.objekt.entries[2] = Some(second).into();
            reference.objekt.entries[4] = Some(Entry {
                reference: class,
                class,
                ..first
            })
            .into();
            reference.objekt.memory.fill(Word::raw(83)?); // first-word cache differs deliberately
            reference.objekt.classes.fill(Some(class).into());
            if case % 5 == 0 {
                reference.objekt.classes[2] = None.into();
            }
            reference.objekt.state.selected = match case % 8 {
                0 => None,
                1 => Some(Entry {
                    reference: Word::reference(17, true)?,
                    ..first
                }),
                2 => Some(Entry {
                    base: 99,
                    representation: Word::ZERO,
                    ..first
                }), // stale snapshot
                3 => Some(Entry {
                    reference: Word::signed(-3),
                    ..first
                }),
                _ => Some(first),
            }
            .into();
            let indices = [0, 1, 2, 3, -1, INDEX_MIN, INDEX_MAX, INDEX_MAX + 1];
            reference.objekt.state.index = indices[case % indices.len()];
            reference.objekt.state.index_reg = indices[(case + n) % indices.len()];
            reference.objekt.state.vr.fill(a);
            reference.objekt.state.prepared = PreparedAccess {
                reference: if case % 3 == 0 { b } else { a },
                index: if case % 4 == 0 { 1 } else { 2 },
                address: if case % 7 == 0 { 65 } else { 2 },
                status: if case % 6 == 0 {
                    Status::BoundsError
                } else {
                    Status::Ok
                },
            };
            reference.objekt.maintenance = case == 39;
            let inputs = [
                a,
                b,
                Word::raw(73)?,
                Word::NIL,
                Word::signed(-7),
                Word::from_bits((3 << 38) | (4 << 32))?,
                Word::from_bits((3 << 38) | (1 << 32) | 2)?,
                Word::reference(17, true)?,
                Word::index(-1)?,
                Word::reference(1, false)?,
            ];
            reference.cpu.symbol = inputs[(case + n) % inputs.len()].bits();
            native.cpu = reference.cpu.clone();
            native.objekt = reference.objekt.clone();
            native.stats = reference.stats;
            native.fault = None;
            let blocks = native.jit_statistics().unwrap().block_calls;
            let expected = reference.run_steps(1);
            let actual = native.run_steps(1);
            assert_eq!(
                actual.is_err(),
                expected.is_err(),
                "command {command:?}, case {case}"
            );
            assert_eq!(
                native.cpu, reference.cpu,
                "command {command:?}, case {case}"
            );
            assert_eq!(
                native.objekt, reference.objekt,
                "command {command:?}, case {case}"
            );
            assert_eq!(
                native.stats, reference.stats,
                "command {command:?}, case {case}"
            );
            assert_eq!(
                native.fault, reference.fault,
                "command {command:?}, case {case}"
            );
            assert_eq!(native.jit_statistics().unwrap().block_calls, blocks + 1);
        }
    }
    Ok(())
}
