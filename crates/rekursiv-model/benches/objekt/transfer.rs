//! Bounded allocation and refill batches. Each sample starts with fresh state.
//! One operation is one allocate/fetch, except dirty refill: one write+fetch pair.
use super::fixtures::{class, install, reference, PAGER_ENTRIES};
use micromeasure::{black_box, BenchContext, BenchmarkRunner, Throughput};
use rekursiv_asm::{Command, Word};
use rekursiv_model::{store::Record, Model};

// Bound identities and RAM even during calibration. The largest body uses
// about 64 MiB of RAM; no measured loop resets a cursor or exhausts capacity.
const COMMANDS: usize = 32_768;
struct Transfers<const WORDS: usize> {
    model: Model,
    keys: [Word; 2],
}
impl<const WORDS: usize> BenchContext for Transfers<WORDS> {
    fn prepare(n: usize) -> Self {
        assert_eq!(n, COMMANDS);
        let mut model = Model::new(PAGER_ENTRIES, (n + 1) * WORDS);
        // Touch RAM before measurement so body initialization/refill does not
        // include demand-zero page faults from fixture allocation.
        black_box(&mut model.memory).fill(Word::raw(73).unwrap());
        Self {
            model,
            keys: [reference(1), reference(1 + PAGER_ENTRIES)],
        }
    }
    fn chunk_size() -> Option<usize> {
        Some(COMMANDS)
    }
}
impl<const WORDS: usize> Transfers<WORDS> {
    fn refill() -> Self {
        let mut ctx = Self::prepare(COMMANDS);
        for key in ctx.keys {
            ctx.model.store.records.insert(
                key.identity().unwrap(),
                Record {
                    reference: key,
                    class: class(),
                    cond: false,
                    body: vec![Word::raw(73).unwrap(); WORDS],
                },
            );
        }
        // Both references map to the same slot. Start with B resident, then
        // alternate A/B: every measured fetch misses and evicts its predecessor.
        install(&mut ctx.model, ctx.keys[1], 0, WORDS);
        ctx.model.state.selected = ctx.model.entries[1];
        ctx.model.state.index = 2;
        ctx
    }
}
fn allocate<const WORDS: usize>(ctx: &mut Transfers<WORDS>, n: usize, _: usize) {
    let command = Command::allocate(class(), WORDS as u32, false).unwrap();
    let mut errors = 0;
    for _ in 0..n {
        let reply = ctx.model.execute_response(black_box(command), false);
        errors |= reply.status as u8;
        black_box(reply.data);
    }
    assert_eq!(errors, 0);
    assert_eq!(ctx.model.body_cursor as usize, n * WORDS);
    assert_eq!(ctx.model.next_identity, n as u64 + 1);
    assert!(ctx.model.store.records.is_empty()); // no hidden eviction work
    black_box(&ctx.model);
}
fn refill<const WORDS: usize, const DIRTY: bool>(ctx: &mut Transfers<WORDS>, n: usize, _: usize) {
    let mut errors = 0;
    for i in 0..n {
        if DIRTY {
            let write = Command::write_field(Word::signed(29));
            let reply = ctx.model.execute_response(black_box(write), false);
            errors |= reply.status as u8;
            black_box(reply.data);
        }
        let command = Command::fetch(ctx.keys[i & 1]);
        let reply = ctx.model.execute_response(black_box(command), false);
        errors |= reply.status as u8;
        black_box(reply.data);
    }
    assert_eq!(errors, 0);
    assert_eq!(ctx.model.body_cursor as usize, (n + 1) * WORDS); // every access refilled
    assert_eq!(
        ctx.model.state.selected.unwrap().reference,
        ctx.keys[(n - 1) & 1]
    );
    assert_eq!(ctx.model.store.records.len(), 2);
    black_box(&ctx.model);
}

pub fn register(runner: &BenchmarkRunner) {
    runner.group::<Transfers<16>>("OBJEKT transfers, 16 words", |g| {
        g.bench("transfer/allocate_16_words", allocate::<16>);
        g.factory(&Transfers::<16>::refill)
            .bench("transfer/clean_refill_16_words", refill::<16, false>);
        g.throughput(Throughput::per_operation(1, "pairs"))
            .factory(&Transfers::<16>::refill)
            .bench(
                "transfer/write_and_dirty_refill_16_words",
                refill::<16, true>,
            );
    });
    runner.group::<Transfers<256>>("OBJEKT transfers, 256 words", |g| {
        g.bench("transfer/allocate_256_words", allocate::<256>);
        g.factory(&Transfers::<256>::refill)
            .bench("transfer/clean_refill_256_words", refill::<256, false>);
    });
}
