use eyre::Result;
use rekursiv_asm::{Status, Word};
use rekursiv_emulator::{boot, Step};
use rekursiv_model::store::Record;

fn microcode(source: &str, memory_words: usize) -> Result<boot::Loaded> {
    boot::microcode_with_pager(source, memory_words, 16)
}

fn record(id: u64, size: usize) -> Record {
    Record {
        reference: Word::reference(id, true).unwrap(),
        class: Word::reference(100, true).unwrap(),
        cond: false,
        body: vec![Word::ZERO; size],
    }
}

#[test]
fn collisions_count_completed_victims_and_actual_word_traffic() -> Result<()> {
    let mut loaded = microcode(
        "d=0xa000000001, page=Fetch
         d=0xa000000001, page=Fetch
         idx=One
         mem=Read
         idx=Two
         mem=Read
         d=7, mem=Write
         d=0xa000000011, page=Fetch
         d=0xa000000001, page=Fetch
         d=0xa000000064, page=Allocate, size=4, scan=1
         d=0xa000000011, page=Fetch
         d=0xc20000002a, page=Fetch
         d=0xa000000011, page=ProbeBus
         halt",
        512,
    )?;
    let m = &mut loaded.machine;
    m.objekt_metrics_enabled = true;
    m.objekt.store.records.insert(1, record(1, 3));
    m.objekt.store.records.insert(17, record(17, 2));
    m.objekt.next_identity = 33;
    m.objekt.classes[2] = Some(Word::reference(100, true)?);
    while m.step()? != Step::Halted {}
    let s = m.stats.objekt;
    assert_eq!(
        (
            s.fetch.hits,
            s.fetch.empty_misses,
            s.fetch.collision_misses,
            s.fetch.compacts
        ),
        (1, 1, 3, 1)
    );
    assert_eq!(s.probe.hits, 1);
    assert_eq!(
        (s.evicted_clean, s.evicted_new, s.evicted_modified),
        (2, 1, 1)
    );
    assert_eq!(
        (s.allocations, s.allocation_attempts, s.allocated_words),
        (1, 1, 4)
    );
    assert_eq!((s.refills, s.refilled_words), (4, 10));
    assert_eq!(
        (s.field_reads, s.cached_field_reads, s.field_writes),
        (2, 1, 1)
    );
    assert_eq!((s.ram_reads, s.ram_writes), (8, 15));
    assert_eq!(
        (
            s.store_metadata,
            s.store_reads,
            s.store_writes,
            s.store_save_commits,
            s.store_requests
        ),
        (4, 10, 7, 2, 25)
    );
    assert_eq!((s.errors, s.out_of_space), (0, 0));
    assert_eq!(m.objekt.store.records[&1].body[1], Word::raw(7)?);
    assert_eq!(m.objekt.store.records[&33].body, vec![Word::NIL; 4]);
    Ok(())
}

#[test]
fn failed_misses_are_not_evictions_and_gc_retries_are_attempts() -> Result<()> {
    // Fill a semispace with an unreachable object; a colliding refill first
    // fails its reservation, then succeeds after GC removes the old mapping.
    let mut loaded = microcode(
        "d=0xa000000064, page=Allocate, size=250, scan=0
         d=0xa000000002, page=Fetch
         d=0xa000000011, page=Fetch
         gc=Collect
         halt",
        512,
    )?;
    let m = &mut loaded.machine;
    m.objekt_metrics_enabled = true;
    m.objekt.store.records.insert(2, record(2, 0));
    m.objekt.store.records.insert(17, record(17, 10));
    for _ in 0..500_000 {
        if m.step()? == Step::Halted {
            break;
        }
    }
    assert!(m.cpu.halted);
    let s = m.stats.objekt;
    assert_eq!((s.fetch.empty_misses, s.fetch.collision_misses), (2, 1));
    assert_eq!((s.refills, s.refilled_words), (2, 10));
    assert_eq!(
        (s.evicted_clean, s.evicted_new, s.evicted_modified),
        (0, 0, 0)
    );
    assert_eq!((s.gc_refill, s.gc_allocation, s.gc_explicit), (1, 0, 1));
    assert_eq!((s.out_of_space, s.errors), (1, 0));
    assert_eq!(s.gc_writes, 10);
    assert!(s.gc_reads >= s.gc_writes);
    assert_eq!(s.gc_reclaimed_words, 250);
    // Object 2 stays rooted by the control-store literal used to fetch it.
    assert_eq!(s.gc_discarded_entries, 1);
    assert_eq!(m.stats.collections, 2);

    let mut loaded = microcode("d=0xa000000001, page=Fetch", 512)?;
    let m = &mut loaded.machine;
    m.objekt_metrics_enabled = true;
    assert!(m.step().is_err());
    assert_eq!(m.fault.unwrap().status, Some(Status::InvalidReference));
    assert_eq!(m.stats.objekt.fetch.empty_misses, 1);
    assert_eq!(m.stats.objekt.refills, 0);
    assert_eq!(m.stats.objekt.errors, 1);
    Ok(())
}

#[test]
fn allocation_pressure_counts_retries_without_double_counting_objects() -> Result<()> {
    let mut loaded = microcode(include_str!("../../../microcode/collection.uc"), 512)?;
    let mut baseline = microcode(include_str!("../../../microcode/collection.uc"), 512)?;
    let m = &mut loaded.machine;
    m.objekt_metrics_enabled = true;
    for _ in 0..1_000_000 {
        let step = m.step()?;
        assert_eq!(baseline.machine.step()?, step);
        if step == Step::Halted {
            break;
        }
    }
    assert!(m.cpu.halted);
    let s = m.stats.objekt;
    assert_eq!(
        (s.allocation_attempts, s.allocations, s.allocated_words),
        (12, 7, 840)
    );
    assert_eq!(
        (s.out_of_space, s.gc_allocation, s.gc_refill, s.gc_explicit),
        (5, 5, 0, 0)
    );
    assert_eq!(s.gc_reclaimed_words, 600);
    assert_eq!(s.gc_discarded_entries, 5);
    assert_eq!(s.gc_writes, 600);
    assert_eq!(s.ram_writes, 840);
    assert_eq!(s.store_requests, 0);
    assert_eq!(m.stats.collections, 5);
    assert_eq!(m.cpu, baseline.machine.cpu);
    assert_eq!(m.objekt, baseline.machine.objekt);
    assert_eq!(baseline.machine.stats.objekt, Default::default());
    Ok(())
}
