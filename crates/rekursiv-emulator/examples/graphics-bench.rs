//! Deterministic Squeak graphics workloads executed entirely by microcode.
//! The scalar pixel oracle below verifies results; it never supplies guest work.
#[allow(dead_code)]
#[path = "../tests/squeak/fixture.rs"]
mod fixture;
use eyre::{bail, ensure, Result};
use fixture::{int, Fixture};
use rekursiv_emulator::{boot, graphics::GraphicsCounters, timing::Config};
use rekursiv_smalltalk::squeak::{image::Body, target};
use std::time::Instant;

const WIDTH: usize = 64;
const HEIGHT: usize = 32;
fn form(f: &mut Fixture, depth: usize, words: Vec<u32>) -> (u32, u32) {
    let bits = f.object(f.special[4], Body::Words(words), 6);
    let origin = f.pointers(f.special[12], vec![int(0), int(0)]);
    (
        f.pointers(
            f.class,
            vec![
                bits,
                int(WIDTH as i32),
                int(HEIGHT as i32),
                int(depth as i32),
                origin,
            ],
        ),
        bits,
    )
}
fn pixel(words: &[u32], x: usize, y: usize, depth: usize) -> u32 {
    let per_word = 32 / depth;
    (words[y * (WIDTH / per_word) + x / per_word] >> (32 - depth * (x % per_word + 1)))
        & ((1 << depth) - 1)
}
fn put(words: &mut [u32], x: usize, y: usize, byte: u32) {
    let shift = 24 - (x % 4) * 8;
    let word = &mut words[y * (WIDTH / 4) + x / 4];
    *word = (*word & !(255 << shift)) | (byte << shift);
}
fn workload(case: &str, iterations: usize) -> (Fixture, u32, Vec<u32>) {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let initial: Vec<u32> = (0..WIDTH * HEIGHT / 4)
        .map(|n| 0x936c5aa5u32.rotate_left(n as u32))
        .collect();
    let (dst, bits) = form(&mut f, 8, initial.clone());
    let depth = if case == "glyph" { 1 } else { 8 };
    let sources: Vec<Vec<u32>> = [0xa35c87e1u32, 0x5ca3781e]
        .into_iter()
        .map(|seed| {
            (0..WIDTH * HEIGHT * depth / 32)
                .map(|n| seed.rotate_left((n * 7) as u32))
                .collect()
        })
        .collect();
    let source_forms: Vec<_> = sources
        .iter()
        .map(|words| form(&mut f, depth, words.clone()).0)
        .collect();
    let colours = [[0x27, 0xd2], [0x93, 0x4e]];
    let halves = [0x275ba1d2u32, 0xe8934e1b];
    let (dx, dy, sx, sy, w, h) = match case {
        "unaligned" => (3, 2, 1, 1, 59, 28),
        "scroll" => (1, 1, 0, 0, 63, 31),
        "glyph" => (1, 1, 0, 0, 62, 30),
        _ => (0, 0, 0, 0, WIDTH, HEIGHT),
    };
    let bb_class = f.pointers(f.class, vec![nil, nil, int(2)]);
    let draw = f.selector("draw");
    let method = f.method(&[122], vec![], 0, 0, 96);
    f.bind(bb_class, draw, method);
    let mut blits = Vec::new();
    for n in 0..2 {
        let src = match case {
            "fill" => nil,
            "scroll" => dst,
            _ => source_forms[n],
        };
        let half = if case == "fill" {
            f.object(f.special[4], Body::Words(vec![halves[n]]), 6)
        } else {
            nil
        };
        let map = if case == "glyph" {
            f.object(f.special[4], Body::Words(colours[n].to_vec()), 6)
        } else {
            nil
        };
        let mut fields = vec![dst, src, half, int(3)];
        fields.extend([dx, dy, w, h, sx, sy, 0, 0, WIDTH, HEIGHT].map(|v| int(v as i32)));
        fields.push(map);
        blits.push(f.pointers(bb_class, fields));
    }
    let register = f.primitive(102, 0, &[122]);
    // Register the display, then alternate two operands so copy/fill/glyph work
    // does not become a sequence of unchanged-word skips after the first draw.
    let mut bytes = vec![35, 212, 135];
    for n in 0..iterations {
        bytes.extend([32 + (n % 2) as u8, 210, 135]);
    }
    bytes.push(120);
    f.start(
        &bytes,
        vec![blits[0], blits[1], draw, dst, register],
        vec![],
    );
    let mut expected = initial;
    for n in 0..iterations {
        let source = if case == "scroll" {
            expected.clone()
        } else {
            sources[n % 2].clone()
        };
        for y in 0..h {
            for x in 0..w {
                let value = match case {
                    "fill" => (halves[n % 2] >> (24 - ((dx + x) % 4) * 8)) & 255,
                    "glyph" => colours[n % 2][pixel(&source, sx + x, sy + y, 1) as usize],
                    _ => pixel(&source, sx + x, sy + y, 8),
                };
                put(&mut expected, dx + x, dy + y, value);
            }
        }
    }
    (f, bits, expected)
}
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut iterations = 8usize;
    let mut selected = "all".to_owned();
    let mut engine = "jit".to_owned();
    let mut profile = "none".to_owned();
    let mut clock = 100.0f64;
    let mut pager_entries = boot::DEFAULT_PAGER_ENTRIES;
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!("graphics-bench [--case all|fill|copy|unaligned|scroll|glyph] [--iterations N] [--engine jit|interpreter] [--estimate-cycles sram|dram] [--clock-mhz N] [--pager-entries N]\nCSV on stdout. Image loading/JIT compilation excluded; guest registration, sends, drawing, publication and final return included. Estimation adds host overhead. Scroll cost changes as the repeated scroll fills the image, so compare identical iteration counts.");
            return Ok(());
        }
        let value = args
            .next()
            .ok_or_else(|| eyre::eyre!("missing value for {arg}"))?;
        match arg.as_str() {
            "--case" => selected = value,
            "--iterations" => iterations = value.parse()?,
            "--engine" => engine = value,
            "--estimate-cycles" => profile = value,
            "--clock-mhz" => clock = value.parse()?,
            "--pager-entries" => pager_entries = value.parse()?,
            _ => bail!("unknown option {arg}"),
        }
    }
    let cases = ["fill", "copy", "unaligned", "scroll", "glyph"];
    ensure!(
        selected == "all" || cases.contains(&selected.as_str()),
        "unknown case"
    );
    ensure!(
        (1..=1000).contains(&iterations),
        "iterations must be 1..1000"
    );
    ensure!(engine == "jit" || engine == "interpreter", "unknown engine");
    let config = match profile.as_str() {
        "none" => None,
        "sram" => Some(Config::SRAM),
        "dram" => Some(Config::DRAM),
        _ => bail!("unknown cycle profile"),
    };
    ensure!(
        clock.is_finite() && clock > 0.0,
        "clock must be positive and finite"
    );
    println!("case,operations,engine,cycle_profile,pager_entries,seconds,operations_per_second,microinstructions_per_operation,object_commands_per_operation,display_updates,published_words,cycles_per_operation,estimated_operations_per_second,clock_mhz");
    for case in cases
        .into_iter()
        .filter(|c| selected == "all" || *c == selected)
    {
        let (f, bits, expected) = workload(case, iterations);
        let mut m = boot::squeak_source(&f.image, 131072, pager_entries)?.machine;
        m.devices = rekursiv_devices::Device::workstation(0, 1000);
        if let Some(config) = config {
            m.enable_cycle_estimate(config)?;
        }
        if engine == "jit" {
            m.enable_jit()?;
        }
        let started = Instant::now();
        m.run_steps(2_000_000 + iterations as u64 * 20_000_000)?;
        let elapsed = started.elapsed().as_secs_f64();
        ensure!(
            m.cpu.halted && !m.cpu.service && m.cpu.rf[15] == 1,
            "{case}: guest did not return successfully: PC={}, status={}, mutator={}, collector={}",
            m.cpu.pc,
            m.cpu.rf[15],
            m.stats.retired,
            m.stats.collector_retired
        );
        let reference = target::reference(bits)?;
        let actual = if let Ok(entry) = m.objekt.resolve(reference) {
            &m.objekt.memory[entry.base as usize..entry.base as usize + entry.size as usize]
        } else {
            &m.objekt.store.records[&reference.identity()?].body
        };
        ensure!(
            actual[2..]
                .iter()
                .map(|w| w.bits() as u32)
                .eq(expected.iter().copied()),
            "{case}: guest pixels differ from oracle"
        );
        ensure!(
            m.devices
                .display_bitmap
                .as_ref()
                .unwrap()
                .visible
                .as_ref()
                .unwrap()
                .words
                == expected,
            "{case}: published pixels differ from oracle"
        );
        let graphics = GraphicsCounters::capture(&m.devices);
        ensure!(
            graphics.display.updates == iterations as u64 + 1 && graphics.cursor.updates == 0,
            "{case}: publication count mismatch"
        );
        let ops = iterations as f64;
        let cycles = m.cycle_estimate().map(|e| e.cycles().total() as f64 / ops);
        println!("{case},{iterations},{engine},{profile},{pager_entries},{elapsed:.6},{:.3},{:.3},{:.3},{},{},{},{},{clock}",
            ops / elapsed, (m.stats.retired + m.stats.collector_retired) as f64 / ops, m.stats.object_commands as f64 / ops,
            graphics.display.updates, graphics.display.words, cycles.map_or(String::new(), |c| format!("{c:.3}")),
            cycles.map_or(String::new(), |c| format!("{:.3}", clock * 1e6 / c)));
    }
    Ok(())
}
