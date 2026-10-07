use eyre::Result;
use rekursiv_asm::Word;
use rekursiv_emulator::{boot, Step};
use rekursiv_model::store::Record;

#[test]
fn full_pager_collects_last_slot_and_preserves_dirty_collision_victim() -> Result<()> {
    let mut loaded = boot::microcode(
        "d=0xa00000ffff, page=Fetch
         idx=One
         d=73, mem=Write
         gc=Collect
         d=0xa00001ffff, page=Fetch
         d=0xa00000ffff, page=Fetch
         mem=Read
         halt",
        512,
    )?;
    let m = &mut loaded.machine;
    m.objekt_metrics_enabled = true;
    assert_eq!(m.objekt.entries.len(), 65_536);
    for id in [65_535, 131_071] {
        m.objekt.store.records.insert(
            id,
            Record {
                reference: Word::reference(id, true)?,
                class: Word::reference(100, true)?,
                cond: false,
                body: vec![Word::ZERO; 3],
            },
        );
    }
    for _ in 0..10_000_000 {
        if m.step()? == Step::Halted {
            break;
        }
    }
    assert!(m.cpu.halted, "collector failed to finish a full pager pass");
    assert_eq!(m.cpu.object, 73);
    assert_eq!(m.stats.collections, 1);
    assert_eq!(m.stats.objekt.gc_writes, 3);
    assert_eq!(m.stats.objekt.fetch.empty_misses, 1);
    assert_eq!(m.stats.objekt.fetch.collision_misses, 2);
    assert_eq!(m.stats.objekt.evicted_modified, 1);
    assert_eq!(m.stats.objekt.evicted_clean, 1);
    assert_eq!(m.objekt.store.records[&65_535].body[0], Word::raw(73)?);
    assert_eq!(
        m.objekt.entries[65_535].unwrap().reference,
        Word::reference(65_535, true)?
    );
    Ok(())
}

#[test]
fn pager_configuration_rejects_invalid_sizes_and_binds_assembly_constant() -> Result<()> {
    for entries in [0, 1, 3, 65_537, 131_072] {
        assert!(boot::microcode_with_pager("halt", 512, entries).is_err());
    }
    for entries in [2, 16, 1024, 65_536] {
        let loaded =
            boot::microcode_with_pager("d=PAGER_ENTRIES, r=Bus, rb=0, ldrb\nhalt", 512, entries)?;
        let mut m = loaded.machine;
        assert_eq!(m.objekt.entries.len(), entries);
        m.step()?;
        assert_eq!(m.cpu.rf[0] as usize, entries);
    }
    Ok(())
}
