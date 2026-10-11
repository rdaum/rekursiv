//! Observe microcode work at a fixed guest-dispatch budget. No guest semantics
//! are implemented here. Run with: dispatch-profile smalltalk|squeak IMAGE COUNT.
use eyre::{ensure, Result};
use rekursiv_emulator::{boot, presentation, Step};
use std::collections::BTreeMap;

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 4,
        "usage: dispatch-profile smalltalk|squeak IMAGE BYTECODES"
    );
    let bytes = std::fs::read(&args[2])?;
    let mut loaded = match args[1].as_str() {
        "smalltalk" => boot::smalltalk(&bytes, 16_777_216)?,
        "squeak" => boot::squeak(&bytes, 16_777_216)?,
        _ => eyre::bail!("unknown guest"),
    };
    let count: u64 = args[3].parse()?;
    ensure!(count > 0, "BYTECODES must be positive");
    let m = &mut loaded.machine;
    m.devices = presentation::workstation(0, 1000);
    if args[1] == "squeak" {
        m.devices.display_bitmap = Some(rekursiv_devices::Bitmap::new(1024, 768));
        let path = std::path::Path::new(&args[2]);
        let mut files = rekursiv_devices::Files::new(b"C:\\", b"C:\\Squeak1.1.image", b'\\');
        for name in ["Squeak1.1.image", "SqueakV1.sources", "Squeak1.1.changes"] {
            files.contents.insert(
                format!("/{name}").into_bytes(),
                std::fs::read(path.parent().unwrap().join(name))?,
            );
        }
        m.devices.files = Some(files);
    }
    let mut counts = vec![0u64; m.image().code.len()];
    let mut opcodes = [0u64; 256];
    let mut primitives = BTreeMap::<u32, u64>::new();
    let decoded = loaded.symbols["decoded"] as u16;
    let primitive = loaded.symbols["primitive_dispatch"] as u16;
    while m.stats.guest_bytecodes < count {
        let pc = m.cpu.pc;
        let recovering = m.recovering();
        if !recovering && pc == decoded {
            opcodes[m.cpu.rf[0] as usize] += 1;
        }
        if !recovering && pc == primitive {
            *primitives.entry(m.cpu.rf[0]).or_default() += 1;
        }
        ensure!(!m.cpu.halted && !m.cpu.service, "guest stopped at {pc}");
        if m.step()? == Step::Retired && !recovering {
            counts[pc as usize] += 1;
        }
    }
    let mut labels: Vec<_> = loaded
        .symbols
        .iter()
        .filter(|(name, pc)| {
            name.chars().any(|c| c.is_ascii_lowercase())
                && **pc >= 0
                && (**pc as usize) < counts.len()
                && m.image().code[**pc as usize].is_some()
        })
        .map(|(name, pc)| (*pc as usize, name))
        .collect();
    labels.sort();
    let mut regions = Vec::new();
    for (i, (start, name)) in labels.iter().enumerate() {
        let end = labels.get(i + 1).map_or(counts.len(), |(pc, _)| *pc);
        regions.push((counts[*start..end].iter().sum::<u64>(), name));
    }
    regions.sort_by(|a, b| b.0.cmp(&a.0));
    println!(
        "bytecodes={} instructions={} instructions/bytecode={:.2}",
        m.stats.guest_bytecodes,
        m.stats.retired,
        m.stats.retired as f64 / m.stats.guest_bytecodes as f64
    );
    for (n, name) in regions.into_iter().filter(|(n, _)| *n > 0).take(60) {
        println!(
            "{n:12} {:6.2}% {name}",
            n as f64 * 100.0 / m.stats.retired as f64
        );
    }
    println!(
        "opcodes: {:?}",
        opcodes
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .collect::<Vec<_>>()
    );
    println!("primitives: {primitives:?}");
    Ok(())
}
