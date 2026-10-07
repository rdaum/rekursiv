//! Shared object identities and resident fixtures. Setup issues service commands
//! before measurement; benchmark loops use the ordinary command entry points.
use rekursiv_asm::{Command, Entry, Service, Status, Word};
use rekursiv_model::Model;

pub const PAGER_ENTRIES: usize = 65_536;
pub const RESIDENT_WORDS: usize = 16;

pub fn reference(id: usize) -> Word {
    Word::reference(id as u64, false).unwrap()
}

pub fn class() -> Word {
    reference(1_000_000)
}

/// Install a body whose first word agrees with its cached representation.
pub fn install(model: &mut Model, reference: Word, base: usize, words: usize) {
    model.memory[base..base + words].fill(Word::raw(73).unwrap());
    let entry = Entry {
        reference,
        class: class(),
        base: base as u32,
        size: words as u32,
        representation: model.memory[base],
        new: false,
        modified: false,
        cond: false,
    };
    assert_eq!(
        model
            .service(Service::Install(entry), false)
            .response
            .status,
        Status::Ok
    );
}

/// Populate consecutive, noncolliding pager slots and select field two.
pub fn resident(objects: usize) -> Model {
    assert!(objects.is_power_of_two() && objects <= PAGER_ENTRIES);
    let mut model = Model::new(PAGER_ENTRIES, objects * RESIDENT_WORDS);
    for n in 0..objects {
        install(
            &mut model,
            reference(n + 1),
            n * RESIDENT_WORDS,
            RESIDENT_WORDS,
        );
    }
    model.classes[2] = Some(class());
    assert_eq!(
        model
            .execute_response(Command::fetch(reference(1)), false)
            .status,
        Status::Ok
    );
    model.state.index = 2;
    assert_eq!(
        model
            .execute_response(
                Command {
                    prepare: true,
                    ..Command::default()
                },
                false
            )
            .status,
        Status::Ok
    );
    model
}
