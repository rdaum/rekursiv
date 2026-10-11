//! Small portable workloads for comparison with a native Smalltalk VM.
//! Guest methods below encode the source in scripts/smalltalk-workload.st.
#[allow(dead_code)]
#[path = "../tests/squeak/fixture.rs"]
mod fixture;
use eyre::{ensure, Result};
use fixture::{int, Fixture};
use rekursiv_asm::Word;
use rekursiv_emulator::boot;
use std::time::Instant;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let case = args.get(1).map_or("fib", String::as_str);
    let repetitions: usize = args.get(2).map_or(Ok(3), |s| s.parse())?;
    ensure!((1..=1000).contains(&repetitions));
    let mut f = Fixture::new();
    let selector = f.selector("work:");
    let (bytes, literals, temps, n, expected) = match case {
        // ^ n < 2 ifTrue: [1] ifFalse: [(self work: n - 1) + (self work: n - 2)]
        "fib" => (
            vec![
                16, 119, 178, 153, 118, 124, 112, 16, 118, 177, 224, 112, 16, 119, 177, 224, 176,
                124,
            ],
            vec![selector],
            1,
            18,
            4181,
        ),
        // | sum i | sum := 0. i := 0.
        // [i < n] whileTrue: [sum := sum + i. i := i + 1]. ^ sum
        "loop" => (
            vec![
                117, 105, 117, 106, 18, 16, 178, 172, 10, 17, 18, 176, 105, 18, 118, 176, 106, 163,
                241, 17, 124,
            ],
            vec![],
            3,
            10000,
            49995000,
        ),
        _ => eyre::bail!("usage: smalltalk-workload fib|loop [repetitions] [eager]"),
    };
    let method = f.method(&bytes, literals, 1, temps, 0);
    f.bind(f.class, selector, method);
    let mut caller = Vec::new();
    for i in 0..repetitions {
        caller.extend([112, 32, 225]); // self work: literal n
        if i + 1 != repetitions {
            caller.push(135);
        }
    }
    caller.push(124);
    f.start(&caller, vec![int(n), selector], vec![]);
    let mut m = boot::squeak_source(&f.image, 16_777_216, 65_536)?.machine;
    let compilation = Instant::now();
    let eager = args.get(3).is_some_and(|s| s == "eager");
    ensure!(
        args.len() <= 4 && (args.len() < 4 || eager),
        "expected optional 'eager'"
    );
    if eager {
        m.enable_jit_eager()?;
    } else {
        m.enable_jit()?;
    }
    let startup_seconds = compilation.elapsed().as_secs_f64();
    eprintln!(
        "JIT startup: {startup_seconds:.3} s ({})",
        if eager { "eager" } else { "on demand" }
    );
    let compiled_before = m.jit_statistics().unwrap().compilation_time;
    let start = Instant::now();
    m.run_steps(repetitions as u64 * 200_000_000)?;
    let elapsed = start.elapsed().as_secs_f64();
    let jit = m.jit_statistics().unwrap();
    let runtime_compilation = (jit.compilation_time - compiled_before).as_secs_f64();
    let execution = (elapsed - runtime_compilation).max(0.0);
    eprintln!("JIT execution: {jit:?}");
    ensure!(
        m.cpu.halted && !m.cpu.service && m.cpu.rf[15] == 1,
        "did not finish: pc={} status={}",
        m.cpu.pc,
        m.cpu.rf[15]
    );
    ensure!(
        m.objekt.state.vr[5] == Word::signed(expected),
        "wrong result: {:?}",
        m.objekt.state.vr[5]
    );
    println!("case={case}, n={n}, repetitions={repetitions}, result={expected}, seconds={elapsed:.9}, seconds_per_call={:.9}, startup_seconds={startup_seconds:.9}, runtime_compilation_seconds={runtime_compilation:.9}, execution_seconds={execution:.9}, execution_seconds_per_call={:.9}, bytecodes={}, microinstructions={}, collections={}", elapsed / repetitions as f64, execution / repetitions as f64, m.stats.guest_bytecodes, m.stats.retired+m.stats.collector_retired,m.stats.collections);
    Ok(())
}
