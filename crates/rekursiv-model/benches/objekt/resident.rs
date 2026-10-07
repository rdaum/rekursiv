//! Hot resident commands and lookup working sets. One operation is one command
//! (or one direct lookup/validation in the explicitly named component cases).
use super::fixtures;
use micromeasure::{black_box, BenchContext, BenchmarkRunner, NoContext, Throughput};
use rekursiv_asm::{Command, Index, Read, Word};
use rekursiv_model::Model;

struct Resident {
    model: Model,
    command: Command,
}
impl BenchContext for Resident {
    fn prepare(_: usize) -> Self {
        Self {
            model: fixtures::resident(256),
            command: Command::read_field(),
        }
    }
}

fn response(ctx: &mut Resident, n: usize, _: usize) {
    let mut errors = 0;
    for _ in 0..n {
        let reply = ctx.model.execute_response(black_box(ctx.command), false);
        errors |= reply.status as u8;
        black_box(reply.data);
    }
    assert_eq!(errors, 0);
    black_box(&ctx.model);
}
fn observed(ctx: &mut Resident, n: usize, _: usize) {
    let mut errors = 0;
    for _ in 0..n {
        let outcome = ctx.model.execute(black_box(ctx.command), false);
        errors |= outcome.response.status as u8;
        // Drop inside the timed region: this includes allocation and destruction
        // of the access record used by metrics and RTL comparisons.
        black_box(outcome);
    }
    assert_eq!(errors, 0);
    black_box(&ctx.model);
}
fn validate(_: &mut NoContext, n: usize, _: usize) {
    for _ in 0..n {
        black_box(black_box(Command::read_field()).validate().unwrap());
    }
}

fn check(_: &mut NoContext, n: usize, _: usize) {
    for _ in 0..n {
        black_box(Command::read_field()).check().unwrap();
    }
}

struct Lookup {
    model: Model,
    references: Vec<Word>,
}
impl Lookup {
    fn new(objects: usize) -> Self {
        Self {
            model: fixtures::resident(objects),
            references: (1..=objects).map(fixtures::reference).collect(),
        }
    }
}
impl BenchContext for Lookup {
    fn prepare(_: usize) -> Self {
        Self::new(256)
    }
}
fn lookup(ctx: &mut Lookup, n: usize, _: usize) {
    let mask = ctx.references.len() - 1;
    for i in 0..n {
        // Odd stride permutes a power-of-two working set instead of repeatedly
        // reading one hot entry. Include index and key-array access in the cost.
        let key = black_box(ctx.references[i.wrapping_mul(40503) & mask]);
        black_box(ctx.model.resolve(key).unwrap());
    }
}

pub fn register(runner: &BenchmarkRunner) {
    runner.group::<NoContext>("OBJEKT components", |g| {
        g.bench("component/validate_field_read", validate);
        g.bench("component/check_field_read", check);
    });
    runner.group::<Lookup>("OBJEKT lookup", |g| {
        for (name, objects) in [
            ("lookup/resident_256", 256),
            ("lookup/resident_65536", 65536),
        ] {
            g.factory(&|| Lookup::new(objects)).bench(name, lookup);
        }
        g.factory(&|| {
            let mut ctx = Lookup::new(1);
            ctx.references[0] = Word::signed(17);
            ctx
        })
        .bench("lookup/compact_integer", lookup);
    });
    runner.group::<Resident>("OBJEKT resident commands", |g| {
        let cases = [
            ("resident/noop", Command::default()),
            ("resident/read_vr", Command::read(Read::Vr)),
            (
                "resident/load_vr",
                Command {
                    load_vr: true,
                    data: Word::signed(17),
                    ..Command::default()
                },
            ),
            (
                "resident/index_load",
                Command {
                    index: Index::Load,
                    data: Word::index(2).unwrap(),
                    ..Command::default()
                },
            ),
            ("resident/read_type", Command::read(Read::Type)),
            (
                "resident/prepare_field",
                Command {
                    prepare: true,
                    ..Command::default()
                },
            ),
            ("resident/read_field", Command::read_field()),
            (
                "resident/read_prepared",
                Command {
                    prepared: true,
                    ..Command::read_field()
                },
            ),
            (
                "resident/write_field",
                Command::write_field(Word::signed(29)),
            ),
            ("resident/fetch_hit", Command::fetch(fixtures::reference(1))),
            ("resident/probe_hit", Command::probe(fixtures::reference(1))),
        ];
        for (name, command) in cases {
            g.throughput(Throughput::per_operation(1, "commands"))
                .factory(&|| Resident {
                    command,
                    ..Resident::prepare(0)
                })
                .bench(name, response);
        }
        g.factory(&|| {
            let mut ctx = Resident::prepare(0);
            ctx.model.state.index = 1;
            ctx
        })
        .bench("resident/read_cached_first_word", response);
        g.bench("observed/read_field_with_access_log", observed);
    });
}
