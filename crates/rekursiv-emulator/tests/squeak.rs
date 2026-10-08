//! Squeak guest semantics exercised by the machine, without a host VM.
#[path = "squeak/fixture.rs"]
mod fixture;
use eyre::{ensure, Result};
use fixture::{int, Fixture};
use rekursiv_asm::Word;
use rekursiv_emulator::{boot, Machine, Step};
use rekursiv_smalltalk::squeak::{image::Body, target};

fn run(f: &Fixture, jit: bool) -> Result<Machine> {
    run_with(f, jit, |_| {})
}
fn run_with(f: &Fixture, jit: bool, configure: impl FnOnce(&mut Machine)) -> Result<Machine> {
    let mut loaded = boot::squeak_source(&f.image, 131072, 16)?;
    configure(&mut loaded.machine);
    if jit {
        loaded.machine.enable_jit()?;
    }
    for _ in 0..10000 {
        loaded.machine.run_steps(256)?;
        if loaded.machine.cpu.halted || loaded.machine.cpu.service {
            break;
        }
    }
    ensure!(
        loaded.machine.cpu.halted && loaded.machine.cpu.rf[15] == 1,
        "guest stopped PC={} status={} R0={} IP={} opcode={}",
        loaded.machine.cpu.pc,
        loaded.machine.cpu.rf[15],
        loaded.machine.cpu.rf[0],
        loaded.machine.cpu.rf[8],
        loaded.machine.cpu.rf[13]
    );
    Ok(loaded.machine)
}
fn result(f: &Fixture, expected: Word) -> Result<()> {
    for jit in [false, true] {
        assert_eq!(run(f, jit)?.objekt.state.vr[5], expected);
    }
    Ok(())
}
fn body(m: &Machine, reference: Word) -> Result<Vec<Word>> {
    if let Ok(entry) = m.objekt.resolve(reference) {
        Ok(
            m.objekt.memory[entry.base as usize..entry.base as usize + entry.size as usize]
                .to_vec(),
        )
    } else {
        Ok(m.objekt
            .store
            .records
            .get(&reference.identity()?)
            .ok_or_else(|| eyre::eyre!("missing object"))?
            .body
            .clone())
    }
}

#[path = "squeak/bytecodes.rs"]
mod bytecodes;
#[path = "squeak/primitives.rs"]
mod primitives;
#[path = "squeak/rtl.rs"]
mod rtl;
#[path = "squeak/startup.rs"]
mod startup;

#[path = "squeak/floats.rs"]
mod floats;

#[path = "squeak/graphics.rs"]
mod graphics;

#[path = "squeak/glyphs.rs"]
mod glyphs;

#[path = "squeak/devices.rs"]
mod devices;

#[path = "squeak/collections.rs"]
mod collections;

#[path = "squeak/scanner.rs"]
mod scanner;

#[path = "squeak/scanner_archive.rs"]
mod scanner_archive;
