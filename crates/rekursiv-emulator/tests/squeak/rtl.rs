//! Retirement and heap parity with the Verilog processor.
use super::*;

#[test]
fn microcode_retirements_and_object_state_match_rtl_through_allocation_and_gc() -> Result<()> {
    let mut f = Fixture::new();
    let class = f.pointers(f.class, vec![f.special[0], f.special[0], int((1 << 7) | 6)]);
    let selector = f.primitive(70, 0, &[122]);
    // Two sends allocate fresh objects. An unreachable filler below forces GC
    // on the first refill, without hundreds of redundant interpreter iterations.
    f.start(
        &[32, 209, 135, 32, 209, 135, 34, 124],
        vec![class, selector, int(100)],
        vec![],
    );
    let m = compare_rtl(&f, true)?;
    assert_eq!(m.objekt.state.vr[5], Word::signed(100));
    assert!(m.stats.collections > 0);
    Ok(())
}

/// Compare every retirement, including all object commands, under stalls.
fn compare_rtl(f: &Fixture, force_gc: bool) -> Result<Machine> {
    compare_rtl_with_memory(f, force_gc, 1024)
}

fn compare_rtl_with_memory(f: &Fixture, force_gc: bool, memory_words: usize) -> Result<Machine> {
    use rekursiv_asm::Service;
    use rekursiv_model::processor::Processor;
    use rekursiv_sim::{Harness, Timing};
    let mut loaded = boot::squeak_source(&f.image, memory_words, 16)?;
    let m = &mut loaded.machine;
    let filler = rekursiv_asm::Entry {
        reference: Word::reference(32000, false)?,
        class: target::reference(f.special[0])?,
        size: 2,
        base: 510,
        representation: Word::ZERO,
        new: false,
        modified: false,
        cond: false,
    };
    if force_gc {
        m.objekt.service(Service::Install(filler), false);
    }
    let runtime = rekursiv_sim::runtime_with_memory(memory_words)?;
    let mut h = Harness::new(
        &runtime,
        Timing {
            request_delay: 2,
            memory_latency: 3,
            response_stall: 2,
        },
        None,
    )?;
    h.store = m.objekt.store.clone();
    h.oracle.store = m.objekt.store.clone();
    let context = target::reference(f.context)?;
    let record = m.objekt.store.records[&context.identity()?].clone();
    h.install(context, record.class, 0, &record.body)?;
    if force_gc {
        h.service(Service::Install(filler))?;
    }
    h.service(Service::ReserveIdentities(m.objekt.next_identity))?;
    h.service(Service::CompactClass {
        code: 2,
        class: target::reference(f.special[5])?,
    })?;
    let image = m.image().clone();
    h.load_processor(&image)?;
    h.start_processor(m.cpu.pc)?;
    let mut oracle = Processor::default();
    let start_cycles = h.stats.cycles;
    h.run_processor_observed(&image, &mut oracle, 20_000_000, |h, expected| {
        let mut complete = false;
        for _ in 0..1_000_000 {
            if m.step()? == Step::Retired && !m.recovering() {
                complete = true;
                break;
            }
        }
        ensure!(complete, "native retirement timeout");
        ensure!(m.cpu == *expected, "CPU mismatch at {}", expected.pc);
        ensure!(m.objekt == h.oracle, "OBJEKT mismatch at {}", expected.pc);
        Ok(())
    })?;
    assert_eq!(m.cpu.rf[15], 1);
    assert_eq!(m.stats.collections, h.stats.collections);
    let cycles = h.stats.cycles - start_cycles;
    let instructions = m.stats.retired + m.stats.collector_retired;
    eprintln!(
        "RTL: {instructions} microinstructions ({} collector), {cycles} cycles, {:.2} cycles/instruction, {:.2} M instructions/s at 100 MHz; pager=16, RAM={memory_words}, request delay=2, memory latency=3, response stall=2, command gate=4/7 cycles",
        m.stats.collector_retired,
        cycles as f64 / instructions as f64,
        100.0 * instructions as f64 / cycles as f64,
    );
    Ok(loaded.machine)
}

#[test]
fn packed_bitblt_word_and_edge_paths_match_rtl_under_memory_stalls() -> Result<()> {
    let mut f = Fixture::new();
    let nil = f.special[0];
    let (dst, bits) = super::graphics::form(&mut f, 12, 1, 8, vec![0xaaaaaaaa; 3]);
    let (src, _) =
        super::graphics::form(&mut f, 12, 1, 8, vec![0x01020304, 0x05060708, 0x090a0b0c]);
    super::graphics::blit(
        &mut f,
        dst,
        src,
        nil,
        3,
        [1, 0, 10, 1, 1, 0, 0, 0, 12, 1],
        nil,
    );
    let m = compare_rtl(&f, false)?;
    assert_eq!(
        body(&m, target::reference(bits)?)?[2..],
        [
            Word::raw(0xaa020304)?,
            Word::raw(0x05060708)?,
            Word::raw(0x090a0baa)?
        ]
    );
    Ok(())
}

#[test]
fn mapped_glyph_words_and_edges_match_rtl_under_memory_stalls() -> Result<()> {
    let (f, bits, expected) =
        super::glyphs::fixture([1, 1, 9, 2, 30, 1, 0, 0, 37, 5], &[0x0ff00ff0, 0xf00ff00f]);
    let m = compare_rtl(&f, false)?;
    assert_eq!(
        body(&m, target::reference(bits)?)?[2..]
            .iter()
            .map(|w| w.bits() as u32)
            .collect::<Vec<_>>(),
        expected
    );
    Ok(())
}

#[test]
fn character_scanner_matches_rtl_through_refills_and_raster_calls() -> Result<()> {
    let sf = super::scanner::fixture(1, 3, 5, true, None);
    // Both character tables must fit in the resident semispace together.
    // These fixtures deliberately collide in the 16-slot pager. Their CPI
    // measures the stress test, not a desktop with the normal pager capacity.
    for memory_words in [4096, 32768] {
        let m = compare_rtl_with_memory(&sf.f, false, memory_words)?;
        assert_eq!(m.objekt.state.vr[5], Word::signed(9002));
        let fields = body(&m, target::reference(sf.scanner)?)?;
        assert_eq!(fields[6], Word::signed(4));
        assert_eq!(fields[17], Word::signed(2));
        if memory_words == 32768 {
            assert_eq!(m.stats.collections, 0);
        }
    }
    Ok(())
}
