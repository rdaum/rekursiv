//! Sends execute on RTL. Fixtures are converted offline; the running harness
//! supplies only memory/store responses and checks architectural effects.
use eyre::{ensure, Result, WrapErr};
use rekursiv_asm::{Service, Word};
use rekursiv_model::{
    processor::{Image as Program, Processor},
    store::Record,
};
use rekursiv_sim::{runtime, Harness, Timing};
use rekursiv_smalltalk::{interpreter, layout, source, target};
use std::collections::BTreeMap;

const ROOT: u16 = 100;
const CALLER: u16 = 102;
const RECEIVER: u16 = 104;
const SELECTOR: u16 = 108;
const CALLEE: u16 = 110;
const CLASS: u16 = 200;
const SUPER: u16 = 202;
const META: u16 = 204;
const DICT: u16 = 206;
const METHODS: u16 = 208;
fn r(oop: u16) -> Word {
    layout::reference(oop).unwrap()
}
fn i(value: i32) -> Word {
    Word::signed(value)
}
fn oop(value: i32) -> u16 {
    layout::integer_oop(value).unwrap()
}
fn pointers(image: &mut source::Image, object: u16, class: u16, fields: Vec<u16>) {
    image.objects.insert(
        object,
        source::Object {
            oop: object,
            class,
            body: source::Body::Pointers(fields),
        },
    );
}
fn method(
    image: &mut source::Image,
    object: u16,
    args: u16,
    temps: u16,
    literals: &[u16],
    bytes: &[u8],
) {
    image.objects.insert(
        object,
        source::Object {
            oop: object,
            class: 34,
            body: source::Body::Method {
                header: layout::MethodHeader(
                    1 | (args << 13) | (temps << 8) | ((literals.len() as u16) << 1),
                ),
                literals: literals.to_vec(),
                bytes: bytes.to_vec(),
            },
        },
    );
}
fn dictionary(
    image: &mut source::Image,
    class: u16,
    parent: u16,
    dict: u16,
    methods: u16,
    bindings: &[(u16, u16)],
) {
    pointers(
        image,
        class,
        META,
        vec![parent, dict, if class == META { 0xc007 } else { 0xc005 }],
    );
    let mut selectors = vec![2; 4];
    let mut code = vec![2; 4];
    for &(selector, method) in bindings {
        let mut index = (selector as usize >> 1) & 3;
        while selectors[index] != 2 {
            index = (index + 1) & 3;
        }
        selectors[index] = selector;
        code[index] = method;
    }
    let mut fields = vec![oop(bindings.len() as i32), methods];
    fields.extend(selectors);
    pointers(image, dict, 16, fields);
    pointers(image, methods, 16, code);
}
fn fixture(bytes: &[u8], literals: &[u16]) -> source::Image {
    let mut image = source::Image {
        sha256: String::new(),
        object_space_words: 0,
        object_table_words: 0,
        objects: BTreeMap::new(),
    };
    for (class, spec) in [
        (12, 0x4001),
        (14, 0x2001),
        (16, 0xe001),
        (22, 0xe00d),
        (34, 0x2001),
        (52, 0xc001),
        (54, 0xc001),
        (56, 0xc001),
        (60, 0xc007),
        (META, 0xc007),
    ] {
        pointers(&mut image, class, 60, vec![2, 2, spec]);
    }
    for (object, class) in [(2, 52), (4, 54), (6, 56)] {
        pointers(&mut image, object, class, vec![]);
    }
    for sel in [SELECTOR, 116, 124, 132] {
        image.objects.insert(
            sel,
            source::Object {
                oop: sel,
                class: 14,
                body: source::Body::Bytes(vec![b's', sel as u8]),
            },
        );
    }
    method(&mut image, CALLER, 0, 0, literals, bytes);
    method(&mut image, CALLEE, 0, 0, &[], &[119, 124]);
    let ip = 3 + 2 * literals.len();
    let mut context = vec![2, oop(ip as i32), oop(0), CALLER, 2, RECEIVER];
    context.resize(18, 2);
    pointers(&mut image, ROOT, 22, context);
    pointers(&mut image, RECEIVER, CLASS, vec![oop(7), 2]);
    dictionary(&mut image, CLASS, 2, DICT, METHODS, &[(SELECTOR, CALLEE)]);
    pointers(&mut image, SUPER, META, vec![2, 2, 0xc001]);
    image
}
#[derive(Debug, Clone)]
struct Frame {
    reference: Word,
    sender: Word,
    method: Word,
    receiver: Word,
    ip: i32,
    slots: Vec<Word>,
}
struct Execution {
    trace: Vec<Frame>,
    result: Word,
    status: u32,
    allocations: u64,
    collections: u64,
    saves: u64,
    allocation_retries: usize,
    refill_retries: usize,
    records: BTreeMap<u64, Record>,
}
fn execute(source: source::Image) -> Result<Execution> {
    execute_root(source, ROOT, false)
}
fn execute_root(source: source::Image, root_oop: u16, force_collection: bool) -> Result<Execution> {
    let converted = target::Image::convert(&source, &[root_oop])?;
    converted.verify_against(&source)?;
    let assembly = interpreter::assemble(r(root_oop))?;
    let cycle = assembly.symbols["cycle"] as u16;
    let program = Program::from_assembly(&assembly)?
        .with_ram_collector(interpreter::COLLECTOR_ENTRY, rekursiv_sim::PAGER_ENTRIES)?;
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
    // Disk contents, prepared before execution. No runtime class-aware callbacks.
    for record in &converted.records {
        let record = Record {
            reference: record.reference,
            class: record.class,
            cond: record.cond,
            body: record.body.clone(),
        };
        h.store
            .records
            .insert(record.reference.identity()?, record.clone());
        h.oracle
            .store
            .records
            .insert(record.reference.identity()?, record);
    }
    let root = converted
        .records
        .iter()
        .find(|x| x.source_oop == root_oop)
        .unwrap();
    h.install(root.reference, root.class, 0, &root.body)?;
    if force_collection {
        h.install(Word::reference(32767, true)?, r(16), 240, &[Word::ZERO; 16])?;
    }
    h.service(Service::ReserveIdentities(converted.next_identity))?;
    h.service(Service::CompactClass {
        code: 2,
        class: r(12),
    })?;
    h.load_processor(&program)?;
    let services = h.stats.services;
    h.start_processor(assembly.entry.unwrap())?;
    let mut cpu = Processor::default();
    let mut trace = Vec::new();
    h.run_processor_observed(&program, &mut cpu, 10_000_000, |h, cpu| {
        if h.rtl.cpu_pc_o == cycle {
            let reference = h.oracle.state.vr[0];
            let entry = h.oracle.resolve(reference)?;
            let body = &h.backing[entry.base as usize..entry.base as usize + entry.size as usize];
            ensure!(
                body[2] == i(cpu.rf[8] as i32) && body[3] == i(cpu.rf[9] as i32),
                "context register materialization"
            );
            let sp = cpu.rf[9] as usize;
            ensure!(
                body[7 + sp..].iter().all(|&x| x == r(2)),
                "dead stack slots not cleared"
            );
            trace.push(Frame {
                reference,
                sender: body[1],
                method: body[4],
                receiver: body[6],
                ip: cpu.rf[8] as i32,
                slots: body[7..7 + sp].to_vec(),
            });
        }
        Ok(())
    })
    .wrap_err_with(|| {
        format!(
            "microaddress {} status {} rf {:?}",
            h.rtl.cpu_pc_o, h.rtl.cpu_last_status_o, cpu.rf
        )
    })?;
    ensure!(
        h.stats.services == services,
        "host service during execution"
    );
    ensure!(h.rtl.cpu_fault_o == 0);
    // Reconcile device records with remaining resident copies for assertions.
    let mut records = h.store.records.clone();
    for entry in h.oracle.entries.iter().flatten() {
        records.insert(
            entry.reference.identity()?,
            Record {
                reference: entry.reference,
                class: entry.class,
                cond: entry.cond,
                body: h.backing[entry.base as usize..entry.base as usize + entry.size as usize]
                    .to_vec(),
            },
        );
    }
    Ok(Execution {
        trace,
        result: h.oracle.state.vr[5],
        status: cpu.rf[15],
        allocations: h.rtl.dbg_next_identity_o - converted.next_identity,
        collections: h.stats.collections,
        saves: h.stats.saved_objects,
        allocation_retries: h
            .commands
            .iter()
            .filter(|(c, r)| c.pager == 6 && r.status == rekursiv_asm::Status::OutOfSpace)
            .count(),
        refill_retries: h
            .commands
            .iter()
            .filter(|(c, r)| c.pager == 5 && r.status == rekursiv_asm::Status::OutOfSpace)
            .count(),
        records,
    })
}

#[test]
fn send_activates_guest_context_and_returns() -> Result<()> {
    let e = execute(fixture(&[112, 208, 124], &[SELECTOR]))?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 1));
    assert_eq!(
        e.trace
            .iter()
            .map(|f| (f.method, f.ip, f.slots.clone()))
            .collect::<Vec<_>>(),
        vec![
            (r(CALLER), 5, vec![]),
            (r(CALLER), 6, vec![r(RECEIVER)]),
            (r(CALLEE), 3, vec![]),
            (r(CALLEE), 4, vec![i(2)]),
            (r(CALLER), 7, vec![i(2)])
        ]
    );
    let child = &e.trace[2];
    assert_eq!(child.sender, r(ROOT));
    assert_eq!(child.receiver, r(RECEIVER));
    let body = &e.records[&child.reference.identity()?].body;
    assert_eq!(&body[1..3], &[r(2), r(2)]);
    Ok(())
}

#[test]
fn argument_order_temporaries_and_nested_sender_chain_are_guest_visible() -> Result<()> {
    let mut source = fixture(&[112, 118, 119, 240, 124], &[SELECTOR]);
    method(&mut source, CALLEE, 2, 3, &[116], &[112, 16, 17, 240, 124]);
    method(&mut source, 112, 2, 2, &[], &[16, 17, 177, 124]);
    dictionary(
        &mut source,
        CLASS,
        2,
        DICT,
        METHODS,
        &[(SELECTOR, CALLEE), (116, 112)],
    );
    let e = execute(source)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(-1), 2));
    let outer = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
    assert_eq!(outer.slots, vec![i(1), i(2), r(2)]);
    let inner = e.trace.iter().find(|f| f.method == r(112)).unwrap();
    assert_eq!(inner.slots, vec![i(1), i(2)]);
    assert_eq!(inner.sender, outer.reference);
    assert_eq!(outer.sender, r(ROOT));
    assert_eq!(inner.receiver, r(RECEIVER));
    Ok(())
}

#[test]
fn dictionary_collision_wrap_and_superclass_search() -> Result<()> {
    let mut source = fixture(&[112, 208, 124], &[132]);
    // All four selectors hash to slot two. The desired selector wraps to one.
    dictionary(&mut source, CLASS, SUPER, DICT, METHODS, &[]);
    dictionary(
        &mut source,
        SUPER,
        2,
        210,
        212,
        &[(SELECTOR, 110), (116, 112), (124, 114), (132, 118)],
    );
    for (object, value) in [(110, 0), (112, 1), (114, 2), (118, -1)] {
        method(&mut source, object, 0, 0, &[], &[(117 + value) as u8, 124]);
    }
    let e = execute(source)?;
    assert_eq!((e.status, e.result), (1, i(-1)));
    assert!(e.trace.iter().any(|f| f.method == r(118)));
    Ok(())
}

#[test]
fn extended_sends_and_super_start_above_the_defining_class() -> Result<()> {
    for send in [vec![131, 0], vec![132, 0, 0]] {
        let mut bytes = vec![112];
        bytes.extend(send);
        bytes.push(124);
        let e = execute(fixture(&bytes, &[SELECTOR]))?;
        assert_eq!((e.status, e.result), (1, i(2)));
    }
    for send in [vec![133, 0], vec![134, 0, 0]] {
        let mut source = fixture(&[112, 208, 124], &[SELECTOR]);
        // The inherited method is defined in SUPER. A super send must skip
        // SUPER's override, starting in its parent (214), not CLASS's parent.
        dictionary(&mut source, CLASS, SUPER, DICT, METHODS, &[]);
        dictionary(
            &mut source,
            SUPER,
            214,
            210,
            212,
            &[(SELECTOR, CALLEE), (116, 112)],
        );
        dictionary(&mut source, 214, 2, 216, 218, &[(116, 114)]);
        pointers(&mut source, 220, 16, vec![2, SUPER]);
        let mut body = vec![112];
        body.extend(send);
        body.push(124);
        method(&mut source, CALLEE, 0, 0, &[116, 220], &body);
        method(&mut source, 112, 0, 0, &[], &[117, 124]);
        method(&mut source, 114, 0, 0, &[], &[119, 124]);
        let e = execute(source)?;
        assert_eq!((e.status, e.result), (1, i(2)));
        assert!(!e.trace.iter().any(|f| f.method == r(112)));
    }
    Ok(())
}

#[test]
fn quick_methods_do_not_allocate_contexts_and_primitive_failure_activates_body() -> Result<()> {
    for (flag, temps, result) in [(5, 0, r(RECEIVER)), (6, 0, i(7))] {
        let mut source = fixture(&[112, 208, 124], &[SELECTOR]);
        method(&mut source, CALLEE, flag, temps, &[], &[]);
        let e = execute(source)?;
        assert_eq!((e.status, e.result, e.allocations), (1, result, 0));
    }
    let mut source = fixture(&[112, 118, 224, 124], &[SELECTOR]);
    pointers(&mut source, 216, 16, vec![2, CLASS]);
    method(
        &mut source,
        CALLEE,
        7,
        1,
        &[oop((1 << 8) | 10), 216],
        &[16, 124],
    );
    let e = execute(source)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(1), 1));
    Ok(())
}

#[test]
fn imported_send_allocates_objects_and_returns_a_reference_after_collection() -> Result<()> {
    let mut source = fixture(&[112, 208, 124], &[SELECTOR]);
    method(
        &mut source,
        CALLEE,
        0,
        2,
        &[CLASS, 132, oop(30)],
        &[
            34, 104, 32, 209, 105, 32, 209, 135, 16, 118, 177, 104, 16, 117, 182, 168, 2, 163, 242,
            17, 124,
        ],
    );
    dictionary(&mut source, META, 2, 210, 212, &[(132, 214)]);
    pointers(&mut source, 216, 16, vec![2, META]);
    method(&mut source, 214, 7, 0, &[oop(70), 216], &[123]);
    if let source::Body::Pointers(fields) = &mut source.objects.get_mut(&CLASS).unwrap().body {
        fields[2] = 0xc005;
    }
    let e = execute(source)?;
    assert_eq!(e.status, 1);
    assert_eq!(e.allocations, 32);
    assert_eq!(e.result, Word::reference(32769, true)?);
    assert!(e.collections > 0, "no machine collection");
    assert!(e.saves > 0, "no eviction saves");
    assert!(
        e.allocation_retries > 0,
        "no allocation recovery ({} refill recoveries)",
        e.refill_retries
    );
    let result = &e.records[&e.result.identity()?];
    assert_eq!(result.class, r(CLASS));
    assert_eq!(result.body, vec![Word::raw(16)?, r(2), r(2)]);
    let first_child = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
    assert_eq!(first_child.sender, r(ROOT));
    assert_eq!(first_child.slots, vec![r(2), r(2)]);
    Ok(())
}

#[test]
fn missing_method_and_argument_mismatch_preserve_caller_operands() -> Result<()> {
    let mut source = fixture(&[112, 118, 224, 124], &[SELECTOR]);
    let e = execute(source.clone())?;
    assert_eq!(e.status, 7);
    assert_eq!(e.allocations, 0);
    let caller = &e.records[&r(ROOT).identity()?].body;
    assert_eq!(&caller[7..9], &[r(RECEIVER), i(1)]);
    dictionary(&mut source, CLASS, 2, DICT, METHODS, &[]);
    let e = execute(source)?;
    assert_eq!(e.status, 6);
    assert_eq!(e.allocations, 0);
    let caller = &e.records[&r(ROOT).identity()?].body;
    assert_eq!(&caller[7..9], &[r(RECEIVER), i(1)]);
    Ok(())
}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn original_xerox_method_sends_basic_new_and_returns_after_collection() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let bytes = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    ensure!(rekursiv_smalltalk::checksum(&bytes) == rekursiv_smalltalk::IMAGE_SHA256);
    let mut source = source::Image::parse(&bytes)?;
    // Keep every original object unchanged, including both original methods.
    // Add a synthetic class whose metaclass exposes Behavior>>basicNew. The
    // saved root runs ExternalStream class>>new, whose send invokes that method.
    for new_oop in [65522, 65524, 65526, 65528, 65530] {
        ensure!(!source.objects.contains_key(&new_oop));
    }
    let mut methods = vec![2; 4];
    methods[3] = 0x49a6;
    pointers(&mut source, 65522, 16, methods);
    pointers(&mut source, 65524, 16, vec![oop(1), 65522, 2, 2, 2, 0x117e]);
    pointers(&mut source, 65526, 16, vec![2, 65524, 0xc007]);
    pointers(&mut source, 65528, 65526, vec![2, 2, 0xc005]);
    let mut context = vec![2, oop(5), oop(0), 0x761e, 2, 65528];
    context.resize(18, 2);
    pointers(&mut source, 65530, 22, context);
    let e = execute_root(source, 65530, true)?;
    assert_eq!((e.status, e.allocations), (1, 1));
    assert!(e.collections > 0);
    assert!(e.refill_retries > 0, "no refill recovery");
    assert_eq!(e.result, Word::reference(32768, true)?);
    let result = &e.records[&32768];
    assert_eq!(result.class, r(65528));
    assert_eq!(result.body, vec![Word::raw(16)?, r(2), r(2)]);
    Ok(())
}

#[test]
fn large_context_initialization_and_escaped_returned_context() -> Result<()> {
    let mut source = fixture(&[112, 118, 224, 124], &[SELECTOR]);
    method(&mut source, CALLEE, 1, 20, &[], &[16, 124]);
    if let source::Body::Method { header, .. } = &mut source.objects.get_mut(&CALLEE).unwrap().body
    {
        header.0 |= 128;
    }
    let e = execute(source)?;
    assert_eq!((e.status, e.result), (1, i(1)));
    let activation = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
    assert_eq!(activation.slots.len(), 20);
    assert_eq!(activation.slots[0], i(1));
    assert!(activation.slots[1..].iter().all(|&v| v == r(2)));
    assert_eq!(e.records[&activation.reference.identity()?].body.len(), 39);
    let mut source = fixture(&[112, 208, 124], &[SELECTOR]);
    method(&mut source, CALLEE, 0, 0, &[], &[137, 124]);
    let e = execute(source)?;
    assert_eq!(e.status, 1);
    let activation = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
    assert_eq!(e.result, activation.reference);
    let body = &e.records[&e.result.identity()?].body;
    assert_eq!(&body[1..3], &[r(2), r(2)]);
    assert_eq!(body[4], r(CALLEE));
    assert_eq!(body[6], r(RECEIVER));
    Ok(())
}

#[test]
fn arithmetic_failure_sends_the_selector_and_activates_fallback() -> Result<()> {
    let mut source = fixture(&[112, 118, 176, 124], &[]);
    // Arithmetic fails because the receiver is not a SmallInteger. The guest
    // selector table determines '+', then ordinary lookup finds a method.
    pointers(&mut source, 48, 16, vec![SELECTOR, oop(1)]);
    method(&mut source, CALLEE, 1, 1, &[], &[16, 124]);
    let e = execute(source)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(1), 1));
    assert!(e
        .trace
        .iter()
        .any(|f| f.method == r(CALLEE) && f.slots == [i(1)]));
    Ok(())
}
