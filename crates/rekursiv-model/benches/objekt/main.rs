//! Standalone OBJEKT command measurements, independent of CPU dispatch and JIT.
//! Run with `cargo bench -p rekursiv-model --bench objekt -- <name-filter>`.
//! Fixture construction is outside the timed region; each case checks its result.
mod fixtures;
mod resident;
mod transfer;

use micromeasure::{benchmark_main, BenchmarkRuntimeOptions};
use std::time::Duration;

benchmark_main!(|runner| {
    runner.set_runtime(BenchmarkRuntimeOptions {
        warm_up_duration: Duration::from_millis(200),
        benchmark_duration: Duration::from_secs(1),
        min_samples: 20,
        max_samples: 512,
    });
    resident::register(runner);
    transfer::register(runner);
});
