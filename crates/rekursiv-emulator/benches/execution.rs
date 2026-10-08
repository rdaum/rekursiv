//! CPU dispatch, JIT blocks, and device-tick costs using standalone microcode.
//! Setup and compilation are outside measurement. One operation is 256 retired
//! microinstructions; these loops never halt, fault, allocate, or collect.
use micromeasure::{benchmark_main, black_box, BenchContext, BenchmarkRuntimeOptions, Throughput};
use rekursiv_emulator::{boot, presentation, Machine};
use std::time::Duration;

const LOCAL: &str = include_str!("../../../microcode/bench/local.uc");
const RESIDENT: &str = include_str!("../../../microcode/bench/resident.uc");
const BATCH: u64 = 256;

struct Cpu(Machine);
impl Cpu {
    fn new(source: &str, jit: bool, peripherals: bool) -> Self {
        let mut loaded = boot::microcode(source, 131072).unwrap();
        if peripherals {
            loaded.machine.devices = presentation::workstation(0, 1000);
        }
        while loaded.machine.cpu.pc != loaded.symbols["loop"] as u16 {
            loaded.machine.step().unwrap();
        }
        if jit {
            loaded.machine.enable_jit().unwrap();
        }
        Self(loaded.machine)
    }
}
impl BenchContext for Cpu {
    fn prepare(_: usize) -> Self {
        Self::new(LOCAL, true, false)
    }
}

fn execute(cpu: &mut Cpu, n: usize, _: usize) {
    let instructions = n as u64 * BATCH;
    let before = cpu.0.stats.retired;
    assert_eq!(cpu.0.run_steps(instructions).unwrap(), instructions);
    assert_eq!(cpu.0.stats.retired - before, instructions);
    assert!(!cpu.0.cpu.halted && !cpu.0.cpu.service && !cpu.0.recovering());
    black_box(&cpu.0);
}

benchmark_main!(|runner| {
    runner.set_runtime(BenchmarkRuntimeOptions {
        warm_up_duration: Duration::from_millis(200),
        benchmark_duration: Duration::from_secs(1),
        min_samples: 20,
        max_samples: 128,
    });
    runner.group::<Cpu>("Microcode execution", |group| {
        for (name, source, jit, peripherals) in [
            ("local/jit", LOCAL, true, false),
            ("local/jit_workstation", LOCAL, true, true),
            ("local/interpreter", LOCAL, false, false),
            ("resident/jit", RESIDENT, true, false),
            ("resident/jit_workstation", RESIDENT, true, true),
            ("resident/interpreter", RESIDENT, false, false),
        ] {
            group
                .throughput(Throughput::per_operation(BATCH, "microinstructions"))
                .factory(&|| Cpu::new(source, jit, peripherals))
                .bench(name, execute);
        }
    });
});
