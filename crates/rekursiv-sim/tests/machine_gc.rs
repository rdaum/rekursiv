//! These tests only clock the machine and emulate RAM/backing devices after
//! startup. Expected heaps are computed independently and never fed to RTL.
use eyre::{ensure, Result};
use rekursiv_asm::{processor::*, Command, Service, Word};
use rekursiv_model::processor::{Image, Processor, CODE_WORDS};
use rekursiv_sim::{runtime, Harness, Timing};
fn object(c: Command) -> Instruction {
    Instruction {
        data: c.data,
        object: Some(c),
        ..Instruction::default()
    }
}
fn reg(rb: u8, value: u32) -> Instruction {
    Instruction {
        rb,
        data: Word::unsigned(value),
        r: Source::Bus,
        write_register: true,
        ..Instruction::default()
    }
}
fn image(code: &[Instruction]) -> Result<Image> {
    Ok(Image::program(code).with_ram_collector(128, 16)?)
}
fn finish(h: &mut Harness<'_>) -> Result<usize> {
    let services = h.stats.services;
    let mut collections = 0;
    let mut active = false;
    // Each collection scans two embedded roots per control-store word.
    for cycle in 0..200_000 * CODE_WORDS.div_ceil(1024) {
        if h.rtl.cpu_halted_o != 0 {
            ensure!(
                h.stats.services == services,
                "host maintenance during execution"
            );
            return Ok(collections);
        }
        h.rtl.cpu_command_enable_i = (cycle % 7 > 2) as u8;
        h.rtl.cpu_response_enable_i = (cycle % 5 > 1) as u8;
        h.tick()?;
        if h.rtl.cpu_gc_active_o != 0 && !active {
            collections += 1;
        }
        active = h.rtl.cpu_gc_active_o != 0;
    }
    eyre::bail!("timeout pc={} fault={}", h.rtl.cpu_pc_o, h.rtl.cpu_fault_o)
}
#[test]
fn allocation_enters_microcode_copies_and_retries_with_context_preserved() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 2,
            memory_latency: 3,
            response_stall: 2,
        },
        None,
    )?;
    let class = Word::reference(100, true)?;
    // Full source space: only the first object is rooted; the large second
    // body is discarded. Both records remain on the backing device.
    let root = Word::reference(1, true)?;
    h.install(root, class, 0, &[Word::signed(73), root])?;
    h.install(Word::reference(2, true)?, class, 2, &vec![Word::NIL; 254])?;
    let alloc = Command::allocate(class, 8, true)?;
    let code = [
        reg(7, 0x12345678),
        Instruction {
            symbol: true,
            data: root,
            ..Instruction::default()
        },
        Instruction {
            load_q: true,
            flags: true,
            r: Source::Branch,
            branch: 29,
            ..Instruction::default()
        },
        Instruction {
            bus: Bus::Symbol,
            estk: Estk::Bus,
            ..Instruction::default()
        },
        object(alloc),
        Instruction::halt(),
    ];
    let image = image(&code)?;
    h.load_processor(&image)?;
    let mut cpu = Processor::default();
    for _ in 0..4 {
        cpu = cpu.prepare(&image, false).unwrap().0;
    }
    h.oracle.collect_ram(&[root, class], false, 8)?;
    let result = h.oracle.execute(alloc, false).response;
    cpu = cpu.prepare(&image, false).unwrap().0;
    cpu.object = result.data.bits();
    cpu = cpu.prepare(&image, false).unwrap().0;
    h.start_processor(0)?;
    assert_eq!(finish(&mut h)?, 1);
    assert_eq!(
        h.rtl.cpu_fault_o, 0,
        "status {} pc {}",
        h.rtl.cpu_last_status_o, h.rtl.cpu_pc_o
    );
    h.compare_state()?;
    h.compare_processor(&cpu)?;
    assert_eq!(h.stats.commands, 2); // failed allocation, then exact retry
    assert_eq!(h.stats.store_transactions, 0); // no fetches during tracing
    Ok(())
}
#[test]
fn successive_collections_alternate_spaces_and_preserve_dynamic_allocation_size() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let class = Word::reference(100, true)?;
    let alloc = Command::allocate(class, 0, false)?;
    let code = [
        reg(9, 120),
        reg(8, 7),
        Instruction {
            allocation_dynamic: true,
            ra: 9,
            ..object(alloc)
        },
        Instruction {
            alu: Alu::Sub,
            ra: 8,
            rb: 8,
            s: Source::Branch,
            branch: 1,
            carry: Carry::One,
            write_register: true,
            flags: true,
            ..Instruction::default()
        },
        Instruction {
            seq: Seq::ConditionalJump,
            condition: Condition::Zero,
            invert: true,
            branch: 2,
            ..Instruction::default()
        },
        Instruction::halt(),
    ];
    let image = image(&code)?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    assert_eq!(finish(&mut h)?, 5);
    assert_eq!(h.rtl.cpu_fault_o, 0);
    h.rtl.cpu_dbg_addr_i = 9;
    h.rtl.eval();
    assert_eq!(h.rtl.cpu_dbg_rf_o, 120);
    h.rtl.cpu_dbg_addr_i = 8;
    h.rtl.eval();
    assert_eq!(h.rtl.cpu_dbg_rf_o, 0);
    assert_eq!(h.rtl.dbg_next_identity_o, 8);
    assert_eq!(h.rtl.dbg_body_cursor_o, 496);
    Ok(())
}
#[test]
fn dirty_persistent_parent_retains_new_descendants_but_opaque_payload_does_not() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let class = Word::reference(100, true)?;
    let parent = Word::reference(1, true)?;
    let child = Word::reference(2, true)?;
    let opaque = Word::reference(3, false)?;
    let dead = Word::reference(4, true)?;
    h.install(parent, class, 0, &[child])?;
    h.install(child, class, 1, &[parent])?;
    h.install(opaque, class, 2, &[dead])?;
    h.install(dead, class, 3, &[dead])?;
    h.install(Word::reference(5, true)?, class, 4, &vec![Word::NIL; 252])?;
    let mut e = h.oracle.resolve(parent)?;
    e.modified = true;
    e.cond = true;
    h.service(Service::Install(e))?;
    let mut e = h.oracle.resolve(child)?;
    e.new = true;
    h.store.records.remove(&2);
    h.oracle.store.records.remove(&2);
    h.service(Service::Install(e))?;
    let alloc = Command::allocate(class, 4, true)?;
    let image = image(&[
        object(alloc),
        Instruction::halt(),
        Instruction::literal(opaque),
    ])?;
    h.load_processor(&image)?;
    h.oracle.collect_ram(&[class, opaque], false, 4)?;
    h.oracle.execute(alloc, false);
    h.start_processor(0)?;
    assert_eq!(finish(&mut h)?, 1);
    assert_eq!(h.rtl.cpu_fault_o, 0);
    h.compare_state()?;
    assert!(h.oracle.resolve(child).is_ok());
    assert!(h.oracle.resolve(dead).is_err());
    assert!(h.store.records.contains_key(&4));
    assert_eq!(h.stats.store_transactions, 0);
    Ok(())
}

#[test]
fn every_copy_failure_preserves_source_pager_and_allows_restart() -> Result<()> {
    let rt = runtime()?;
    // Two tracing reads and four copy transfers. Inject an error at each
    // request, including after a successful destination write.
    for fault_at in 0..6 {
        let mut h = Harness::new(
            &rt,
            Timing {
                request_delay: 2,
                memory_latency: 4,
                response_stall: 1,
            },
            None,
        )?;
        let class = Word::reference(100, true)?;
        let root = Word::reference(1, true)?;
        h.install(root, class, 0, &[root, Word::signed(42)])?;
        h.install(Word::reference(2, true)?, class, 2, &vec![Word::NIL; 254])?;
        let allocation = Command::allocate(class, 8, true)?;
        let image = image(&[
            object(allocation),
            Instruction::halt(),
            Instruction::literal(root),
        ])?;
        h.load_processor(&image)?;
        let source = h.backing[..256].to_vec();
        let requests = h.transfers.len();
        let services = h.stats.services;
        h.start_processor(0)?;
        let mut injected = false;
        for _ in 0..40_000 * CODE_WORDS.div_ceil(1024) {
            if h.rtl.cpu_halted_o != 0 {
                break;
            }
            if !injected && h.rtl.cpu_gc_active_o != 0 && h.transfers.len() == requests + fault_at {
                h.fail_next_memory = true;
                injected = true;
            }
            h.tick()?;
        }
        assert!(injected);
        assert_eq!(h.rtl.cpu_fault_o, 4);
        assert_eq!(h.rtl.cpu_last_status_o, 9);
        assert_eq!(h.rtl.cpu_pc_o, 0);
        assert_eq!(h.rtl.cpu_gc_active_o, 0);
        assert_eq!(h.backing[..256], source);
        assert_eq!(h.stats.services, services);
        // Failed destination writes may leave bytes, but no published mapping
        // may refer to them. Compare every other architectural field unchanged.
        h.oracle.memory[256..].copy_from_slice(&h.backing[256..]);
        h.compare_state()?;
        h.oracle.collect_ram(&[root, class], false, 8)?;
        h.oracle.execute(allocation, false);
        h.start_processor(0)?;
        assert_eq!(finish(&mut h)?, 1);
        assert_eq!(h.rtl.cpu_fault_o, 0);
        h.compare_state()?;
    }
    Ok(())
}

#[test]
fn live_set_exhaustion_does_not_publish_or_retry_forever() -> Result<()> {
    let rt = runtime()?;
    for size in [1, 257] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let class = Word::reference(100, true)?;
        let root = Word::reference(1, false)?;
        h.install(root, class, 0, &vec![Word::ZERO; 256])?;
        let image = image(&[
            object(Command::allocate(class, size, false)?),
            Instruction::halt(),
            Instruction::literal(root),
        ])?;
        h.load_processor(&image)?;
        h.start_processor(0)?;
        assert_eq!(finish(&mut h)?, 1);
        assert_eq!(h.rtl.cpu_fault_o, 4);
        assert_eq!(h.rtl.cpu_last_status_o, 11);
        assert_eq!(h.rtl.cpu_pc_o, 0);
        assert_eq!(h.stats.commands, 1);
        assert_eq!(h.rtl.dbg_body_cursor_o, 256);
        h.oracle.memory[256..].copy_from_slice(&h.backing[256..]);
        h.compare_state()?;
    }
    Ok(())
}

#[test]
fn each_machine_root_path_retains_an_object_without_host_enumeration() -> Result<()> {
    let rt = runtime()?;
    for kind in [
        "symbol", "cache", "stack", "result", "vr", "selected", "class", "compact", "literal",
        "expected", "pending", "extra",
    ] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let class = Word::reference(100, true)?;
        let root = Word::reference(1, true)?;
        h.install(root, class, 0, &[Word::signed(42)])?;
        h.install(Word::reference(2, true)?, class, 1, &vec![Word::NIL; 255])?;
        // Execute initial state setup, then erase its embedded reference from
        // the control store. Thus a literal cannot accidentally mask a missing
        // machine root path in the cases that test registers or stacks.
        let mut setup = vec![Instruction {
            data: root,
            symbol: kind == "symbol",
            estk: if kind == "cache" || kind == "stack" {
                Estk::Bus
            } else {
                Estk::Hold
            },
            ..Instruction::default()
        }];
        if kind == "stack" {
            setup.extend([
                Instruction {
                    sp: Pointer::Increment,
                    esp: Address::Sp,
                    ..Instruction::default()
                },
                Instruction {
                    estk: Estk::Bus,
                    ..Instruction::default()
                },
            ]);
        }
        if kind == "cache" {
            setup.insert(
                0,
                Instruction {
                    sp: Pointer::Increment,
                    esp: Address::Sp,
                    ..Instruction::default()
                },
            );
            setup.push(Instruction {
                sp: Pointer::Bus,
                data: Word::ZERO,
                ..Instruction::default()
            });
        }
        if kind == "result" {
            setup[0] = object(Command::probe(root));
        }
        setup.push(Instruction::halt());
        let setup_image = Image::program(&setup);
        h.load_processor(&setup_image)?;
        h.start_processor(0)?;
        h.run_processor(&setup_image, &mut Processor::default(), 1000)?;
        // Clear selection for the last-response test; this doesn't touch LOGIK's cache.
        if kind == "result" {
            h.install(class, class, 0, &[])?;
            h.execute(Command::probe(class))?;
        }
        for address in 0..setup.len() {
            for (lane, data) in Instruction::default().encode()?.into_iter().enumerate() {
                h.program_lane(0, address, lane, data)?;
            }
        }
        match kind {
            "vr" => {
                h.execute(Command {
                    data: root,
                    load_vr: true,
                    vr: 7,
                    ..Command::default()
                })?;
            }
            "selected" => {
                h.execute(Command::probe(root))?;
            }
            "class" => {
                let holder = Word::reference(3, true)?;
                h.install(holder, root, 0, &[])?;
                h.execute(Command::probe(holder))?;
            }
            "compact" => {
                h.service(Service::CompactClass {
                    code: 3,
                    class: root,
                })?;
            }
            _ => (),
        }
        let allocation = Command::allocate(if kind == "pending" { root } else { class }, 8, true)?;
        let mut image = image(&[object(allocation), Instruction::halt()])?;
        if kind == "literal" {
            image.code[60] = Some(Instruction::literal(root));
        }
        if kind == "expected" {
            image.code[60] = Some(object(Command {
                expected_type: Some(root),
                ..Command::default()
            }));
        }
        if kind == "extra" {
            image.roots[31] = root;
        }
        h.load_processor(&image)?;
        h.oracle.collect_ram(&[root, class], false, 8)?;
        h.oracle.execute(allocation, false);
        h.start_processor(0)?;
        assert_eq!(finish(&mut h)?, 1, "{kind}");
        assert_eq!(h.rtl.cpu_fault_o, 0, "{kind}");
        h.compare_state().map_err(|e| eyre::eyre!("{kind}: {e}"))?;
    }
    Ok(())
}

#[test]
fn nonresident_and_colliding_roots_do_not_trigger_disk_fetch() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let class = Word::reference(100, true)?;
    let resident = Word::reference(1, true)?;
    h.install(resident, class, 0, &vec![Word::NIL; 256])?;
    let missing = Word::reference(17, true)?;
    let alloc = Command::allocate(class, 256, true)?;
    let image = image(&[
        object(alloc),
        Instruction::halt(),
        Instruction::literal(missing),
    ])?;
    h.load_processor(&image)?;
    h.oracle.collect_ram(&[class, missing], false, 256)?;
    h.oracle.execute(alloc, false);
    h.start_processor(0)?;
    assert_eq!(finish(&mut h)?, 1);
    assert_eq!(h.rtl.cpu_fault_o, 0);
    h.compare_state()?;
    assert_eq!(h.stats.store_transactions, 0);
    assert!(h.oracle.resolve(resident).is_err());
    Ok(())
}

#[test]
fn fetch_exhaustion_collects_before_refill_and_preserves_request() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(
        &rt,
        Timing {
            request_delay: 2,
            memory_latency: 3,
            response_stall: 2,
        },
        None,
    )?;
    let class = Word::reference(100, true)?;
    let target = Word::reference(3, true)?;
    h.install(target, class, 0, &[Word::signed(99)])?;
    h.service(Service::Invalidate(target))?;
    h.install(Word::reference(1, true)?, class, 0, &vec![Word::NIL; 256])?;
    let code = [
        object(Command::fetch(target)),
        object(Command::index(1)?),
        object(Command::read_field()),
        Instruction::halt(),
    ];
    let image = image(&code)?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut cpu = Processor::default();
    // Collection scans the parameterized control-store roots.
    h.run_processor(&image, &mut cpu, 100_000 * CODE_WORDS.div_ceil(4096))?;
    assert_eq!(cpu.object, Word::signed(99).bits());
    assert_eq!(h.stats.collections, 1);
    // Metadata is read once before exhaustion and once on retry; marking
    // never fetches the nonresident target or any other backing record.
    assert_eq!(h.stats.store_transactions, 3);
    Ok(())
}

#[test]
fn new_victim_saved_before_failed_transfer_keeps_persistent_descendants() -> Result<()> {
    use rekursiv_asm::Faults;
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let class = Word::reference(100, true)?;
    let parent = h.execute(Command::allocate(class, 1, true)?)?.data;
    let child = h.execute(Command::allocate(class, 1, true)?)?.data;
    h.execute(Command::probe(parent))?;
    h.execute(Command::index(1)?)?;
    h.execute(Command::write_field(child))?;
    let e = h.oracle.resolve(parent)?;
    // A colliding incoming object reserves space, saves the NEW victim, then
    // fails its first refill read. Parent remains NEW, but now has disk identity.
    let incoming = Word::reference(17, true)?;
    let record = rekursiv_model::store::Record {
        reference: incoming,
        class,
        cond: false,
        body: vec![Word::ZERO],
    };
    h.store.records.insert(17, record.clone());
    h.oracle.store.records.insert(17, record);
    let rsp = h.execute_faults(
        Command::fetch(incoming).encode()?,
        Faults {
            store_at: Some(4),
            ..Faults::default()
        },
    )?;
    assert_ne!(rsp.status, rekursiv_asm::Status::Ok);
    assert!(h.store.records.contains_key(&1));
    assert_eq!(h.oracle.resolve(parent)?, e);
    // Drop incidental selection and fill the remaining source space.
    h.install(class, class, 0, &[])?;
    h.execute(Command::probe(class))?;
    h.install(Word::reference(3, false)?, class, 3, &vec![Word::ZERO; 253])?;
    let allocation = Command::allocate(class, 2, true)?;
    let image = image(&[object(allocation), Instruction::halt()])?;
    h.load_processor(&image)?;
    h.oracle.collect_ram(&[class], false, 2)?;
    h.oracle.execute(allocation, false);
    h.start_processor(0)?;
    assert_eq!(finish(&mut h)?, 1);
    assert_eq!(h.rtl.cpu_fault_o, 0);
    h.compare_state()?;
    assert!(h.oracle.resolve(parent).is_ok());
    assert!(h.oracle.resolve(child).is_ok());
    Ok(())
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(32))]
    #[test]
    fn resident_graph_matches_independent_work_list(
        graph in proptest::collection::vec((0u8..15,0u8..15,proptest::prelude::any::<bool>()),14), root in 1u8..15
    ) {
        let rt=runtime().unwrap();let mut h=Harness::new(&rt,Timing {request_delay:1,memory_latency:2,response_stall:1},None).unwrap();
        let class=Word::reference(100,true).unwrap();
        let refs:Vec<_>=graph.iter().enumerate().map(|(j,(_,_,scan))|Word::reference(j as u64+1,*scan).unwrap()).collect();
        for (j,(a,b,_)) in graph.iter().enumerate() {
            let edge=|n:u8|if n==0 {Word::NIL} else {refs[n as usize-1]};
            h.install(refs[j],class,j as u32*2,&[edge(*a),edge(*b)]).unwrap();
        }
        h.install(Word::reference(15,false).unwrap(),class,28,&vec![Word::ZERO;228]).unwrap();
        let allocation=Command::allocate(class,16,true).unwrap();
        let mut image=image(&[object(allocation),Instruction::halt()]).unwrap();
        image.roots[0]=refs[root as usize-1];
        h.load_processor(&image).unwrap();h.oracle.collect_ram(&[image.roots[0],class],false,16).unwrap();h.oracle.execute(allocation,false);
        h.start_processor(0).unwrap();assert_eq!(finish(&mut h).unwrap(),1);assert_eq!(h.rtl.cpu_fault_o,0);
        h.compare_state().unwrap();assert_eq!(h.stats.store_transactions,0);
    }
}

#[test]
fn evicted_parent_keeps_new_body_and_class_edges_without_disk_traversal() -> Result<()> {
    let rt = runtime()?;
    for class_edge in [false, true] {
        let mut h = Harness::new(&rt, Timing::default(), None)?;
        let class = Word::reference(100, true)?;
        let child = h.execute(Command::allocate(class, 1, true)?)?.data;
        let parent = h
            .execute(Command::allocate(
                if class_edge { child } else { class },
                1,
                true,
            )?)?
            .data;
        if !class_edge {
            h.execute(Command::index(1)?)?;
            h.execute(Command::write_field(child))?;
        }
        let incoming = Word::reference(18, true)?;
        let record = rekursiv_model::store::Record {
            reference: incoming,
            class,
            cond: false,
            body: vec![Word::ZERO],
        };
        h.store.records.insert(18, record.clone());
        h.oracle.store.records.insert(18, record);
        h.execute(Command::fetch(incoming))?;
        assert!(h.oracle.resolve(parent).is_err());
        assert!(!h.store.records.contains_key(&child.identity()?));
        h.install(Word::reference(3, false)?, class, 3, &vec![Word::ZERO; 253])?;
        let allocation = Command::allocate(class, 4, true)?;
        let image = image(&[object(allocation), Instruction::halt()])?;
        h.load_processor(&image)?;
        h.oracle.collect_ram(&[class], false, 4)?;
        h.oracle.execute(allocation, false);
        let store_transactions = h.stats.store_transactions;
        h.start_processor(0)?;
        assert_eq!(finish(&mut h)?, 1);
        assert_eq!(h.rtl.cpu_fault_o, 0);
        h.compare_state()?;
        assert!(h.oracle.resolve(child).is_ok());
        assert_eq!(h.stats.store_transactions, store_transactions);
    }
    Ok(())
}

#[test]
fn interrupted_refill_metadata_class_is_a_machine_root() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let class = Word::reference(1, true)?;
    let metaclass = Word::reference(100, true)?;
    h.install(class, metaclass, 0, &[Word::NIL])?;
    h.install(
        Word::reference(2, true)?,
        metaclass,
        1,
        &vec![Word::NIL; 255],
    )?;
    let target = Word::reference(3, true)?;
    let record = rekursiv_model::store::Record {
        reference: target,
        class,
        cond: false,
        body: vec![Word::signed(99)],
    };
    h.store.records.insert(3, record.clone());
    h.oracle.store.records.insert(3, record);
    let image = image(&[object(Command::fetch(target)), Instruction::halt()])?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    // Collection scans the parameterized control-store roots.
    h.run_processor(
        &image,
        &mut Processor::default(),
        100_000 * CODE_WORDS.div_ceil(4096),
    )?;
    assert_eq!(h.stats.collections, 1);
    assert!(h.oracle.resolve(class).is_ok());
    assert_eq!(h.stats.store_transactions, 3);
    Ok(())
}

#[test]
fn retry_validates_and_branches_using_the_original_condition() -> Result<()> {
    let rt = runtime()?;
    let mut h = Harness::new(&rt, Timing::default(), None)?;
    let class = Word::reference(100, true)?;
    h.install(Word::reference(1, true)?, class, 0, &vec![Word::NIL; 256])?;
    let allocation = Command::allocate(class, 1, true)?;
    let image = image(&[
        Instruction {
            seq: Seq::ConditionalJump,
            condition: Condition::Interrupt,
            branch: 65535,
            ..object(allocation)
        },
        Instruction::halt(),
    ])?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    for _ in 0..100 {
        if h.rtl.cpu_gc_active_o != 0 {
            break;
        }
        h.tick()?;
    }
    assert_ne!(h.rtl.cpu_gc_active_o, 0);
    // This would select an invalid target if retry validation saw a new IRQ.
    h.rtl.cpu_irq_i = 1;
    finish(&mut h)?;
    assert_eq!(h.rtl.cpu_fault_o, 0);
    assert_eq!(h.rtl.cpu_pc_o, 1);
    assert_eq!(h.stats.commands, 2);
    Ok(())
}
