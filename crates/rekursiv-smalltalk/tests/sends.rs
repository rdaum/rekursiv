//! Sends execute on RTL. Fixtures are converted offline; the running harness
//! supplies only memory/store responses and checks architectural effects.
use eyre::{ensure, Result, WrapErr};
use rekursiv_asm::{Service, Word};
use rekursiv_model::{
    processor::{Image as Program, Processor},
    store::Record,
};
use rekursiv_sim::{Harness, Timing};
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
        (26, 0xc005),
        (28, 0x2001),
        (40, 0xc003),
        (32, 0xc005),
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
    for sel in [42, SELECTOR, 116, 124, 132] {
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
    unsigned_top: Option<u64>,
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
    recovery_after_exchange: bool,
    records: BTreeMap<u64, Record>,
    roots: [Word; 32],
    idle_visits: usize,
    event_acknowledgements: [u64; 4],
    device_requests: Vec<rekursiv_sim::device::Request>,
    timer_deadline: Option<u64>,
    capacity_samples: Vec<(u32, u64)>,
    debugger_breaks: usize,
    service_stop: bool,
    mouse: Option<(i32, i32)>,
    cursor: Option<(i32, i32)>,
    cursor_linked: Option<bool>,
    sample_interval: Option<u32>,
    input_remaining: Option<usize>,
    input_overruns: Option<u64>,
    cursor_frame: Option<rekursiv_sim::device::BitmapFrame>,
    display_frame: Option<rekursiv_sim::device::BitmapFrame>,
    bitmap_publications: [u64; 2],
}
fn execute(source: source::Image) -> Result<Execution> {
    execute_root(source, ROOT, false)
}
fn execute_root(source: source::Image, root_oop: u16, force_collection: bool) -> Result<Execution> {
    execute_prepared(source, root_oop, force_collection, |_| {})
}
fn execute_prepared(
    source: source::Image,
    root_oop: u16,
    force_collection: bool,
    prepare: impl FnOnce(&mut target::Image),
) -> Result<Execution> {
    execute_sized(
        source,
        root_oop,
        force_collection,
        rekursiv_sim::MEMORY_WORDS,
        prepare,
    )
}
fn execute_sized(
    source: source::Image,
    root_oop: u16,
    force_collection: bool,
    memory_words: usize,
    prepare: impl FnOnce(&mut target::Image),
) -> Result<Execution> {
    execute_with_device(
        source,
        root_oop,
        force_collection,
        memory_words,
        prepare,
        Default::default(),
    )
}
fn execute_with_device(
    source: source::Image,
    root_oop: u16,
    force_collection: bool,
    memory_words: usize,
    prepare: impl FnOnce(&mut target::Image),
    device: rekursiv_sim::device::Device,
) -> Result<Execution> {
    execute_machine(
        source,
        root_oop,
        force_collection,
        memory_words,
        prepare,
        device,
        false,
    )
}
fn execute_machine(
    source: source::Image,
    root_oop: u16,
    force_collection: bool,
    memory_words: usize,
    prepare: impl FnOnce(&mut target::Image),
    device: rekursiv_sim::device::Device,
    resume_debugger: bool,
) -> Result<Execution> {
    let mut converted = target::Image::convert(&source, &[root_oop])?;
    converted.verify_against(&source)?;
    // Extend a converted fixture before boot, for identities which cannot be
    // represented in the original image's 16-bit object table.
    prepare(&mut converted);
    let assembly = interpreter::assemble(r(root_oop))?;
    let cycle = assembly.symbols["cycle"] as u16;
    let program = Program::from_assembly(&assembly)?
        .with_ram_collector(interpreter::COLLECTOR_ENTRY, rekursiv_sim::PAGER_ENTRIES)?;
    let rt = rekursiv_sim::runtime_with_memory(memory_words)?;
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
        let filler_id = (1..32768)
            .rev()
            .find(|id| !h.store.records.contains_key(id))
            .unwrap();
        h.install(
            Word::reference(filler_id, true)?,
            r(16),
            (memory_words / 2 - 16) as u32,
            &[Word::ZERO; 16],
        )?;
    }
    h.service(Service::ReserveIdentities(converted.next_identity))?;
    h.service(Service::CompactClass {
        code: 2,
        class: r(12),
    })?;
    h.load_processor(&program)?;
    h.device = device;
    let services = h.stats.services;
    h.start_processor(assembly.entry.unwrap())?;
    let mut cpu = Processor::default();
    let mut trace = Vec::new();
    let mut idle_visits = 0;
    let mut capacity_samples = Vec::new();
    let mut debugger_breaks = 0;
    loop {
        h.run_processor_observed(&program, &mut cpu, 10_000_000, |h, cpu| {
            if h.rtl.cpu_pc_o == assembly.symbols["capacity_value"] as u16 {
                let expected = if cpu.rf[0] == 112 {
                    u64::from(
                        h.oracle
                            .allocation_limit
                            .saturating_sub(h.oracle.body_cursor),
                    )
                } else {
                    (1 << 37) - h.oracle.next_identity
                };
                ensure!(
                    cpu.object == expected,
                    "capacity sample differs from committed allocator state"
                );
                capacity_samples.push((cpu.rf[0], expected));
            }
            if h.rtl.cpu_pc_o == assembly.symbols["scheduler_idle"] as u16 {
                idle_visits += 1;
            }
            if h.rtl.cpu_pc_o == cycle {
                let reference = h.oracle.state.vr[0];
                let entry = h.oracle.resolve(reference)?;
                let body =
                    &h.backing[entry.base as usize..entry.base as usize + entry.size as usize];
                ensure!(
                    body[2] == i(cpu.rf[8] as i32) && body[3] == i(cpu.rf[9] as i32),
                    "context register materialization"
                );
                let sp = cpu.rf[9] as usize;
                ensure!(
                    body[7 + sp..].iter().all(|&x| x == r(2)),
                    "dead stack slots not cleared"
                );
                // Copy a numeric diagnostic while the stack still roots it.
                // Later collection may reclaim a popped LargePositiveInteger.
                let unsigned_top = body[7..7 + sp].last().and_then(|&word| {
                    if let Ok((2, value)) = word.compact_parts() {
                        return Some(u64::from(value));
                    }
                    let (class, number) = if let Ok(entry) = h.oracle.resolve(word) {
                        (
                            entry.class,
                            &h.backing[entry.base as usize..(entry.base + entry.size) as usize],
                        )
                    } else {
                        let record = h.store.records.get(&word.identity().ok()?)?;
                        (record.class, record.body.as_slice())
                    };
                    if class != r(28)
                        || !(2..=9).contains(&number.len())
                        || number[1..].iter().any(|b| b.bits() > 255)
                    {
                        return None;
                    }
                    Some(
                        number[1..]
                            .iter()
                            .enumerate()
                            .fold(0, |n, (i, b)| n | (b.bits() << (8 * i))),
                    )
                });
                trace.push(Frame {
                    unsigned_top,
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
        if h.rtl.cpu_service_o == 0 {
            break;
        }
        debugger_breaks += 1;
        ensure!(
            cpu.rf[15] == 11 && cpu.service_code == 11 && debugger_breaks == 1,
            "unexpected or repeated debugger break"
        );
        if !resume_debugger {
            break;
        }
        // A debugger resume toggles only the generic machine control. No guest
        // state, result, primitive, or next microaddress is supplied by the host.
        h.resume_processor()?;
        cpu.service = false;
    }
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
        cursor_frame: h
            .device
            .cursor_bitmap
            .as_ref()
            .and_then(|b| b.visible.clone()),
        display_frame: h
            .device
            .display_bitmap
            .as_ref()
            .and_then(|b| b.visible.clone()),
        bitmap_publications: [
            h.device
                .cursor_bitmap
                .as_ref()
                .map_or(0, |b| b.publications),
            h.device
                .display_bitmap
                .as_ref()
                .map_or(0, |b| b.publications),
        ],
        input_remaining: h.device.input.as_ref().map(|i| i.len()),
        input_overruns: h.device.input.as_ref().map(|i| i.overruns),
        mouse: h.device.pointer.as_ref().map(|p| p.mouse),
        cursor: h.device.pointer.as_ref().map(|p| p.cursor),
        cursor_linked: h.device.pointer.as_ref().map(|p| p.linked),
        sample_interval: h.device.pointer.as_ref().map(|p| p.sample_interval_ms),
        capacity_samples,
        debugger_breaks,
        service_stop: h.rtl.cpu_service_o != 0,
        device_requests: h.device.requests.clone(),
        timer_deadline: h.device.clocks.as_ref().and_then(|c| c.deadline),
        roots: cpu.roots.unwrap(),
        idle_visits,
        event_acknowledgements: h
            .device
            .events
            .as_ref()
            .map_or([0; 4], |e| e.acknowledgements),
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
        recovery_after_exchange: h
            .commands
            .iter()
            .position(|(c, r)| c.pager == 7 && r.status == rekursiv_asm::Status::Ok)
            .is_some_and(|at| {
                h.commands[at + 1..].iter().any(|(c, r)| {
                    matches!(c.pager, 5 | 6) && r.status == rekursiv_asm::Status::OutOfSpace
                })
            }),
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
fn argument_mismatch_preserves_operands_and_missing_dnu_stops_recursion() -> Result<()> {
    let mut source = fixture(&[112, 118, 224, 124], &[SELECTOR]);
    let e = execute(source.clone())?;
    assert_eq!(e.status, 7);
    assert_eq!(e.allocations, 0);
    let caller = &e.records[&r(ROOT).identity()?].body;
    assert_eq!(&caller[7..9], &[r(RECEIVER), i(1)]);
    dictionary(&mut source, CLASS, 2, DICT, METHODS, &[]);
    let e = execute(source)?;
    assert_eq!(e.status, 6);
    assert_eq!(e.allocations, 2);
    let caller = &e.records[&r(ROOT).identity()?].body;
    assert_eq!(caller[7], r(RECEIVER));
    let message = &e.records[&caller[8].identity()?];
    assert_eq!(message.class, r(32));
    assert_eq!(message.body[1], r(SELECTOR));
    let args = &e.records[&message.body[2].identity()?];
    assert_eq!(args.body[1..], [i(1)]);
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

// The special-selector table is ordinary guest data. These fixtures deliberately
// omit block primitive methods: special bytecodes must execute their fast path.
fn block_fixture(bytes: &[u8], literals: &[u16]) -> source::Image {
    let mut image = fixture(bytes, literals);
    pointers(&mut image, 24, META, vec![2, 2, 0xe00d]);
    let mut selectors = Vec::new();
    for op in 176..208 {
        selectors.extend([SELECTOR, oop(if op == 201 { 0 } else { 1 })]);
    }
    pointers(&mut image, 48, 16, selectors);
    image
}

#[test]
fn block_copy_and_repeated_local_returns_execute_without_method_lookup() -> Result<()> {
    let e = execute_root(
        block_fixture(
            &[137, 117, 200, 164, 2, 119, 125, 136, 201, 135, 201, 124],
            &[],
        ),
        ROOT,
        true,
    )?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 1));
    assert!(e.collections > 0);
    let block = &e.records[&32768];
    assert_eq!(block.class, r(24));
    assert_eq!(block.body[4], i(0));
    assert_eq!(block.body[5], i(8)); // initial IP survives each return
    assert_eq!(block.body[6], r(ROOT));
    assert_eq!(&block.body[1..3], &[r(2), r(2)]);
    assert_eq!(
        e.trace
            .iter()
            .filter(|f| f.reference == block.reference)
            .count(),
        4
    );
    Ok(())
}

#[test]
fn escaped_block_reads_and_updates_its_home_temporary_after_method_return() -> Result<()> {
    let mut image = block_fixture(&[112, 208, 201, 124], &[SELECTOR]);
    // Home temp0 starts at 2. The escaped block changes it to 1 and returns it.
    method(
        &mut image,
        CALLEE,
        0,
        1,
        &[],
        &[119, 104, 137, 117, 200, 164, 4, 118, 104, 16, 125, 124],
    );
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(1), 2));
    let home = &e.records[&32768];
    let block = &e.records[&32769];
    assert_eq!(home.body[7], i(1));
    assert_eq!(&home.body[1..3], &[r(2), r(2)]);
    assert_eq!(block.body[6], home.reference);
    Ok(())
}

#[test]
fn block_argument_moves_to_home_temporary_and_nonlocal_return_skips_caller() -> Result<()> {
    let mut image = block_fixture(&[112, 208, 124], &[SELECTOR]);
    // Block takes one argument. The compiler stores it in a home temporary.
    // Its method-return bytecode returns to ROOT, bypassing the -1 below.
    method(
        &mut image,
        CALLEE,
        0,
        1,
        &[],
        &[137, 118, 200, 164, 3, 104, 16, 124, 119, 202, 116, 124],
    );
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 2));
    assert_eq!(e.records[&32768].body[7], i(2));
    assert!(e
        .trace
        .iter()
        .any(|f| f.reference == Word::reference(32769, true).unwrap() && f.slots == vec![i(2)]));
    Ok(())
}

#[test]
fn escaped_nonlocal_return_sends_cannot_return_with_the_result() -> Result<()> {
    let mut image = block_fixture(&[112, 208, 201, 124], &[SELECTOR]);
    method(
        &mut image,
        CALLEE,
        0,
        0,
        &[],
        &[137, 117, 200, 164, 3, 119, 124, 125, 124],
    );
    image.objects.insert(
        44,
        source::Object {
            oop: 44,
            class: 14,
            body: source::Body::Bytes(b"cannotReturn:".to_vec()),
        },
    );
    method(&mut image, 114, 1, 1, &[], &[16, 124]);
    dictionary(&mut image, 24, 2, 212, 214, &[(44, 114)]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result), (1, i(2)));
    let handler = e.trace.iter().find(|f| f.method == r(114)).unwrap();
    assert_eq!(handler.receiver, Word::reference(32769, true)?);
    assert_eq!(handler.slots, vec![i(2)]);
    Ok(())
}

#[test]
fn does_not_understand_receives_guest_message_with_ordered_arguments_after_gc() -> Result<()> {
    for args in 0..=2 {
        let mut bytes = vec![112];
        bytes.extend([118, 119].into_iter().take(args));
        bytes.extend([208 + 16 * args as u8, 124]);
        let mut image = fixture(&bytes, &[SELECTOR]);
        dictionary(&mut image, CLASS, 2, DICT, METHODS, &[(42, CALLEE)]);
        // Return the actual Message argument, allowing inspection of its graph.
        method(&mut image, CALLEE, 1, 1, &[], &[16, 124]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(e.status, 1);
        assert!(e.collections > 0);
        let message = &e.records[&e.result.identity()?];
        assert_eq!(message.class, r(32));
        assert_eq!(message.body[1], r(SELECTOR));
        let arguments = &e.records[&message.body[2].identity()?];
        assert_eq!(arguments.class, r(16));
        assert_eq!(arguments.body[1..], [i(1), i(2)][..args]);
        let handler = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
        assert_eq!(handler.receiver, r(RECEIVER));
        assert_eq!(handler.slots, vec![message.reference]);
    }
    Ok(())
}

fn primitive_method(image: &mut source::Image, number: i32, args: i32, bytes: &[u8]) {
    pointers(image, 216, 16, vec![2, CLASS]);
    method(
        image,
        CALLEE,
        7,
        args as u16,
        &[oop((args << 8) | number), 216],
        bytes,
    );
}

#[test]
fn block_value_with_argument_array_copies_values_without_consuming_array() -> Result<()> {
    let mut image = block_fixture(&[32, 33, 224, 124], &[220, 222, SELECTOR]);
    // Send selector literal2 to block literal0 with array literal1.
    method(
        &mut image,
        CALLER,
        0,
        1,
        &[220, 222, SELECTOR],
        &[32, 33, 226, 124, 104, 16, 125],
    );
    if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&ROOT).unwrap().body {
        fields[1] = oop(9);
        fields[2] = oop(1);
    }
    let mut block = vec![2, oop(13), oop(0), oop(1), oop(13), ROOT];
    block.resize(18, 2);
    pointers(&mut image, 220, 24, block);
    pointers(&mut image, 222, 16, vec![oop(7)]);
    dictionary(&mut image, 24, 2, 212, 214, &[(SELECTOR, CALLEE)]);
    primitive_method(&mut image, 82, 1, &[116, 124]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result), (1, i(7)));
    assert_eq!(e.records[&r(222).identity()?].body[1..], [i(7)]);
    assert_eq!(e.records[&r(ROOT).identity()?].body[7], i(7));
    Ok(())
}

#[test]
fn special_identity_and_class_use_full_tags_and_compact_class_mapping() -> Result<()> {
    for (bytes, expected) in [
        (vec![112, 112, 198, 124], r(6)),
        (vec![117, 118, 198, 124], r(4)),
        (vec![117, 117, 198, 124], r(6)),
        (vec![112, 199, 124], r(CLASS)),
        (vec![118, 199, 124], r(12)),
    ] {
        let mut source = block_fixture(&bytes, &[]);
        if let source::Body::Pointers(fields) = &mut source.objects.get_mut(&48).unwrap().body {
            fields[(199 - 176) * 2 + 1] = oop(0);
        }
        let e = execute(source)?;
        assert_eq!((e.status, e.result, e.allocations), (1, expected, 0));
    }
    Ok(())
}

#[test]
fn integer_primitive_methods_cover_division_rounding_shifts_and_failure() -> Result<()> {
    let cases = [
        (1, 7, 3, 10),
        (2, 7, 3, 4),
        (9, -7, 3, -21),
        (10, 12, -3, -4),
        (10, 7, 3, -1),
        (10, 7, 0, -1),
        (10, -16384, -1, -1),
        (11, 7, 3, 1),
        (11, -7, 3, 2),
        (11, 7, -3, -2),
        (11, -7, -3, -1),
        (12, 7, 3, 2),
        (12, -7, 3, -3),
        (12, 7, -3, -3),
        (12, -7, -3, 2),
        (13, -7, 3, -2),
        (13, 7, -3, -2),
        (13, -7, -3, 2),
        (14, 7, 3, 3),
        (15, 4, 3, 7),
        (16, 7, 3, 4),
        (17, -1, 14, -16384),
        (17, 1, 14, -1),
        (17, 1, 15, -1),
        (17, 0, 16383, 0),
        (17, -7, -1, -4),
        (17, 7, -1, 3),
        (17, -16384, -16384, -1),
        (17, 16383, -16384, 0),
    ];
    for (primitive, a, b, expected) in cases {
        let mut image = fixture(&[32, 33, 226, 124], &[oop(a), oop(b), SELECTOR]);
        dictionary(&mut image, 12, 2, 212, 214, &[(SELECTOR, CALLEE)]);
        primitive_method(&mut image, primitive, 1, &[116, 124]);
        let e = execute(image)?;
        assert_eq!(
            (e.status, e.result),
            (1, i(expected)),
            "primitive {primitive}, {a}, {b}"
        );
        let failed = matches!(
            (primitive, a, b),
            (10, 7, 3 | 0) | (10, -16384, -1) | (17, 1, 14 | 15)
        );
        assert_eq!(
            e.allocations,
            u64::from(failed),
            "primitive {primitive}, {a}, {b}"
        );
        if failed {
            let frame = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
            assert_eq!(frame.receiver, i(a));
            assert_eq!(frame.slots, vec![i(b)]);
        }
    }
    Ok(())
}

fn indexed_fixture(
    number: i32,
    body: source::Body,
    specification: u16,
    operands: &[u16],
) -> source::Image {
    let mut literals = vec![220];
    literals.extend_from_slice(operands);
    literals.push(SELECTOR);
    let mut bytes: Vec<u8> = (0..=operands.len()).map(|n| 32 + n as u8).collect();
    bytes.extend([
        208 + 16 * operands.len() as u8 + operands.len() as u8 + 1,
        124,
    ]);
    let mut image = fixture(&bytes, &literals);
    image.objects.insert(
        220,
        source::Object {
            oop: 220,
            class: CLASS,
            body,
        },
    );
    if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&CLASS).unwrap().body {
        fields[2] = specification;
    }
    primitive_method(&mut image, number, operands.len() as i32, &[116, 124]);
    image
}

#[test]
fn indexed_pointer_access_respects_fixed_fields_and_bounds() -> Result<()> {
    for (number, operands, result) in [
        (60, vec![oop(1)], r(RECEIVER)),
        (73, vec![oop(1)], i(8)),
        (62, vec![], i(2)),
        (60, vec![oop(0)], i(-1)),
        (60, vec![oop(3)], i(-1)),
        (61, vec![oop(2), RECEIVER], r(RECEIVER)),
        (74, vec![oop(1), RECEIVER], r(RECEIVER)),
    ] {
        let image = indexed_fixture(
            number,
            source::Body::Pointers(vec![oop(8), RECEIVER, oop(9)]),
            0xe003,
            &operands,
        );
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(
            (e.status, e.result),
            (1, result),
            "primitive {number}, {operands:?}"
        );
        let body = &e.records[&r(220).identity()?].body;
        assert_eq!(body[1], if number == 74 { r(RECEIVER) } else { i(8) });
        assert_eq!(body[3], if number == 61 { r(RECEIVER) } else { i(9) });
    }
    Ok(())
}

#[test]
fn indexed_words_and_bytes_preserve_unsigned_values_and_reject_invalid_writes() -> Result<()> {
    for (body, specification, expected) in [
        (source::Body::Bytes(vec![0, 255]), 0x2001, 255),
        (source::Body::Words(vec![0, 65535]), 0x6001, 65535),
    ] {
        let mut image = indexed_fixture(60, body, specification, &[222]);
        image.objects.insert(
            222,
            source::Object {
                oop: 222,
                class: 28,
                body: source::Body::Bytes(vec![2]),
            },
        );
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(e.status, 1);
        if expected == 255 {
            assert_eq!(e.result, i(255));
        } else {
            let integer = &e.records[&e.result.identity()?];
            assert_eq!(integer.class, r(28));
            assert_eq!(
                integer.body,
                vec![Word::raw(10)?, Word::raw(255)?, Word::raw(255)?]
            );
        }
    }
    let mut image = indexed_fixture(61, source::Body::Words(vec![0]), 0x6001, &[oop(1), 222]);
    image.objects.insert(
        222,
        source::Object {
            oop: 222,
            class: 28,
            body: source::Body::Bytes(vec![255, 255]),
        },
    );
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result), (1, r(222)));
    assert_eq!(e.records[&r(220).identity()?].body[1], Word::raw(65535)?);
    for value in [oop(-1), oop(256), RECEIVER] {
        let e = execute(indexed_fixture(
            61,
            source::Body::Bytes(vec![17]),
            0x2001,
            &[oop(1), value],
        ))?;
        assert_eq!((e.status, e.result), (1, i(-1)));
        assert_eq!(e.records[&r(220).identity()?].body[1], Word::raw(17)?);
        let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
        assert_eq!(fallback.slots, vec![i(1), target::value(value)?]);
    }
    Ok(())
}

#[test]
fn indexed_large_integer_arguments_require_raw_bytes_and_matching_format() -> Result<()> {
    for (descriptor, digits, valid) in [
        (6, vec![Word::raw(1)?], true),
        (10, vec![Word::raw(1)?, Word::ZERO], true),
        (6, vec![i(1)], false),
        (6, vec![r(2)], false),
        (6, vec![Word::raw((1 << 32) | 1)?], false),
        (6, vec![Word::raw(256)?], false),
        (10, vec![Word::raw(1)?, i(0)], false),
        (10, vec![Word::raw(1)?, Word::raw(1 << 32)?], false),
        (9, vec![Word::raw(1)?], false), // word body with LargePositiveInteger class
        (10, vec![Word::raw(1)?], false), // descriptor promises a missing digit
    ] {
        let mut image = indexed_fixture(61, source::Body::Bytes(vec![17]), 0x2001, &[222, oop(42)]);
        image.objects.insert(
            222,
            source::Object {
                oop: 222,
                class: 28,
                body: source::Body::Bytes(vec![1; digits.len()]),
            },
        );
        let e = execute_prepared(image, ROOT, true, |target| {
            let number = target
                .records
                .iter_mut()
                .find(|r| r.source_oop == 222)
                .unwrap();
            number.body = std::iter::once(Word::raw(descriptor).unwrap())
                .chain(digits)
                .collect();
        })?;
        assert_eq!((e.status, e.result), (1, i(if valid { 42 } else { -1 })));
        assert_eq!(
            e.records[&r(220).identity()?].body[1],
            Word::raw(if valid { 42 } else { 17 })?
        );
        if !valid {
            let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
            assert_eq!(
                (fallback.receiver, fallback.slots.as_slice()),
                (r(220), [r(222), i(42)].as_slice())
            );
        }
    }
    Ok(())
}

#[test]
fn string_primitives_map_guest_character_objects() -> Result<()> {
    for primitive in [63, 64] {
        let operands = if primitive == 63 {
            vec![oop(1)]
        } else {
            vec![oop(1), 222]
        };
        let mut image =
            indexed_fixture(primitive, source::Body::Bytes(vec![65]), 0x2001, &operands);
        pointers(&mut image, 222, 40, vec![oop(65)]);
        let mut table = vec![2; 66];
        table[65] = 222;
        pointers(&mut image, 50, 16, table);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!((e.status, e.result), (1, r(222)));
        assert_eq!(e.records[&r(220).identity()?].body[1], Word::raw(65)?);
    }
    Ok(())
}

#[test]
fn new_and_new_with_size_initialize_pointer_word_and_byte_bodies() -> Result<()> {
    for (primitive, spec, count, expected_kind, expected_length) in [
        (70, 0xc005, None, 0, 2),
        (70, 0x4005, None, 1, 2),
        (71, 0xe003, Some(3), 0, 4),
        (71, 0x6001, Some(3), 1, 3),
        (71, 0x2001, Some(3), 2, 3),
        (71, 0x2001, Some(0), 2, 0),
    ] {
        let mut literals = vec![CLASS];
        let bytes = if let Some(n) = count {
            literals.extend([oop(n), SELECTOR]);
            vec![32, 33, 226, 124]
        } else {
            literals.push(SELECTOR);
            vec![32, 209, 124]
        };
        let mut image = fixture(&bytes, &literals);
        dictionary(&mut image, META, 2, 212, 214, &[(SELECTOR, CALLEE)]);
        if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&CLASS).unwrap().body {
            fields[2] = spec;
        }
        primitive_method(
            &mut image,
            primitive,
            i32::from(count.is_some()),
            &[116, 124],
        );
        let e = execute_root(image, ROOT, true)?;
        assert_eq!((e.status, e.allocations), (1, 1));
        let object = &e.records[&e.result.identity()?];
        assert_eq!(object.class, r(CLASS));
        let length_multiplier = if expected_kind == 2 { 4 } else { 8 };
        assert_eq!(
            object.body[0],
            Word::raw(expected_length * length_multiplier + expected_kind)?
        );
        assert_eq!(object.body.len(), expected_length as usize + 1);
        assert!(object.body[1..]
            .iter()
            .all(|&v| v == if expected_kind == 0 { r(2) } else { Word::ZERO }));
    }
    Ok(())
}

#[test]
fn method_indexing_keeps_full_literals_separate_from_bytecodes() -> Result<()> {
    let wide = Word::reference((1 << 36) + 7, true)?;
    for (primitive, args, expected, changed) in [
        (60, vec![oop(1)], i(-1), false),
        (60, vec![oop(4)], i(-1), false),
        (60, vec![oop(5)], i(128), false),
        (60, vec![oop(6)], i(255), false),
        (60, vec![oop(7)], i(-1), false),
        (61, vec![oop(4), oop(17)], i(-1), false),
        (61, vec![oop(5), oop(17)], i(17), true),
        (62, vec![], i(6), false),
        (68, vec![oop(1)], i(1), false),
        (68, vec![oop(2)], wide, false),
        (68, vec![oop(3)], i(-1), false),
        (69, vec![oop(1), oop(1)], i(-1), false), // even an unchanged header is immutable
        (69, vec![oop(1), oop(2)], i(-1), false), // cannot move the literal/byte boundary
        (69, vec![oop(1), oop(67)], i(-1), false), // nor change frame metadata in place
        (69, vec![oop(1), CLASS], i(-1), false),
        (69, vec![oop(2), CLASS], r(CLASS), true),
        (69, vec![oop(3), CLASS], i(-1), false),
    ] {
        let image = indexed_fixture(
            primitive,
            source::Body::Method {
                header: layout::MethodHeader(3),
                literals: vec![RECEIVER],
                bytes: vec![128, 255],
            },
            0x2001,
            &args,
        );
        // Replace one imported identity before boot. Header/literal access must
        // preserve this 37-bit reference; no legacy 16-bit oop can encode it.
        let e = execute_prepared(image, ROOT, true, |target| {
            for record in &mut target.records {
                if record.reference == r(RECEIVER) {
                    record.reference = wide;
                }
                for value in &mut record.body {
                    if *value == r(RECEIVER) {
                        *value = wide;
                    }
                }
            }
            target.next_identity = wide.identity().unwrap() + 1;
        })?;
        assert_eq!(
            (e.status, e.result),
            (1, expected),
            "primitive {primitive}, {args:?}"
        );
        assert_eq!(
            e.records[&r(220).identity()?].body,
            vec![
                Word::raw(27)?,
                i(1),
                if primitive == 69 && changed {
                    r(CLASS)
                } else {
                    wide
                },
                Word::raw(if primitive == 61 && changed { 17 } else { 128 })?,
                Word::raw(255)?,
            ]
        );
        if expected == i(-1) {
            let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
            assert_eq!(fallback.receiver, r(220));
            assert_eq!(
                fallback.slots,
                args.into_iter()
                    .map(target::value)
                    .collect::<rekursiv_smalltalk::Result<Vec<_>>>()?
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn original_compiled_method_growth_preserves_wide_literals_and_bytecodes_on_rtl() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let bytes = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    ensure!(rekursiv_smalltalk::checksum(&bytes) == rekursiv_smalltalk::IMAGE_SHA256);
    let mut image = source::Image::parse(&bytes)?;
    let dictionary = image.object(image.object(34)?.pointer(1)?)?;
    let methods = image.object(dictionary.pointer(1)?)?;
    let source::Body::Pointers(methods) = &methods.body else {
        unreachable!()
    };
    let slot = methods.iter().position(|&m| m == 0x8d0c).unwrap();
    let selector = dictionary.pointer(slot + 2)?;
    for id in [65520, 65522, 65524, 65526] {
        ensure!(!image.objects.contains_key(&id));
    }
    // The original needsStack:encoder: allocates a replacement method, copies
    // literals through objectAt:, copies only bytecodes/trailer through at:,
    // and exchanges the result into the original identity with become:.
    pointers(&mut image, 65522, 16, vec![oop(42)]);
    method(
        &mut image,
        65520,
        0,
        0,
        &[65522, 2, oop(-1)],
        &[32, 124, 0x12, 0x34, 0x56],
    );
    method(
        &mut image,
        65524,
        0,
        0,
        &[65520, oop(20), 2, selector],
        &[32, 33, 34, 243, 124],
    );
    let mut context = vec![2, oop(11), oop(0), 65524, 2, 65520];
    context.resize(18, 2);
    pointers(&mut image, 65526, 22, context);
    let wide = Word::reference((1 << 36) + 7, true)?;
    let e = execute_sized(image, 65526, true, 65536, |image| {
        relocate_fixture_object(image, r(65522), wide);
    })?;
    assert_eq!((e.status, e.result), (1, r(65520)));
    assert!(e.trace.iter().any(|f| f.method == r(0x8d0c)));
    assert!(e.collections > 0 && e.refill_retries > 0);
    let grown = &e.records[&r(65520).identity()?];
    assert_eq!(grown.class, r(34));
    assert_eq!(
        grown.body,
        vec![
            Word::raw(55)?,
            i(67),
            wide,
            r(2),
            i(-1),
            Word::raw(32)?,
            Word::raw(124)?,
            Word::raw(0x12)?,
            Word::raw(0x34)?,
            Word::raw(0x56)?
        ]
    );
    assert_eq!(
        e.records[&wide.identity()?].body,
        vec![Word::raw(8)?, i(42)]
    );
    Ok(())
}

#[test]
fn new_method_initializes_header_literals_and_bytecode_region() -> Result<()> {
    let header = 0x8207; // one argument, two temporaries, three literals
    let mut image = fixture(&[32, 33, 34, 243, 124], &[34, oop(4), header, SELECTOR]);
    dictionary(&mut image, 60, 2, 212, 214, &[(SELECTOR, CALLEE)]);
    primitive_method(&mut image, 79, 2, &[116, 124]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.allocations), (1, 1));
    let object = &e.records[&e.result.identity()?];
    assert_eq!(object.class, r(34));
    assert_eq!(
        object.body,
        vec![
            Word::raw(51)?,
            target::value(header)?,
            r(2),
            r(2),
            r(2),
            Word::ZERO,
            Word::ZERO,
            Word::ZERO,
            Word::ZERO
        ]
    );
    Ok(())
}

#[test]
fn new_method_validates_receiver_and_preserves_arguments_on_failure() -> Result<()> {
    let header = 0x8207;
    let bodies = [
        source::Body::Pointers(vec![]),
        source::Body::Pointers(vec![2, 2]),
        source::Body::Bytes(vec![0, 0, 0]),
        source::Body::Words(vec![0, 0, 0]),
        source::Body::Pointers(vec![2, 2, 2]), // specification is not an integer
        source::Body::Pointers(vec![2, 2, 0xc001]), // fixed pointers
        source::Body::Pointers(vec![2, 2, 0xe001]), // indexable pointers
        source::Body::Pointers(vec![2, 2, 0x6001]), // indexable words
        source::Body::Pointers(vec![2, 2, 0x0001]), // non-indexable bytes
        source::Body::Pointers(vec![2, 2, 0x2003]), // byte class with fixed field
    ];
    for body in bodies {
        let mut image = fixture(&[32, 33, 34, 243, 124], &[220, oop(4), header, SELECTOR]);
        image.objects.insert(
            220,
            source::Object {
                oop: 220,
                class: CLASS,
                body,
            },
        );
        primitive_method(&mut image, 79, 2, &[16, 124]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!((e.status, e.result, e.allocations), (1, i(4), 1));
        let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
        assert_eq!(fallback.receiver, r(220));
        assert_eq!(fallback.slots, vec![i(4), target::value(header)?]);
        assert!(e.collections > 0);
    }
    Ok(())
}

#[test]
fn new_method_accepts_compatible_subclass_and_rejects_invalid_arguments() -> Result<()> {
    for (count, header, succeeds) in [
        (oop(0), oop(0), true),
        (oop(-1), oop(0), false),
        (oop(4), 2, false),
    ] {
        let mut image = fixture(&[32, 33, 34, 243, 124], &[220, count, header, SELECTOR]);
        // Class identity is not fixed: a subclass retains the byte specification.
        pointers(&mut image, 220, 60, vec![34, 2, 0x2001]);
        dictionary(&mut image, 60, 2, 212, 214, &[(SELECTOR, CALLEE)]);
        primitive_method(&mut image, 79, 2, &[16, 124]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!((e.status, e.allocations), (1, 1));
        if succeeds {
            let object = &e.records[&e.result.identity()?];
            assert_eq!(object.class, r(220));
            assert_eq!(object.body, vec![Word::raw(11)?, i(0)]);
            assert!(!e.trace.iter().any(|f| f.method == r(CALLEE)));
        } else {
            assert_eq!(e.result, target::value(count)?);
            let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
            assert_eq!(fallback.receiver, r(220));
            assert_eq!(
                fallback.slots,
                vec![target::value(count)?, target::value(header)?]
            );
        }
        assert!(e.collections > 0);
    }
    Ok(())
}

#[test]
fn non_boolean_branches_send_must_be_boolean_and_resume_after_the_branch() -> Result<()> {
    for bytes in [&[112, 152, 124][..], &[112, 168, 1, 124, 116, 124]] {
        let mut image = fixture(bytes, &[]);
        dictionary(&mut image, CLASS, 2, DICT, METHODS, &[(52, CALLEE)]);
        method(&mut image, CALLEE, 0, 0, &[], &[121]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!((e.status, e.result), (1, r(6)));
        assert_eq!(
            e.trace
                .iter()
                .find(|f| f.method == r(CALLEE))
                .unwrap()
                .receiver,
            r(RECEIVER)
        );
    }
    Ok(())
}

#[test]
fn perform_validates_target_arity_before_shifting_caller_arguments() -> Result<()> {
    for target_args in [0, 1] {
        let mut image = fixture(&[112, 32, 33, 242, 124], &[116, oop(7), SELECTOR]);
        dictionary(
            &mut image,
            CLASS,
            2,
            DICT,
            METHODS,
            &[(SELECTOR, CALLEE), (116, 114)],
        );
        primitive_method(&mut image, 83, 2, &[16, 124]);
        method(
            &mut image,
            114,
            target_args,
            target_args,
            &[],
            if target_args == 1 {
                &[16, 124]
            } else {
                &[119, 124]
            },
        );
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(
            (e.status, e.result),
            (1, if target_args == 1 { i(7) } else { r(116) })
        );
        if target_args == 0 {
            let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
            assert_eq!(fallback.slots, vec![r(116), i(7)]);
        } else {
            assert!(!e.trace.iter().any(|f| f.method == r(CALLEE)));
            assert_eq!(
                e.trace.iter().find(|f| f.method == r(114)).unwrap().slots,
                vec![i(7)]
            );
        }
    }
    Ok(())
}

#[test]
fn perform_with_arguments_copies_array_and_checks_context_capacity() -> Result<()> {
    for count in [0, 2, 12] {
        let mut image = fixture(&[112, 32, 33, 242, 124], &[116, 220, SELECTOR]);
        dictionary(
            &mut image,
            CLASS,
            2,
            DICT,
            METHODS,
            &[(SELECTOR, CALLEE), (116, 114)],
        );
        primitive_method(&mut image, 84, 2, &[16, 124]);
        pointers(&mut image, 220, 16, vec![oop(7); count]);
        method(
            &mut image,
            114,
            if count == 0 { 0 } else { 2 },
            if count == 0 { 0 } else { 2 },
            &[],
            if count == 0 {
                &[119, 124]
            } else {
                &[16, 17, 176, 124]
            },
        );
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(e.status, 1);
        assert_eq!(
            e.result,
            match count {
                0 => i(2),
                2 => i(14),
                _ => r(116),
            }
        );
        assert_eq!(e.records[&r(220).identity()?].body[1..], vec![i(7); count]);
        if count == 12 {
            assert_eq!(
                e.trace
                    .iter()
                    .find(|f| f.method == r(CALLEE))
                    .unwrap()
                    .slots,
                vec![r(116), r(220)]
            );
        }
    }
    Ok(())
}

#[test]
fn perform_missing_target_constructs_message_for_the_dynamic_selector() -> Result<()> {
    let mut image = fixture(&[112, 32, 33, 242, 124], &[116, oop(7), SELECTOR]);
    dictionary(
        &mut image,
        CLASS,
        2,
        DICT,
        METHODS,
        &[(SELECTOR, CALLEE), (42, 114)],
    );
    primitive_method(&mut image, 83, 2, &[116, 124]);
    method(&mut image, 114, 1, 1, &[], &[16, 124]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!(e.status, 1);
    let message = &e.records[&e.result.identity()?];
    assert_eq!(message.class, r(32));
    assert_eq!(message.body[1], r(116));
    assert_eq!(e.records[&message.body[2].identity()?].body[1..], [i(7)]);
    Ok(())
}

fn scheduler_fixture(
    bytes: &[u8],
    literals: &[u16],
    active_priority: i32,
    other_priority: i32,
) -> source::Image {
    let mut image = fixture(bytes, literals);
    pointers(&mut image, 320, META, vec![2, 2, 0xc009]);
    pointers(&mut image, 322, META, vec![2, 2, 0xc005]);
    pointers(&mut image, 324, META, vec![2, 2, 0xc005]);
    pointers(&mut image, 8, 16, vec![2, 300]);
    pointers(&mut image, 300, 324, vec![302, 304]);
    pointers(&mut image, 302, 16, vec![314, 316, 318]);
    for queue in [314, 316, 318] {
        pointers(&mut image, queue, 322, vec![2, 2]);
    }
    pointers(&mut image, 304, 320, vec![2, 2, oop(active_priority), 2]);
    pointers(&mut image, 306, 320, vec![2, 308, oop(other_priority), 2]);
    pointers(&mut image, 312, 38, vec![2, 2, oop(0)]);
    primitive_method(&mut image, 87, 0, &[116, 124]);
    method(&mut image, 114, 7, 0, &[oop(88), 216], &[116, 124]);
    method(&mut image, 118, 7, 0, &[oop(86), 216], &[116, 124]);
    method(&mut image, 120, 7, 0, &[oop(85), 216], &[116, 124]);
    dictionary(
        &mut image,
        320,
        2,
        326,
        328,
        &[(SELECTOR, CALLEE), (124, 114)],
    );
    dictionary(&mut image, 38, 2, 330, 332, &[(116, 118), (132, 120)]);
    method(
        &mut image,
        310,
        0,
        0,
        &[312, 116, 306, 124],
        &[32, 209, 135, 119, 96, 34, 211, 135, 116, 124],
    );
    let mut context = vec![2, oop(11), oop(0), 310, 2, RECEIVER];
    context.resize(18, 2);
    pointers(&mut image, 308, 22, context);
    image
}

#[test]
fn scheduler_preempts_waits_signals_and_suspends_through_guest_queues() -> Result<()> {
    let image = scheduler_fixture(
        &[32, 209, 135, 34, 211, 135, 0, 124],
        &[306, SELECTOR, 312, 132],
        1,
        3,
    );
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result), (1, i(2)));
    assert!(e.collections > 0);
    let contexts: Vec<_> = e.trace.iter().map(|f| f.reference).collect();
    let mut transitions = vec![];
    for context in contexts {
        if transitions.last() != Some(&context) {
            transitions.push(context);
        }
    }
    assert_eq!(transitions, vec![r(ROOT), r(308), r(ROOT), r(308), r(ROOT)]);
    assert_eq!(e.records[&r(300).identity()?].body[2], r(304));
    assert_eq!(e.records[&r(304).identity()?].body[2], r(2));
    assert_eq!(e.records[&r(306).identity()?].body[2], r(308));
    assert_eq!(e.records[&r(306).identity()?].body[4], r(2));
    assert_eq!(e.records[&r(312).identity()?].body[1..], [r(2), r(2), i(0)]);
    for queue in [314, 316, 318] {
        assert_eq!(e.records[&r(queue).identity()?].body[1..], [r(2), r(2)]);
    }
    Ok(())
}

#[test]
fn scheduler_keeps_lower_priority_processes_in_fifo_order() -> Result<()> {
    let mut image = scheduler_fixture(
        &[32, 209, 135, 34, 209, 135, 119, 124],
        &[306, SELECTOR, 334],
        2,
        1,
    );
    pointers(&mut image, 334, 320, vec![2, 308, oop(1), 2]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result), (1, i(2)));
    assert!(e.trace.iter().all(|f| f.reference == r(ROOT)));
    assert_eq!(e.records[&r(314).identity()?].body[1..], [r(306), r(334)]);
    assert_eq!(e.records[&r(306).identity()?].body[1], r(334));
    assert_eq!(e.records[&r(334).identity()?].body[1], r(2));
    for process in [306, 334] {
        assert_eq!(e.records[&r(process).identity()?].body[4], r(314));
    }
    Ok(())
}

#[test]
fn semaphore_excess_signal_is_consumed_without_switching() -> Result<()> {
    let image = scheduler_fixture(
        &[32, 209, 135, 32, 210, 135, 119, 124],
        &[312, 132, 116],
        1,
        3,
    );
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result), (1, i(2)));
    assert!(e.trace.iter().all(|f| f.reference == r(ROOT)));
    assert_eq!(e.records[&r(312).identity()?].body[1..], [r(2), r(2), i(0)]);
    Ok(())
}

#[test]
fn asynchronous_input_wakes_idle_and_preserves_repeated_notifications() -> Result<()> {
    for repeated in [false, true] {
        let mut image = scheduler_fixture(
            &[32, 33, 226, 135, 33, 211, 135, 119, 124],
            &[RECEIVER, 312, SELECTOR, 116],
            1,
            3,
        );
        primitive_method(&mut image, 93, 1, &[116, 124]);
        let events = rekursiv_sim::device::Events::default();
        let mut input = rekursiv_sim::device::Input::default();
        let packet = input_packet(rekursiv_sim::device::InputKind::KeyDown, 65, 0, 1000);
        input.schedule.insert(200_000, vec![packet]);
        if repeated {
            input.schedule.insert(200_001, vec![packet]);
        }
        let mut device = rekursiv_sim::device::Device::default();
        device.events = Some(events);
        device.input = Some(input);
        device.timing = Timing {
            request_delay: 4,
            memory_latency: 7,
            response_stall: 0,
        };
        let e = execute_with_device(image, ROOT, true, 512, |_| {}, device)?;
        assert_eq!((e.status, e.result, e.allocations), (1, i(2), 1));
        assert!(
            e.idle_visits > 0,
            "notification must arrive after idle entry"
        );
        assert!(e.collections > 0);
        assert_eq!(e.roots[27], r(312));
        assert_eq!(e.roots[31], Word::ZERO);
        assert_eq!(e.event_acknowledgements, [1 + u64::from(repeated), 0, 0, 0]);
        assert_eq!(
            e.records[&r(312).identity()?].body[1..],
            [r(2), r(2), i(3 + 4 * i32::from(repeated))]
        );
        assert_eq!(e.records[&r(304).identity()?].body[2], r(2));
        assert_eq!(e.records[&r(304).identity()?].body[4], r(2));
        assert!(e.trace.iter().all(|f| f.reference == r(ROOT)));
    }
    Ok(())
}

#[test]
fn input_semaphore_registration_replaces_clears_and_rejects_invalid_values() -> Result<()> {
    for argument in [334, 2, oop(7), RECEIVER] {
        let mut image = scheduler_fixture(
            &[32, 33, 226, 135, 32, 35, 226, 124],
            &[RECEIVER, 312, SELECTOR, argument],
            1,
            3,
        );
        pointers(&mut image, 334, 38, vec![2, 2, oop(0)]);
        primitive_method(&mut image, 93, 1, &[16, 124]);
        let e = execute_root(image, ROOT, true)?;
        let valid = matches!(argument, 334 | 2);
        assert_eq!(e.status, 1);
        assert_eq!(e.roots[27], r(if valid { argument } else { 312 }));
        assert_eq!(
            e.result,
            if valid {
                r(RECEIVER)
            } else {
                target::value(argument)?
            }
        );
        assert_eq!(e.allocations, u64::from(!valid));
        if !valid {
            let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
            assert_eq!(fallback.receiver, r(RECEIVER));
            assert_eq!(fallback.slots, vec![target::value(argument)?]);
        }
    }
    Ok(())
}

#[test]
fn asynchronous_signal_preempts_or_wakes_a_different_process_from_idle() -> Result<()> {
    for idle in [false, true] {
        let (bytes, literals) = if idle {
            (
                vec![32, 33, 226, 135, 36, 211, 135, 0, 124],
                vec![RECEIVER, 312, SELECTOR, 116, 334],
            )
        } else {
            // Continue ordinary bytecodes until the higher-priority waiter wakes.
            (
                vec![32, 33, 226, 135, 0, 119, 182, 168, 2, 163, 249, 0, 124],
                vec![RECEIVER, 312, SELECTOR],
            )
        };
        let mut image = scheduler_fixture(
            &bytes,
            &literals,
            if idle { 3 } else { 1 },
            if idle { 1 } else { 3 },
        );
        primitive_method(&mut image, 93, 1, &[116, 124]);
        pointers(&mut image, 312, 38, vec![306, 306, oop(0)]);
        pointers(&mut image, 334, 38, vec![2, 2, oop(0)]);
        pointers(
            &mut image,
            306,
            320,
            vec![2, 308, oop(if idle { 1 } else { 3 }), 312],
        );
        method(
            &mut image,
            310,
            0,
            0,
            &[334, 132, 306, 124],
            &[119, 96, 32, 209, 135, 34, 211, 135, 116, 124],
        );
        let mut context = vec![2, oop(11), oop(0), 310, 2, RECEIVER];
        context.resize(18, 2);
        pointers(&mut image, 308, 22, context);
        let mut device = rekursiv_sim::device::Device::default();
        let events = rekursiv_sim::device::Events::default();
        let mut input = rekursiv_sim::device::Input::default();
        input.schedule.insert(
            200_000,
            vec![input_packet(
                rekursiv_sim::device::InputKind::KeyDown,
                65,
                0,
                1000,
            )],
        );
        device.events = Some(events);
        device.input = Some(input);
        device.timing = Timing {
            request_delay: 5,
            memory_latency: 9,
            response_stall: 0,
        };
        let e = execute_with_device(image, ROOT, true, 512, |_| {}, device)?;
        assert_eq!(
            (e.status, e.result, e.allocations),
            (1, i(2), 1),
            "idle={idle}, acks={:?}, last={:?}",
            e.event_acknowledgements,
            e.trace.last()
        );
        assert_eq!(e.idle_visits > 0, idle);
        assert!(e.collections > 0);
        assert_eq!(e.event_acknowledgements, [1, 0, 0, 0]);
        assert_eq!(e.roots[31], Word::ZERO);
        assert_eq!(e.records[&r(300).identity()?].body[2], r(304));
        assert_eq!(e.records[&r(304).identity()?].body[2], r(2));
        assert_eq!(e.records[&r(304).identity()?].body[4], r(2));
        assert_eq!(e.records[&r(312).identity()?].body[1..], [r(2), r(2), i(3)]);
        assert_eq!(
            e.records[&r(306).identity()?].body[4],
            r(if idle { 314 } else { 2 })
        );
        let transitions: Vec<_> =
            e.trace
                .iter()
                .map(|f| f.reference)
                .fold(Vec::new(), |mut v, r| {
                    if v.last() != Some(&r) {
                        v.push(r);
                    }
                    v
                });
        assert_eq!(transitions, vec![r(ROOT), r(308), r(ROOT)]);
    }
    Ok(())
}

fn clock_device(utc: u64, monotonic: u64, divider: u64) -> rekursiv_sim::device::Device {
    let mut device = rekursiv_sim::device::Device::default();
    device.events = Some(Default::default());
    device.clocks = Some(rekursiv_sim::device::Clocks::new(utc, monotonic, divider));
    device.timing = Timing {
        request_delay: 4,
        memory_latency: 7,
        response_stall: 0,
    };
    device
}

#[test]
fn clock_primitives_pack_unsigned_values_and_preserve_trailing_bytes() -> Result<()> {
    for primitive in [98, 99] {
        for value in [0, 0x1234_5678, 0xffff_ffff, 0x1_0000_0001] {
            let mut image = fixture(&[112, 32, 225, 124], &[220, SELECTOR]);
            image.objects.insert(
                220,
                source::Object {
                    oop: 220,
                    class: 14,
                    body: source::Body::Bytes(vec![0xaa; 5]),
                },
            );
            primitive_method(&mut image, primitive, 1, &[16, 124]);
            let e = execute_with_device(
                image,
                ROOT,
                true,
                512,
                |_| {},
                clock_device(value, value, 1_000_000_000),
            )?;
            assert_eq!((e.status, e.result, e.allocations), (1, r(RECEIVER), 0));
            let expected =
                (value as u32).wrapping_add(if primitive == 98 { 2_177_452_800 } else { 0 });
            let mut bytes = expected
                .to_le_bytes()
                .map(|b| Word::raw(u64::from(b)).unwrap())
                .to_vec();
            bytes.push(Word::raw(0xaa)?);
            assert_eq!(&e.records[&r(220).identity()?].body[1..], bytes);
            assert_eq!(e.device_requests.len(), 1);
            assert!(e.collections > 0);
        }
    }
    Ok(())
}

#[test]
fn clock_primitives_reject_invalid_buffers_before_device_or_guest_writes() -> Result<()> {
    for primitive in [98, 99] {
        for body in [
            source::Body::Bytes(vec![7; 3]),
            source::Body::Words(vec![7; 4]),
            source::Body::Pointers(vec![oop(7); 4]),
        ] {
            let mut image = fixture(&[112, 32, 225, 124], &[220, SELECTOR]);
            image.objects.insert(
                220,
                source::Object {
                    oop: 220,
                    class: 14,
                    body,
                },
            );
            let converted = target::Image::convert(&image, &[ROOT])?;
            let original = converted
                .records
                .iter()
                .find(|r| r.source_oop == 220)
                .unwrap()
                .body
                .clone();
            primitive_method(&mut image, primitive, 1, &[16, 124]);
            let e = execute_with_device(image, ROOT, true, 512, |_| {}, clock_device(0, 0, 1000))?;
            assert_eq!((e.status, e.result, e.allocations), (1, r(220), 1));
            assert_eq!(e.records[&r(220).identity()?].body, original);
            assert!(e.device_requests.is_empty());
        }
    }
    Ok(())
}

#[test]
fn timer_expiry_wakes_guest_once_including_modular_deadline_rollover() -> Result<()> {
    for (now, deadline, expected_high, idle) in [
        (1000, 1400u32, 0, true),
        (1000, 900, 0, false),
        (0xffff_ff00, 0x100, 1, true),
    ] {
        let mut image = scheduler_fixture(
            &[32, 33, 34, 243, 135, 33, 212, 135, 119, 124],
            &[RECEIVER, 312, 220, SELECTOR, 116],
            1,
            3,
        );
        primitive_method(&mut image, 100, 2, &[116, 124]);
        image.objects.insert(
            220,
            source::Object {
                oop: 220,
                class: 14,
                body: source::Body::Bytes(deadline.to_le_bytes().to_vec()),
            },
        );
        let e = execute_with_device(image, ROOT, true, 512, |_| {}, clock_device(0, now, 1000))?;
        assert_eq!(
            (e.status, e.result, e.allocations),
            (1, i(2), 0),
            "now={now}, deadline={deadline}"
        );
        assert_eq!(e.idle_visits > 0, idle);
        assert!(e.collections > 0);
        assert_eq!(e.event_acknowledgements, [0, 1, 0, 0]);
        assert_eq!(e.roots[28], r(2));
        assert_eq!(e.roots[31], Word::ZERO);
        assert_eq!(e.timer_deadline, None);
        assert_eq!(e.records[&r(312).identity()?].body[1..], [r(2), r(2), i(0)]);
        assert!(e
            .device_requests
            .iter()
            .any(|r| r.write && r.address == 0x214 && r.data == expected_high));
    }
    Ok(())
}

#[test]
fn timer_replacement_cancellation_and_failure_preserve_the_right_registration() -> Result<()> {
    // 0 replaces; 1 cancels; 2 rejects the semaphore; 3 rejects a word buffer;
    // 4 rejects a malformed byte component before touching the old timer.
    for case in 0..5 {
        let replacement = match case {
            1 => 2,
            2 => RECEIVER,
            _ => 334,
        };
        let mut code = vec![32, 33, 34, 243, 135, 32, 36, 37, 243];
        if case == 0 {
            code.extend([135, 36, 214, 135, 119, 124]);
        } else {
            code.push(124);
        }
        let mut image = scheduler_fixture(
            &code,
            &[RECEIVER, 312, 220, SELECTOR, replacement, 222, 116],
            1,
            3,
        );
        primitive_method(&mut image, 100, 2, &[16, 124]);
        pointers(&mut image, 334, 38, vec![2, 2, oop(0)]);
        for (id, deadline) in [(220, 5000u32), (222, 1400)] {
            image.objects.insert(
                id,
                source::Object {
                    oop: id,
                    class: 14,
                    body: if case == 3 && id == 222 {
                        source::Body::Words(vec![0; 4])
                    } else {
                        source::Body::Bytes(deadline.to_le_bytes().to_vec())
                    },
                },
            );
        }
        let e = execute_with_device(
            image,
            ROOT,
            true,
            512,
            |image| {
                if case == 4 {
                    image
                        .records
                        .iter_mut()
                        .find(|r| r.source_oop == 222)
                        .unwrap()
                        .body[1] = Word::raw(256).unwrap();
                }
            },
            clock_device(0, 1000, 1000),
        )?;
        assert_eq!(e.status, 1, "case {case}");
        assert_eq!(e.allocations, u64::from(case >= 2));
        assert_eq!(e.roots[28], r(if case >= 2 { 312 } else { 2 }));
        assert_eq!(e.timer_deadline, if case >= 2 { Some(5000) } else { None });
        assert_eq!(e.event_acknowledgements, [0, u64::from(case == 0), 0, 0]);
        assert_eq!(e.records[&r(312).identity()?].body[1..], [r(2), r(2), i(0)]);
        if case == 0 {
            assert!(e.idle_visits > 0);
            assert_eq!(e.result, i(2));
            assert_eq!(e.records[&r(334).identity()?].body[1..], [r(2), r(2), i(0)]);
        } else {
            assert_eq!(e.result, r(if case >= 2 { replacement } else { RECEIVER }));
            assert_eq!(e.device_requests.len(), if case == 1 { 7 } else { 6 });
        }
    }
    Ok(())
}

#[test]
fn simultaneous_input_and_timer_notifications_signal_both_registered_semaphores() -> Result<()> {
    let mut image = scheduler_fixture(
        &[
            32, 33, 226, 135, 32, 35, 36, 245, 135, 33, 214, 135, 35, 214, 135, 119, 124,
        ],
        &[RECEIVER, 312, SELECTOR, 334, 220, 132, 116],
        1,
        3,
    );
    primitive_method(&mut image, 93, 1, &[116, 124]);
    method(
        &mut image,
        336,
        7,
        2,
        &[oop((2 << 8) | 100), 216],
        &[116, 124],
    );
    dictionary(
        &mut image,
        CLASS,
        2,
        DICT,
        METHODS,
        &[(SELECTOR, CALLEE), (132, 336)],
    );
    pointers(&mut image, 334, 38, vec![2, 2, oop(0)]);
    image.objects.insert(
        220,
        source::Object {
            oop: 220,
            class: 14,
            body: source::Body::Bytes(1400u32.to_le_bytes().to_vec()),
        },
    );
    let mut device = clock_device(0, 1000, 1000);
    // Tick 399999 completes the 400th millisecond and expires the timer.
    // The independent input source arrives on that same device edge.
    let mut input = rekursiv_sim::device::Input::default();
    input.schedule.insert(
        399_999,
        vec![input_packet(
            rekursiv_sim::device::InputKind::KeyDown,
            65,
            0,
            1400,
        )],
    );
    device.input = Some(input);
    let e = execute_with_device(image, ROOT, true, 512, |_| {}, device)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 1));
    assert!(e.collections > 0 && e.idle_visits > 0);
    assert_eq!(e.event_acknowledgements, [1, 1, 0, 0]);
    assert_eq!(e.roots[27], r(312));
    assert_eq!(e.roots[28], r(2));
    for semaphore in [312, 334] {
        assert_eq!(
            e.records[&r(semaphore).identity()?].body[1..],
            [r(2), r(2), i(if semaphore == 312 { 3 } else { 0 })]
        );
    }
    assert!(e
        .device_requests
        .iter()
        .any(|r| r.address == 0x100 && !r.write));
    Ok(())
}

fn unsigned_guest_result(execution: &Execution) -> u64 {
    if let Ok((2, payload)) = execution.result.compact_parts() {
        return payload as u64;
    }
    let result = &execution.records[&execution.result.identity().unwrap()];
    assert_eq!(result.class, r(28));
    result.body[1..]
        .iter()
        .enumerate()
        .fold(0, |n, (i, b)| n | (b.bits() << (8 * i)))
}

#[test]
fn system_capacity_primitives_return_full_hardware_counts_as_guest_integers() -> Result<()> {
    for memory in [512, 65536] {
        let mut image = fixture(&[112, 208, 124], &[SELECTOR]);
        primitive_method(&mut image, 112, 0, &[116, 124]);
        let e = execute_sized(image, ROOT, true, memory, |_| {})?;
        assert_eq!(e.status, 1);
        assert_eq!(e.capacity_samples.len(), 1);
        let (primitive, count) = e.capacity_samples[0];
        assert_eq!(primitive, 112);
        assert_eq!(unsigned_guest_result(&e), count);
        assert!(count < (memory / 2) as u64);
        assert_eq!(e.allocations, u64::from(count > 16383));
        assert!(e.collections > 0);
    }
    for remaining in [
        0,
        1,
        16383,
        16384,
        0xffff_ffff,
        0x1_0000_0000,
        0x1f_ffff_0000,
    ] {
        let mut image = fixture(&[112, 208, 124], &[SELECTOR]);
        primitive_method(&mut image, 115, 0, &[116, 124]);
        let e = execute_prepared(image, ROOT, true, |image| {
            image.next_identity = (1 << 37) - remaining
        })?;
        assert_eq!(e.status, 1);
        assert_eq!(e.capacity_samples, [(115, remaining)]);
        assert_eq!(unsigned_guest_result(&e), remaining);
        assert_eq!(e.allocations, u64::from(remaining > 16383));
        assert!(e.collections > 0);
    }
    Ok(())
}

#[test]
fn system_quit_and_debugger_save_context_and_debugger_resumes_once() -> Result<()> {
    for (primitive, resume) in [(113, false), (114, false), (114, true)] {
        let mut image = fixture(&[112, 208, 135, 119, 96, 120], &[SELECTOR]);
        primitive_method(&mut image, primitive, 0, &[116, 124]);
        let e = execute_machine(image, ROOT, true, 512, |_| {}, Default::default(), resume)?;
        assert_eq!(e.status, if resume { 1 } else { primitive as u32 - 103 });
        assert_eq!(e.result, r(RECEIVER));
        assert_eq!(e.debugger_breaks, usize::from(primitive == 114));
        assert_eq!(e.service_stop, primitive == 114 && !resume);
        assert_eq!(e.allocations, 0);
        assert!(e.collections > 0);
        assert_eq!(
            e.records[&r(RECEIVER).identity()?].body[1],
            i(if resume { 2 } else { 7 })
        );
        if !resume {
            let context = &e.records[&r(ROOT).identity()?].body;
            assert_eq!(context[2], i(7));
            assert_eq!(context[3], i(1));
            assert_eq!(context[7], r(RECEIVER));
        }
    }
    Ok(())
}

fn positive_bytes(image: &mut source::Image, id: u16, value: u64) {
    let mut bytes = value.to_le_bytes().to_vec();
    while bytes.len() > 1 && bytes.last() == Some(&0) {
        bytes.pop();
    }
    image.objects.insert(
        id,
        source::Object {
            oop: id,
            class: 28,
            body: source::Body::Bytes(bytes),
        },
    );
}

#[test]
fn low_space_uses_full_width_thresholds_and_signals_only_after_strict_crossing() -> Result<()> {
    let remaining = 0x1_0000_0020;
    for (allocate_after, word_limit) in [(false, 0), (true, 0), (false, 0xffff_ffff)] {
        let mut code = vec![112, 32, 33, 34, 132, 3, 3, 135];
        if allocate_after {
            code.extend([36, 213, 135]);
        }
        code.extend([119, 124]);
        let mut image = scheduler_fixture(&code, &[312, 220, 222, SELECTOR, CLASS, 132], 1, 3);
        primitive_method(&mut image, 116, 3, &[16, 124]);
        method(&mut image, 336, 7, 0, &[oop(70), 216], &[116, 124]);
        dictionary(&mut image, META, 2, 338, 340, &[(132, 336)]);
        positive_bytes(&mut image, 220, remaining - 1);
        positive_bytes(&mut image, 222, word_limit);
        let e = execute_prepared(image, ROOT, true, |image| {
            image.next_identity = (1 << 37) - remaining
        })?;
        assert_eq!((e.status, e.result), (1, i(2)));
        assert_eq!(e.allocations, 1 + u64::from(allocate_after));
        let signaled = allocate_after || word_limit != 0;
        assert_eq!(
            e.records[&r(312).identity()?].body[1..],
            [r(2), r(2), i(i32::from(signaled))]
        );
        assert!(e.collections > 0);
        assert!(
            e.device_requests.is_empty(),
            "low-space decisions must not call an external device"
        );
        if signaled {
            assert_eq!(e.roots[30], r(2));
        } else {
            let registration = &e.records[&e.roots[30].identity()?];
            assert_eq!(registration.class, r(16));
            assert_eq!(
                registration.body,
                vec![Word::raw(56)?, r(312), i(31), i(0), i(16), i(0), i(0), i(0)]
            );
        }
    }
    Ok(())
}

#[test]
fn low_space_signal_preempts_at_a_completed_bytecode_boundary() -> Result<()> {
    let mut image = scheduler_fixture(
        &[112, 32, 33, 34, 132, 3, 3, 135, 0, 124],
        &[312, oop(0), 220, SELECTOR],
        1,
        3,
    );
    primitive_method(&mut image, 116, 3, &[16, 124]);
    positive_bytes(&mut image, 220, 0xffff_ffff);
    pointers(&mut image, 312, 38, vec![306, 306, oop(0)]);
    pointers(&mut image, 306, 320, vec![2, 308, oop(3), 312]);
    method(
        &mut image,
        310,
        0,
        0,
        &[306, 124],
        &[119, 96, 32, 209, 135, 116, 124],
    );
    let mut context = vec![2, oop(7), oop(0), 310, 2, RECEIVER];
    context.resize(18, 2);
    pointers(&mut image, 308, 22, context);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 1));
    assert!(e.collections > 0);
    assert_eq!(e.roots[30], r(2));
    assert_eq!(e.records[&r(312).identity()?].body[1..], [r(2), r(2), i(0)]);
    let mut contexts = e.trace.iter().map(|f| f.reference).collect::<Vec<_>>();
    contexts.dedup();
    assert_eq!(contexts, vec![r(ROOT), r(308), r(ROOT)]);
    assert!(e.device_requests.is_empty());
    Ok(())
}

#[test]
fn low_space_delivery_is_one_shot_and_a_new_registration_rearms_it() -> Result<()> {
    let mut image = scheduler_fixture(
        &[
            112, 32, 33, 34, 132, 3, 3, 135, 112, 32, 33, 34, 132, 3, 3, 135, 119, 124,
        ],
        &[312, oop(0), 220, SELECTOR],
        1,
        3,
    );
    primitive_method(&mut image, 116, 3, &[16, 124]);
    positive_bytes(&mut image, 220, 0xffff_ffff);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 2));
    assert_eq!(e.roots[30], r(2));
    assert_eq!(e.records[&r(312).identity()?].body[1..], [r(2), r(2), i(2)]);
    Ok(())
}

#[test]
fn low_space_replacement_cancellation_and_invalid_arguments_are_atomic() -> Result<()> {
    let cases = [
        (334, oop(0), 228),
        (2, 2, 2),
        (RECEIVER, oop(0), oop(0)),
        (334, oop(-1), oop(0)),
        (334, 220, oop(0)),
        (334, 224, oop(0)),
        (334, 226, oop(0)),
        (334, RECEIVER, oop(0)),
        (334, 230, oop(0)),
        (334, oop(0), oop(-1)),
        (334, oop(0), 222),
    ];
    for (case, (semaphore, ids, words)) in cases.into_iter().enumerate() {
        let mut image = scheduler_fixture(
            &[
                112, 32, 33, 34, 132, 3, 3, 135, 112, 36, 37, 38, 132, 3, 3, 124,
            ],
            &[312, oop(0), oop(0), SELECTOR, semaphore, ids, words],
            1,
            3,
        );
        primitive_method(&mut image, 116, 3, &[16, 124]);
        pointers(&mut image, 334, 38, vec![2, 2, oop(0)]);
        for (id, value) in [
            (220, 1 << 37),
            (222, 1 << 32),
            (226, 1 << 40),
            (228, 0xffff_ffff),
            (230, 0),
        ] {
            positive_bytes(&mut image, id, value);
        }
        image.objects.insert(
            224,
            source::Object {
                oop: 224,
                class: 28,
                body: source::Body::Bytes(vec![]),
            },
        );
        let e = execute_prepared(image, ROOT, true, |image| {
            if ids == 230 {
                image
                    .records
                    .iter_mut()
                    .find(|r| r.source_oop == 230)
                    .unwrap()
                    .body[1] = Word::raw(256).unwrap();
            }
        })?;
        assert_eq!(e.status, 1, "case {case}");
        assert_eq!(e.allocations, if case == 1 { 1 } else { 2 }, "case {case}");
        assert_eq!(e.records[&r(312).identity()?].body[3], i(0));
        assert_eq!(
            e.records[&r(334).identity()?].body[3],
            i(i32::from(case == 0))
        );
        if case <= 1 {
            assert_eq!(e.roots[30], r(2));
            assert_eq!(e.result, r(RECEIVER));
        } else {
            let registration = &e.records[&e.roots[30].identity()?];
            assert_eq!(registration.body[1], r(312));
            assert!(registration.body[2..].iter().all(|&v| v == i(0)));
            assert_eq!(e.result, r(semaphore));
            let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
            assert_eq!(
                fallback.slots,
                vec![r(semaphore), target::value(ids)?, target::value(words)?]
            );
        }
        assert!(e.device_requests.is_empty());
    }
    Ok(())
}

#[test]
fn low_space_copies_mutable_thresholds_and_retains_registration_through_repeated_gc() -> Result<()>
{
    // Mutation of an original LargePositiveInteger does not change its copied
    // threshold. With fewer than 255 identities left, retaining the argument
    // instead of its value would cause an unintended signal after the write.
    let mut image = scheduler_fixture(
        &[
            112, 32, 33, 34, 132, 3, 3, 135, 33, 118, 36, 245, 135, 119, 124,
        ],
        &[312, 220, oop(0), SELECTOR, oop(255), 132],
        1,
        3,
    );
    primitive_method(&mut image, 116, 3, &[16, 124]);
    positive_bytes(&mut image, 220, 0);
    method(
        &mut image,
        336,
        7,
        2,
        &[oop((2 << 8) | 61), 216],
        &[116, 124],
    );
    dictionary(&mut image, 28, 2, 338, 340, &[(132, 336)]);
    if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&28).unwrap().body {
        fields[2] = 0x2001;
    }
    let e = execute_prepared(image, ROOT, true, |image| {
        image.next_identity = (1 << 37) - 100
    })?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 1));
    assert_eq!(e.records[&r(220).identity()?].body[1], Word::raw(255)?);
    assert_eq!(e.records[&r(312).identity()?].body[3], i(0));
    assert!(e.records[&e.roots[30].identity()?].body[2..]
        .iter()
        .all(|&v| v == i(0)));

    let bytes = [
        112, 32, 33, 34, 132, 3, 3, 135, 36, 104, 37, 214, 135, 16, 118, 177, 104, 16, 117, 182,
        168, 2, 163, 242, 119, 124,
    ];
    let literals = [312, oop(0), oop(0), SELECTOR, oop(80), CLASS, 132];
    let mut image = scheduler_fixture(&bytes, &literals, 1, 3);
    method(&mut image, CALLER, 0, 1, &literals, &bytes);
    if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&ROOT).unwrap().body {
        fields[2] = oop(1);
    }
    primitive_method(&mut image, 116, 3, &[16, 124]);
    method(&mut image, 336, 7, 0, &[oop(70), 216], &[116, 124]);
    dictionary(&mut image, META, 2, 338, 340, &[(132, 336)]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 81));
    assert!(e.collections >= 2);
    let registration = &e.records[&e.roots[30].identity()?];
    assert_eq!(registration.class, r(16));
    assert_eq!(registration.body[1], r(312));
    assert!(registration.body[2..].iter().all(|&v| v == i(0)));
    assert_eq!(e.records[&r(312).identity()?].body[3], i(0));
    Ok(())
}

#[test]
fn become_exchanges_classes_sizes_and_aliases_then_survives_collection() -> Result<()> {
    let bytes = [
        32, 33, 226, 135, 37, 104, 35, 212, 135, 16, 118, 177, 104, 16, 117, 182, 168, 2, 163, 242,
        32, 199, 124,
    ];
    let literals = [220, 222, SELECTOR, CLASS, 116, oop(40)];
    let mut image = block_fixture(&bytes, &literals);
    method(&mut image, CALLER, 0, 1, &literals, &bytes);
    if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&ROOT).unwrap().body {
        fields[2] = oop(1);
    }
    if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&48).unwrap().body {
        fields[(199 - 176) * 2 + 1] = oop(0);
    }
    pointers(&mut image, 220, CLASS, vec![oop(7), 220]);
    pointers(&mut image, 222, SUPER, vec![oop(9)]);
    primitive_method(&mut image, 72, 1, &[116, 124]);
    method(&mut image, 114, 7, 0, &[oop(70), 216], &[116, 124]);
    dictionary(&mut image, META, 2, 212, 214, &[(116, 114)]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result), (1, r(SUPER)));
    assert!(e.recovery_after_exchange);
    let a = &e.records[&r(220).identity()?];
    let b = &e.records[&r(222).identity()?];
    assert_eq!((a.class, &a.body), (r(SUPER), &vec![Word::raw(8)?, i(9)]));
    assert_eq!(
        (b.class, &b.body),
        (r(CLASS), &vec![Word::raw(16)?, i(7), r(220)])
    );
    Ok(())
}

#[test]
fn stream_primitives_advance_only_on_success_and_use_separate_limits() -> Result<()> {
    // Result, final position, and collection contents are checked independently.
    // A read sees readLimit=1, while a write can fill both collection elements.
    for (primitive, position, result, advanced) in [
        (65, 0, r(RECEIVER), true),
        (65, 1, i(-1), false),
        (65, -1, i(-1), false),
        (65, 2, i(-1), false),
        (66, 0, i(2), true),
        (66, 1, i(2), true),
        (66, 2, i(-1), false),
        (67, 0, r(4), false),
        (67, 1, r(6), false),
        (67, 2, r(6), false),
    ] {
        let args = if primitive == 66 {
            vec![oop(2)]
        } else {
            vec![]
        };
        let mut image = indexed_fixture(
            primitive,
            source::Body::Pointers(vec![222, oop(position), oop(1), oop(2)]),
            0xc009,
            &args,
        );
        pointers(&mut image, 222, 16, vec![RECEIVER, oop(9)]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(
            (e.status, e.result),
            (1, result),
            "primitive {primitive}, position {position}"
        );
        let stream = &e.records[&r(220).identity()?].body;
        assert_eq!(stream[2], i(position + i32::from(advanced)));
        assert_eq!(&stream[3..], &[i(1), i(2)]);
        let mut expected = vec![r(RECEIVER), i(9)];
        if primitive == 66 && advanced {
            expected[position as usize] = i(2);
        }
        assert_eq!(e.records[&r(222).identity()?].body[1..], expected);
        assert!(e.collections > 0 && e.refill_retries > 0);
    }
    Ok(())
}

#[test]
fn string_streams_convert_characters_and_preserve_state_on_bad_writes() -> Result<()> {
    for (primitive, value, success) in [
        (65, 224, true),
        (66, 224, true),
        (66, oop(2), false),
        (66, 226, false),
    ] {
        let args = if primitive == 66 { vec![value] } else { vec![] };
        let mut image = indexed_fixture(
            primitive,
            source::Body::Pointers(vec![222, oop(0), oop(1), oop(1)]),
            0xc009,
            &args,
        );
        image.objects.insert(
            222,
            source::Object {
                oop: 222,
                class: 14,
                body: source::Body::Bytes(vec![65]),
            },
        );
        pointers(&mut image, 224, 40, vec![oop(65)]);
        pointers(&mut image, 226, 40, vec![oop(256)]);
        let mut table = vec![2; 66];
        table[65] = 224;
        pointers(&mut image, 50, 16, table);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(
            (e.status, e.result),
            (1, if success { r(224) } else { i(-1) })
        );
        assert_eq!(
            e.records[&r(220).identity()?].body[2],
            i(i32::from(success))
        );
        assert_eq!(e.records[&r(222).identity()?].body[1], Word::raw(65)?);
    }
    Ok(())
}

fn float_object(image: &mut source::Image, object: u16, bits: u32) {
    image.objects.insert(
        object,
        source::Object {
            oop: object,
            class: 20,
            body: source::Body::Words(vec![(bits >> 16) as u16, bits as u16]),
        },
    );
}
fn float_fixture(number: i32, left: u32, right: Option<u32>) -> source::Image {
    let args = if right.is_some() { vec![222] } else { vec![] };
    let mut image = indexed_fixture(
        number,
        source::Body::Words(vec![(left >> 16) as u16, left as u16]),
        0x4005,
        &args,
    );
    dictionary(&mut image, 20, 2, 230, 232, &[(SELECTOR, CALLEE)]);
    if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&20).unwrap().body {
        fields[2] = 0x4005;
    }
    image.objects.get_mut(&220).unwrap().class = 20;
    if let Some(bits) = right {
        float_object(&mut image, 222, bits);
    }
    image
}
fn float_result_bits(e: &Execution) -> Result<u32> {
    let record = &e.records[&e.result.identity()?];
    assert_eq!(record.class, r(20));
    assert_eq!(record.body[0], Word::raw(17)?);
    Ok(((record.body[1].bits() as u32) << 16) | (record.body[2].bits() as u32))
}
#[test]
fn float_arithmetic_runs_on_numerik_and_allocates_guest_results() -> Result<()> {
    for (prim, a, b) in [
        (41, 1.25f32, 2.5f32),
        (42, -1.25, 2.5),
        (49, -3.0, 0.5),
        (50, 1.0, 3.0),
        (41, 0.0, -0.0),
        (49, f32::MIN_POSITIVE, 0.5),
        (50, -0.0, 2.0),
    ] {
        let image = float_fixture(prim, a.to_bits(), Some(b.to_bits()));
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(e.status, 1);
        let expected = match prim {
            41 => a + b,
            42 => a - b,
            49 => a * b,
            _ => a / b,
        };
        assert_eq!(
            float_result_bits(&e)?,
            expected.to_bits(),
            "primitive {prim}: {a}, {b}"
        );
        assert!(e.collections > 0 && e.refill_retries > 0);
    }
    for (prim, a, b) in [
        (50, 1.0f32, 0.0f32),
        (49, f32::MAX, 2.0),
        (41, f32::NAN, 1.0),
    ] {
        let e = execute(float_fixture(prim, a.to_bits(), Some(b.to_bits())))?;
        assert_eq!((e.status, e.result), (1, i(-1)));
        let fallback = e.trace.iter().find(|t| t.method == r(CALLEE)).unwrap();
        assert_eq!(fallback.receiver, r(220));
        assert_eq!(fallback.slots, vec![r(222)]);
    }
    Ok(())
}
#[test]
fn float_comparisons_truncation_fraction_exponent_and_scaling() -> Result<()> {
    for (a, b) in [(-2.0f32, 1.0f32), (2.0, -1.0), (-0.0, 0.0), (-3.0, -2.0)] {
        for prim in 43..=48 {
            let expected = match prim {
                43 => a < b,
                44 => a > b,
                45 => a <= b,
                46 => a >= b,
                47 => a == b,
                _ => a != b,
            };
            let e = execute(float_fixture(prim, a.to_bits(), Some(b.to_bits())))?;
            assert_eq!((e.status, e.result), (1, r(if expected { 6 } else { 4 })));
        }
    }
    for (value, expected) in [
        (1.75f32, 1),
        (-1.75, -1),
        (-16384.75, -16384),
        (16384.0, -1),
        (1e30, -1),
    ] {
        let e = execute(float_fixture(51, value.to_bits(), None))?;
        assert_eq!((e.status, e.result), (1, i(expected)));
    }
    for value in [1.75f32, -1.75, 1e30, f32::from_bits(1), -0.0] {
        let e = execute_root(float_fixture(52, value.to_bits(), None), ROOT, true)?;
        assert_eq!(
            float_result_bits(&e)?,
            if value.abs() >= 8388608.0 {
                0
            } else {
                (value - value.trunc()).to_bits()
            }
        );
    }
    for (bits, exponent) in [
        (0u32, -1),
        (0x80000000, -1),
        (0x3f800000, 0),
        (0xc0400000, 1),
        (1, -149),
        (0x00800000, -126),
    ] {
        let e = execute(float_fixture(53, bits, None))?;
        assert_eq!((e.status, e.result), (1, i(exponent)));
    }
    for (bits, power, expected) in [
        (0x3fc00000, 2, 0x40c00000),
        (0x80000000, 1000, 0x80000000),
        (1, 149, 0x3f800000),
        (0x00800000, -1, 0x00400000),
        (1, -1, 0),
        (3, -1, 2),
        (0x80000001, -200, 0x80000000),
    ] {
        let mut image = float_fixture(54, bits, Some(0));
        // The scale operand is an integer literal, not a Float.
        if let source::Body::Method { literals, .. } =
            &mut image.objects.get_mut(&CALLER).unwrap().body
        {
            literals[1] = oop(power);
        }
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(
            float_result_bits(&e)?,
            expected,
            "{bits:08x} scaled by {power}"
        );
    }
    Ok(())
}
#[test]
fn small_integer_as_float_uses_generic_signed_conversion() -> Result<()> {
    for value in [-16384, -17, -1, 0, 1, 17, 16383] {
        let mut image = fixture(&[32, 209, 124], &[oop(value), SELECTOR]);
        dictionary(&mut image, 12, 2, 230, 232, &[(SELECTOR, CALLEE)]);
        pointers(&mut image, 20, 60, vec![2, 2, 0x4005]);
        primitive_method(&mut image, 40, 0, &[116, 124]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(float_result_bits(&e)?, (value as f32).to_bits());
    }
    Ok(())
}

fn relocate_fixture_object(image: &mut target::Image, old: Word, new: Word) {
    for record in &mut image.records {
        if record.reference == old {
            record.reference = new;
        }
        if record.class == old {
            record.class = new;
        }
        for word in &mut record.body {
            if *word == old {
                *word = new;
            }
        }
    }
    // Keep allocations away from the test's relocated identity, including the
    // first identity immediately above the imported object's address range.
    image.next_identity = 65536;
}

#[test]
fn identity_numbers_preserve_imported_values_and_all_37_identity_bits() -> Result<()> {
    for id in [
        110,
        16383,
        16384,
        32767,
        32768,
        0xffff_8000,
        0xffff_ffff,
        1 << 32,
        (1 << 37) - 1,
    ] {
        let mut image = fixture(&[32, 209, 124], &[220, SELECTOR]);
        pointers(&mut image, 220, CLASS, vec![]);
        primitive_method(&mut image, 75, 0, &[116, 124]);
        let expected_ref = Word::reference(id, true)?;
        let e = execute_prepared(image, ROOT, true, |image| {
            relocate_fixture_object(image, r(220), expected_ref);
        })?;
        assert_eq!(e.status, 1, "identity {id}");
        if id < 32768 {
            let expected = if id >= 16384 {
                id as i32 - 32768
            } else {
                id as i32
            };
            assert_eq!(e.result, i(expected), "identity {id}");
        } else {
            let record = &e.records[&e.result.identity()?];
            assert_eq!(record.class, r(28));
            let encoded = id + 32768;
            let bytes = (64 - encoded.leading_zeros()).div_ceil(8) as usize;
            assert_eq!(record.body[0], Word::raw((bytes * 4 + 2) as u64)?);
            assert_eq!(record.body.len(), bytes + 1);
            for (shift, value) in record.body[1..].iter().enumerate() {
                assert_eq!(*value, Word::raw((encoded >> (shift * 8)) & 255)?);
            }
        }
        assert!(e.collections > 0);
    }
    Ok(())
}

#[test]
fn identity_conversion_round_trips_new_objects_through_guest_large_integers() -> Result<()> {
    for id in [
        110,
        16384,
        32768,
        0xffff_8000,
        0xffff_ffff,
        1 << 32,
        (1 << 37) - 1,
    ] {
        let mut image = fixture(&[32, 209, 210, 124], &[220, SELECTOR, 116]);
        pointers(&mut image, 220, CLASS, vec![]);
        primitive_method(&mut image, 76, 0, &[116, 124]);
        let mut inverse = image.objects[&CALLEE].clone();
        inverse.oop = 118;
        image.objects.insert(118, inverse);
        dictionary(&mut image, 12, 2, 230, 232, &[(116, 118)]);
        dictionary(&mut image, 28, 2, 234, 236, &[(116, 118)]);
        primitive_method(&mut image, 75, 0, &[116, 124]);
        let expected = Word::reference(id, true)?;
        let e = execute_prepared(image, ROOT, true, |image| {
            relocate_fixture_object(image, r(220), expected);
        })?;
        assert_eq!((e.status, e.result), (1, expected), "identity {id}");
        assert!(!e
            .trace
            .iter()
            .any(|f| f.method == r(118) || f.method == r(CALLEE)));
    }
    Ok(())
}

#[test]
fn identity_conversion_reserves_immediate_codes_and_falls_back_for_absent_objects() -> Result<()> {
    for (encoded, expected) in [
        (32768u64, i(0)),
        (49151, i(16383)),
        (49152, i(-16384)),
        (65535, i(-1)),
        (0, i(-1)),
        (12345, i(-1)),
        ((1 << 37) + 32768, i(-1)),
    ] {
        let mut image = fixture(&[32, 209, 124], &[220, SELECTOR]);
        let mut bytes = encoded.to_le_bytes().to_vec();
        while bytes.len() > 1 && bytes.last() == Some(&0) {
            bytes.pop();
        }
        image.objects.insert(
            220,
            source::Object {
                oop: 220,
                class: 28,
                body: source::Body::Bytes(bytes),
            },
        );
        dictionary(&mut image, 28, 2, 230, 232, &[(SELECTOR, CALLEE)]);
        primitive_method(&mut image, 76, 0, &[116, 124]);
        let e = execute(image)?;
        assert_eq!((e.status, e.result), (1, expected), "encoding {encoded}");
        if !(32768..65536).contains(&encoded) {
            let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
            assert_eq!(fallback.receiver, r(220));
        }
    }
    Ok(())
}

#[test]
fn identity_conversion_rejects_an_empty_large_integer_without_a_memory_fault() -> Result<()> {
    let mut image = fixture(&[32, 209, 124], &[220, SELECTOR]);
    image.objects.insert(
        220,
        source::Object {
            oop: 220,
            class: 28,
            body: source::Body::Bytes(vec![]),
        },
    );
    dictionary(&mut image, 28, 2, 230, 232, &[(SELECTOR, CALLEE)]);
    primitive_method(&mut image, 76, 0, &[116, 124]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(-1), 1));
    assert_eq!(
        e.trace
            .iter()
            .find(|f| f.method == r(CALLEE))
            .unwrap()
            .receiver,
        r(220)
    );
    Ok(())
}

#[test]
fn instance_enumeration_filters_classes_without_loading_candidate_bodies() -> Result<()> {
    for (primitive, receiver, expected) in
        [(77, CLASS, r(220)), (78, 220, r(224)), (78, 226, i(-1))]
    {
        let mut image = fixture(&[32, 209, 124], &[receiver, SELECTOR]);
        image.objects.get_mut(&RECEIVER).unwrap().class = SUPER;
        pointers(&mut image, 220, CLASS, vec![]);
        pointers(&mut image, 222, SUPER, vec![]);
        pointers(&mut image, 224, CLASS, vec![2; 600]);
        pointers(&mut image, 226, CLASS, vec![]);
        if primitive == 77 {
            // The first matching instance is too large to Fetch into the
            // 256-word active semispace. Directory enumeration must not do so.
            pointers(&mut image, 220, CLASS, vec![2; 600]);
            dictionary(&mut image, META, 2, 230, 232, &[(SELECTOR, CALLEE)]);
        }
        primitive_method(&mut image, primitive, 0, &[116, 124]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!(
            (e.status, e.result),
            (1, expected),
            "primitive {primitive}, receiver {receiver}"
        );
        if expected == i(-1) {
            assert_eq!(
                e.trace
                    .iter()
                    .find(|f| f.method == r(CALLEE))
                    .unwrap()
                    .receiver,
                r(receiver)
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn original_large_integer_fallbacks_execute_as_guest_bytecodes() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let bytes = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    ensure!(rekursiv_smalltalk::checksum(&bytes) == rekursiv_smalltalk::IMAGE_SHA256);
    let original = source::Image::parse(&bytes)?;
    let class = original.object(28)?;
    let dictionary = original.object(class.pointer(1)?)?;
    let methods = original.object(dictionary.pointer(1)?)?;
    let source::Body::Pointers(methods) = &methods.body else {
        unreachable!()
    };
    let cases = [
        (21, 65792),
        (22, 65280),
        (23, -2),
        (24, -3),
        (25, -2),
        (26, -3),
        (27, -2),
        (28, -3),
        (29, 16777216),
        (30, 256),
        (31, 0),
        (32, 256),
        (33, 256),
        (34, 0),
        (35, 65792),
        (36, 65792),
        (37, 16777216),
    ];
    for (primitive, expected) in cases {
        eprintln!("checking original LargeInteger primitive {primitive}");
        let (slot, primitive_method) = methods
            .iter()
            .enumerate()
            .find(|(_, method)| {
                original
                    .object(**method)
                    .is_ok_and(|m| m.primitive().ok() == Some(Some(primitive)))
            })
            .unwrap();
        let selector = dictionary.pointer(slot + 2)?;
        let mut image = original.clone();
        for id in [65520, 65522, 65524, 65526] {
            ensure!(!image.objects.contains_key(&id));
        }
        for (id, bytes) in [(65520, vec![0, 0, 1]), (65522, vec![0, 1])] {
            image.objects.insert(
                id,
                source::Object {
                    oop: id,
                    class: 28,
                    body: source::Body::Bytes(bytes),
                },
            );
        }
        let rhs = if primitive == 37 { oop(8) } else { 65522 };
        method(
            &mut image,
            65524,
            0,
            0,
            &[65520, rhs, selector],
            &[32, 33, 226, 124],
        );
        let mut context = vec![2, oop(9), oop(0), 65524, 2, 65520];
        context.resize(18, 2);
        pointers(&mut image, 65526, 22, context);
        let e = execute_sized(image, 65526, true, 65536, |_| {})
            .wrap_err_with(|| format!("LargeInteger primitive {primitive}"))?;
        assert_eq!(e.status, 1, "primitive {primitive}");
        assert!(
            e.trace.iter().any(|f| f.method == r(*primitive_method)),
            "fallback did not run for primitive {primitive}"
        );
        if expected < 0 {
            assert_eq!(
                e.result,
                r(if expected == -2 { 4 } else { 6 }),
                "primitive {primitive}"
            );
        } else if let Ok((2, payload)) = e.result.compact_parts() {
            assert_eq!(payload as i32, expected, "primitive {primitive}");
        } else {
            let result = &e.records[&e.result.identity()?];
            assert_eq!(result.class, r(28), "primitive {primitive}");
            let value = result.body[1..]
                .iter()
                .enumerate()
                .fold(0u64, |n, (i, w)| n | (w.bits() << (8 * i)));
            assert_eq!(value, expected as u64, "primitive {primitive}");
        }
        assert!(e.collections > 0);
    }
    Ok(())
}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn original_byte_replacement_fallbacks_execute_as_guest_bytecodes() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let bytes = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    ensure!(rekursiv_smalltalk::checksum(&bytes) == rekursiv_smalltalk::IMAGE_SHA256);
    let original = source::Image::parse(&bytes)?;
    for (class, source_class, method_oop) in [
        (0x1b4c, 0x1b4c, 0x1b7e),
        (0x1b4c, 14, 0x1b80),
        (14, 14, 0x8894),
        (14, 0x1b4c, 0x889c),
    ] {
        let dictionary = original.object(original.object(class)?.pointer(1)?)?;
        let methods = original.object(dictionary.pointer(1)?)?;
        let source::Body::Pointers(methods) = &methods.body else {
            unreachable!()
        };
        let slot = methods.iter().position(|&m| m == method_oop).unwrap();
        let selector = dictionary.pointer(slot + 2)?;
        let mut image = original.clone();
        for id in [65520, 65522, 65524, 65526] {
            ensure!(!image.objects.contains_key(&id));
        }
        for (id, class, bytes) in [
            (65520, class, b"abcde".to_vec()),
            (65522, source_class, b"XYZ".to_vec()),
        ] {
            image.objects.insert(
                id,
                source::Object {
                    oop: id,
                    class,
                    body: source::Body::Bytes(bytes),
                },
            );
        }
        method(
            &mut image,
            65524,
            0,
            0,
            &[65520, oop(2), oop(4), 65522, oop(1), selector],
            &[32, 33, 34, 35, 36, 132, 4, 5, 124],
        );
        let mut context = vec![2, oop(15), oop(0), 65524, 2, 65520];
        context.resize(18, 2);
        pointers(&mut image, 65526, 22, context);
        let e = execute_sized(image, 65526, true, 65536, |_| {})
            .wrap_err_with(|| format!("replacement method {method_oop:04x}"))?;
        assert_eq!((e.status, e.result), (1, r(65520)));
        assert_eq!(
            e.records[&r(65520).identity()?].body[1..],
            b"aXYZe".map(|b| Word::raw(b as u64).unwrap())
        );
        assert!(e.trace.iter().any(|f| f.method == r(method_oop)));
        assert!(e.collections > 0);
    }
    Ok(())
}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn original_character_scanning_fallback_measures_without_display() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let bytes = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    ensure!(rekursiv_smalltalk::checksum(&bytes) == rekursiv_smalltalk::IMAGE_SHA256);
    let original = source::Image::parse(&bytes)?;
    let class = 0x13f4; // CharacterScanner, from the distribution's class.oops
    let dictionary = original.object(original.object(class)?.pointer(1)?)?;
    let source::Body::Pointers(methods) = &original.object(dictionary.pointer(1)?)?.body else {
        unreachable!()
    };
    let (slot, &method_oop) = methods
        .iter()
        .enumerate()
        .find(|(_, method)| {
            original
                .object(**method)
                .is_ok_and(|m| m.primitive().ok() == Some(Some(103)))
        })
        .unwrap();
    let selector = dictionary.pointer(slot + 2)?;
    // Each glyph is two pixels wide. Cover end-of-run, right-edge crossing,
    // character stop, and an empty interval without invoking BitBlt/display.
    for (start, stop, right, stop_at_b, expected, dest_x, last_index, source_x, width) in [
        (1, 3, 16, false, 777, 16, 3, 134, 2),
        (1, 3, 13, false, 888, 12, 2, 132, 2),
        (1, 3, 16, true, 999, 12, 2, 130, 2),
        (2, 1, 16, false, 777, 10, 1, 0, 0),
    ] {
        let mut image = original.clone();
        for id in [65520, 65522, 65524, 65526, 65528, 65530] {
            ensure!(!image.objects.contains_key(&id));
        }
        let mut scanner = vec![2; 27];
        scanner[4] = oop(10); // BitBlt destX
        scanner[6] = oop(0); // width
        scanner[8] = oop(0); // sourceX
        scanner[14] = oop(0); // CharacterScanner lastIndex
        scanner[15] = 65528; // xTable
        scanner[16] = 65530; // stopConditions
        pointers(&mut image, 65520, class, scanner);
        pointers(
            &mut image,
            65528,
            16,
            (0..257).map(|n| oop(n * 2)).collect(),
        );
        let mut stops = vec![2; 258];
        stops[256] = oop(777);
        stops[257] = oop(888);
        if stop_at_b {
            stops[b'B' as usize] = oop(999);
        }
        pointers(&mut image, 65530, 16, stops);
        image.objects.insert(
            65522,
            source::Object {
                oop: 65522,
                class: 14,
                body: source::Body::Bytes(b"ABC".to_vec()),
            },
        );
        method(
            &mut image,
            65524,
            0,
            0,
            &[
                65520,
                oop(start),
                oop(stop),
                65522,
                oop(right),
                65530,
                4,
                selector,
            ],
            &[32, 33, 34, 35, 36, 37, 38, 132, 6, 7, 124],
        );
        let mut context = vec![2, oop(19), oop(0), 65524, 2, 65520];
        context.resize(18, 2);
        pointers(&mut image, 65526, 22, context);
        let e = execute_sized(image, 65526, true, 65536, |_| {})?;
        assert_eq!((e.status, e.result), (1, i(expected)));
        assert!(e.trace.iter().any(|f| f.method == r(method_oop)));
        assert!(e.collections > 0);
        let scanner = &e.records[&r(65520).identity()?].body[1..];
        assert_eq!(scanner[4], i(dest_x));
        assert_eq!(scanner[6], i(width));
        assert_eq!(scanner[8], i(source_x));
        assert_eq!(scanner[14], i(last_index));
    }
    Ok(())
}

fn pointer_device(linked: bool) -> rekursiv_sim::device::Device {
    let mut device = rekursiv_sim::device::Device::default();
    let mut pointer = rekursiv_sim::device::Pointer::default();
    pointer.mouse = (640, 480);
    pointer.cursor = (12, 34);
    pointer.linked = linked;
    pointer.sample_interval_ms = 10;
    device.pointer = Some(pointer);
    device.timing = Timing {
        request_delay: 3,
        memory_latency: 5,
        response_stall: 0,
    };
    device
}

#[test]
fn pointer_polling_constructs_points_and_rejects_unrepresentable_coordinates() -> Result<()> {
    for (x, y) in [
        (640, 480),
        (0, 0),
        (-16384, 16383),
        (16384, 0),
        (-16385, 0),
        (0, 16384),
        (0, -16385),
    ] {
        let mut image = fixture(&[112, 208, 124], &[SELECTOR]);
        primitive_method(&mut image, 90, 0, &[116, 124]);
        let mut device = pointer_device(false);
        device.pointer.as_mut().unwrap().mouse = (x, y);
        let e = execute_with_device(image, ROOT, true, 512, |_| {}, device)?;
        let valid = (-16384..=16383).contains(&x) && (-16384..=16383).contains(&y);
        assert_eq!(
            (e.status, e.allocations, e.device_requests.len()),
            (1, 1, 2)
        );
        assert!(e.collections > 0);
        assert_eq!(e.trace.iter().any(|f| f.method == r(CALLEE)), !valid);
        if valid {
            let point = &e.records[&e.result.identity()?];
            assert_eq!(point.class, r(26));
            assert_eq!(point.body, [Word::raw(16)?, i(x), i(y)]);
        } else {
            assert_eq!(e.result, i(-1));
        }
    }
    Ok(())
}

#[test]
fn pointer_position_validates_points_before_atomic_device_publication() -> Result<()> {
    for linked in [false, true] {
        for case in 0..5 {
            let mut image = fixture(&[112, 32, 225, 124], &[220, SELECTOR]);
            let fields = match case {
                1 => vec![oop(7)],
                2 => vec![2, oop(9)],
                _ => vec![oop(-16384), oop(16383)],
            };
            pointers(&mut image, 220, if case == 3 { CLASS } else { 26 }, fields);
            primitive_method(&mut image, 91, 1, &[16, 124]);
            let e = execute_with_device(
                image,
                ROOT,
                true,
                512,
                |image| {
                    if case == 4 {
                        image
                            .records
                            .iter_mut()
                            .find(|r| r.source_oop == 220)
                            .unwrap()
                            .body[1] = i(16384);
                    }
                },
                pointer_device(linked),
            )?;
            assert_eq!(e.status, 1);
            if case == 0 {
                assert_eq!((e.result, e.allocations), (r(RECEIVER), 0));
                assert_eq!(e.cursor, Some((-16384, 16383)));
                assert_eq!(
                    e.mouse,
                    Some(if linked { (-16384, 16383) } else { (640, 480) })
                );
                assert_eq!(
                    e.device_requests
                        .iter()
                        .map(|r| r.address)
                        .collect::<Vec<_>>(),
                    [0x400, 0x404, 0x408]
                );
            } else {
                assert_eq!((e.result, e.allocations), (r(220), 1));
                assert_eq!(e.cursor, Some((12, 34)));
                assert_eq!(e.mouse, Some((640, 480)));
                assert!(e.device_requests.is_empty());
            }
        }
    }
    Ok(())
}

#[test]
fn pointer_link_and_sample_interval_validate_operands_before_device_writes() -> Result<()> {
    for (primitive, argument, valid) in [
        (92, 6, true),
        (92, 4, true),
        (92, 2, false),
        (92, oop(1), false),
        (94, oop(0), true),
        (94, oop(1), true),
        (94, oop(16383), true),
        (94, oop(-1), false),
        (94, 2, false),
        (94, 220, false),
    ] {
        let mut image = fixture(&[112, 32, 225, 124], &[argument, SELECTOR]);
        positive_bytes(&mut image, 220, 0);
        primitive_method(&mut image, primitive, 1, &[16, 124]);
        let e = execute_with_device(image, ROOT, true, 512, |_| {}, pointer_device(false))?;
        let arg = if argument & 1 != 0 {
            i((argument as i16 >> 1) as i32)
        } else {
            r(argument)
        };
        assert_eq!(
            (e.status, e.result, e.allocations),
            (1, if valid { r(RECEIVER) } else { arg }, u64::from(!valid))
        );
        assert_eq!(e.device_requests.len(), usize::from(valid));
        let linked = primitive == 92 && argument == 6;
        assert_eq!(e.cursor_linked, Some(linked));
        assert_eq!(e.cursor, Some(if linked { (640, 480) } else { (12, 34) }));
        assert_eq!(
            e.sample_interval,
            Some(if primitive == 94 && valid {
                u32::from(argument >> 1)
            } else {
                10
            })
        );
    }
    Ok(())
}

#[test]
fn pointer_primitives_reject_wrong_arity_without_device_access() -> Result<()> {
    for primitive in [90, 91, 92, 94] {
        let args = i32::from(primitive == 90);
        let mut image = if args == 0 {
            fixture(&[112, 208, 124], &[SELECTOR])
        } else {
            fixture(&[112, 32, 225, 124], &[oop(7), SELECTOR])
        };
        primitive_method(&mut image, primitive, args, &[116, 124]);
        let e = execute_with_device(image, ROOT, true, 512, |_| {}, pointer_device(false))?;
        assert_eq!((e.status, e.result, e.allocations), (1, i(-1), 1));
        assert!(e.device_requests.is_empty());
        assert_eq!(e.mouse, Some((640, 480)));
        assert_eq!(e.cursor, Some((12, 34)));
        assert_eq!(e.cursor_linked, Some(false));
        assert_eq!(e.sample_interval, Some(10));
    }
    Ok(())
}

fn input_packet(
    kind: rekursiv_sim::device::InputKind,
    value: i32,
    extra: i32,
    timestamp_ms: u32,
) -> rekursiv_sim::device::InputPacket {
    rekursiv_sim::device::InputPacket {
        kind,
        value,
        extra,
        timestamp_ms,
    }
}
fn input_device(packets: Vec<rekursiv_sim::device::InputPacket>) -> rekursiv_sim::device::Device {
    let mut device = clock_device(0, 0, 1_000_000_000);
    let mut input = rekursiv_sim::device::Input::default();
    input.schedule.insert(0, packets);
    device.input = Some(input);
    device
}

#[test]
fn buffered_input_converts_packets_wraps_the_ring_and_falls_back_when_empty() -> Result<()> {
    use rekursiv_sim::device::InputKind::*;
    let packets = vec![
        input_packet(Motion, 291, 1110, 0x1234abcd),
        input_packet(KeyDown, 65, 0, 0xffff4321),
        input_packet(KeyUp, 4095, 0, 0xffffffff),
        input_packet(Motion, -7, 7000, 0),
        input_packet(KeyDown, 0, 0, 65536),
    ];
    let expected: Vec<u16> = vec![
        0x5000, 0x1234, 0xabcd, 0x1123, 0x2456, 0x5000, 0xffff, 0x4321, 0x3041, 0x5000, 0xffff,
        0xffff, 0x4fff, 0x5000, 0, 0, 0x1000, 0x2fff, 0x5000, 1, 0, 0x3000,
    ];
    let mut code = vec![];
    for _ in &expected {
        code.extend([112, 208, 135]);
    }
    // One more read must activate fallback, without inventing another word.
    code.extend([112, 208, 124]);
    let mut image = fixture(&code, &[SELECTOR]);
    primitive_method(&mut image, 95, 0, &[116, 124]);
    let e = execute_with_device(image, ROOT, true, 512, |_| {}, input_device(packets))?;
    assert_eq!((e.status, e.result), (1, i(-1)));
    for (n, &expected) in expected.iter().enumerate() {
        let frame = e
            .trace
            .iter()
            .find(|f| f.method == r(CALLER) && f.ip == 7 + 3 * n as i32)
            .unwrap();
        assert_eq!(frame.unsigned_top, Some(u64::from(expected)), "word {n}");
    }
    let buffer = &e.records[&e.roots[29].identity()?];
    assert_eq!(&buffer.body[2..5], &[i(6), i(0), i(0)]);
    assert!(buffer.body[5..37].iter().all(|&w| w == i(0)));
    assert_eq!(e.event_acknowledgements, [5, 0, 0, 0]);
    assert_eq!(e.input_remaining, Some(0));
    assert!(
        e.collections > 1,
        "collection must recur after the initial refill: {}",
        e.collections
    );
    Ok(())
}

#[test]
fn buffered_input_empty_and_wrong_arity_preserve_send_operands() -> Result<()> {
    for args in [0, 1] {
        let mut image = if args == 0 {
            fixture(&[112, 208, 124], &[SELECTOR])
        } else {
            fixture(&[112, 32, 225, 124], &[oop(7), SELECTOR])
        };
        primitive_method(&mut image, 95, args, &[116, 124]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!((e.status, e.result, e.allocations), (1, i(-1), 1));
        assert_eq!(e.roots[29], r(2));
        assert!(e.device_requests.is_empty());
    }
    Ok(())
}

#[test]
fn buffered_input_retains_unsignalled_words_until_registration_but_excludes_polled_words(
) -> Result<()> {
    let mut image = scheduler_fixture(
        &[112, 209, 135, 112, 32, 226, 124],
        &[312, 132, SELECTOR],
        1,
        3,
    );
    primitive_method(&mut image, 93, 1, &[116, 124]);
    method(&mut image, 336, 7, 0, &[oop(95), 216], &[116, 124]);
    dictionary(
        &mut image,
        CLASS,
        2,
        DICT,
        METHODS,
        &[(SELECTOR, CALLEE), (132, 336)],
    );
    let e = execute_with_device(
        image,
        ROOT,
        true,
        512,
        |_| {},
        input_device(vec![input_packet(
            rekursiv_sim::device::InputKind::KeyDown,
            65,
            0,
            1000,
        )]),
    )?;
    assert_eq!((e.status, e.result, e.allocations), (1, r(RECEIVER), 2));
    assert_eq!(e.records[&r(312).identity()?].body[3], i(3));
    let buffer = &e.records[&e.roots[29].identity()?];
    assert_eq!(&buffer.body[2..5], &[i(1), i(3), i(0)]);
    assert_eq!(e.event_acknowledgements, [1, 0, 0, 0]);
    Ok(())
}

#[test]
fn buffered_input_sampling_uses_the_guest_interval_and_wakes_an_idle_process() -> Result<()> {
    let mut image = scheduler_fixture(
        &[112, 32, 225, 135, 112, 34, 227, 135, 32, 212, 135, 119, 124],
        &[312, SELECTOR, oop(2), 132, 116],
        1,
        3,
    );
    primitive_method(&mut image, 93, 1, &[116, 124]);
    method(
        &mut image,
        336,
        7,
        1,
        &[oop((1 << 8) | 94), 216],
        &[116, 124],
    );
    dictionary(
        &mut image,
        CLASS,
        2,
        DICT,
        METHODS,
        &[(SELECTOR, CALLEE), (132, 336)],
    );
    let mut device = input_device(vec![]);
    device.clocks = Some(rekursiv_sim::device::Clocks::new(0, 0, 1000));
    let mut pointer = rekursiv_sim::device::Pointer::default();
    pointer.sample_interval_ms = 1000; // The primitive must lower this to 2.
    pointer.schedule.insert(200_000, (320, 240));
    device.pointer = Some(pointer);
    let e = execute_with_device(image, ROOT, true, 512, |_| {}, device)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 1));
    assert!(e.idle_visits > 0 && e.collections > 0);
    assert_eq!(e.sample_interval, Some(2));
    assert_eq!(e.mouse, Some((320, 240)));
    assert_eq!(e.records[&r(312).identity()?].body[3], i(4));
    let buffer = &e.records[&e.roots[29].identity()?];
    assert_eq!(&buffer.body[2..5], &[i(0), i(5), i(0)]);
    let words: Vec<u32> = buffer.body[5..15]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| p[0].bits() as u32 | ((p[1].bits() as u32) << 14))
        .collect();
    assert_eq!(words, [0x5000, 0, 200, 0x1140, 0x20f0]);
    assert_eq!(e.event_acknowledgements, [1, 0, 0, 0]);
    Ok(())
}

#[test]
fn buffered_input_pressure_does_not_starve_timer_delivery() -> Result<()> {
    let mut image = scheduler_fixture(
        &[112, 32, 33, 242, 135, 32, 211, 135, 119, 124],
        &[312, 220, SELECTOR, 116],
        1,
        3,
    );
    primitive_method(&mut image, 100, 2, &[116, 124]);
    image.objects.insert(
        220,
        source::Object {
            oop: 220,
            class: 14,
            body: source::Body::Bytes(200u32.to_le_bytes().to_vec()),
        },
    );
    let packet = input_packet(rekursiv_sim::device::InputKind::KeyDown, 65, 0, 100);
    let mut device = input_device(vec![packet; 5]);
    device.clocks = Some(rekursiv_sim::device::Clocks::new(0, 0, 1000));
    let e = execute_with_device(image, ROOT, true, 512, |_| {}, device)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 1));
    assert!(e.idle_visits > 0);
    assert_eq!(e.event_acknowledgements, [3, 1, 0, 0]);
    assert_eq!(e.input_remaining, Some(2));
    assert_eq!(e.input_overruns, Some(0));
    let buffer = &e.records[&e.roots[29].identity()?];
    assert_eq!(&buffer.body[2..5], &[i(0), i(12), i(12)]);
    Ok(())
}

#[test]
fn buffered_input_reports_physical_overrun_without_consuming_unread_packets() -> Result<()> {
    let mut image = fixture(&[112, 208, 124], &[SELECTOR]);
    primitive_method(&mut image, 95, 0, &[116, 124]);
    let packet = input_packet(rekursiv_sim::device::InputKind::KeyDown, 65, 0, 100);
    let e = execute_with_device(
        image,
        ROOT,
        true,
        512,
        |_| {},
        input_device(vec![packet; 33]),
    )?;
    assert_eq!(e.status, 5);
    assert_eq!(e.allocations, 0);
    assert_eq!(e.input_remaining, Some(32));
    assert_eq!(e.input_overruns, Some(1));
    assert_eq!(e.event_acknowledgements, [0; 4]);
    assert_eq!(e.roots[29], r(2));
    Ok(())
}

#[test]
fn buffered_input_wrong_arity_preserves_queued_words() -> Result<()> {
    let mut image = fixture(&[112, 32, 225, 124], &[oop(7), SELECTOR]);
    primitive_method(&mut image, 95, 1, &[16, 124]);
    let e = execute_with_device(
        image,
        ROOT,
        true,
        512,
        |_| {},
        input_device(vec![input_packet(
            rekursiv_sim::device::InputKind::KeyUp,
            65,
            0,
            1000,
        )]),
    )?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(7), 2));
    let buffer = &e.records[&e.roots[29].identity()?];
    assert_eq!(&buffer.body[2..5], &[i(0), i(4), i(4)]);
    assert_eq!(e.event_acknowledgements, [1, 0, 0, 0]);
    Ok(())
}

#[test]
fn buffered_input_rejects_unsupported_key_codes_before_packet_consumption() -> Result<()> {
    for code in [-1, 4096, i32::MAX] {
        let mut image = fixture(&[112, 208, 124], &[SELECTOR]);
        primitive_method(&mut image, 95, 0, &[116, 124]);
        let e = execute_with_device(
            image,
            ROOT,
            true,
            512,
            |_| {},
            input_device(vec![input_packet(
                rekursiv_sim::device::InputKind::KeyDown,
                code,
                0,
                1000,
            )]),
        )?;
        assert_eq!(e.status, 5);
        assert_eq!(e.input_remaining, Some(1));
        assert_eq!(e.event_acknowledgements, [0; 4]);
        let buffer = &e.records[&e.roots[29].identity()?];
        assert_eq!(&buffer.body[2..5], &[i(0), i(0), i(0)]);
    }
    Ok(())
}

fn bitmap_device() -> rekursiv_sim::device::Device {
    let mut device = rekursiv_sim::device::Device::default();
    device.cursor_bitmap = Some(rekursiv_sim::device::Bitmap::new(16, 16));
    device.display_bitmap = Some(rekursiv_sim::device::Bitmap::new(128, 128));
    device.timing = Timing {
        request_delay: 4,
        memory_latency: 7,
        response_stall: 0,
    };
    device
}
fn bitmap_fixture(primitive: i32, width: i32, height: i32, words: Vec<u16>) -> source::Image {
    let mut image = fixture(&[112, 208, 124], &[SELECTOR]);
    pointers(
        &mut image,
        RECEIVER,
        CLASS,
        vec![220, oop(width), oop(height), 2],
    );
    image.objects.insert(
        220,
        source::Object {
            oop: 220,
            class: 16,
            body: source::Body::Words(words),
        },
    );
    primitive_method(&mut image, primitive, 0, &[116, 124]);
    image
}
#[test]
fn bitmap_registration_packs_rows_masks_padding_and_roots_the_form() -> Result<()> {
    for (primitive, width, height) in [
        (101, 16, 16),
        (102, 1, 3),
        (102, 15, 3),
        (102, 16, 3),
        (102, 17, 3),
        (102, 31, 3),
        (102, 32, 3),
        (102, 33, 3),
        (102, 63, 3),
        (102, 64, 3),
        (102, 65, 3),
    ] {
        let stride16 = (width + 15) / 16;
        let stride32 = (width + 31) / 32;
        let words: Vec<u16> = (0..stride16 * height + 2)
            .map(|n| 0x8421u16.rotate_left(n as u32 % 16))
            .collect();
        let mut expected = vec![0u32; (stride32 * height) as usize];
        // Pixel-by-pixel expectation is independent of the microcode's pairing.
        for y in 0..height {
            for x in 0..width {
                if words[(y * stride16 + x / 16) as usize] & (1 << (15 - x % 16)) != 0 {
                    expected[(y * stride32 + x / 32) as usize] |= 1 << (31 - x % 32);
                }
            }
        }
        let image = bitmap_fixture(primitive, width, height, words.clone());
        let e = execute_with_device(image, ROOT, true, 512, |_| {}, bitmap_device())?;
        assert_eq!(
            (e.status, e.result, e.allocations),
            (1, r(RECEIVER), 1),
            "primitive {primitive}, {width}x{height}"
        );
        let frame = if primitive == 101 {
            e.cursor_frame.as_ref()
        } else {
            e.display_frame.as_ref()
        }
        .unwrap();
        assert_eq!(
            (frame.width, frame.height, frame.stride),
            (width as u32, height as u32, stride32 as u32)
        );
        assert_eq!(frame.words, expected, "{width}x{height}");
        assert_eq!(
            e.bitmap_publications,
            if primitive == 101 { [1, 0] } else { [0, 1] }
        );
        let state = &e.records[&e.roots[29].identity()?];
        assert_eq!(
            state.body[if primitive == 101 { 37 } else { 38 }],
            r(RECEIVER)
        );
        assert_eq!(
            e.records[&r(220).identity()?].body[1..]
                .iter()
                .map(|w| w.bits() as u16)
                .collect::<Vec<_>>(),
            words
        );
        assert!(e.collections > 0);
    }
    Ok(())
}

#[test]
fn bitmap_registration_rejects_invalid_forms_before_replacing_the_current_bitmap() -> Result<()> {
    for primitive in [101, 102] {
        for case in 0..17 {
            let mut image = bitmap_fixture(primitive, 16, 16, vec![0xaaaa; 16]);
            method(
                &mut image,
                CALLER,
                0,
                0,
                &[224, SELECTOR],
                &[112, 209, 135, 32, 209, 124],
            );
            // The fixture's root IP depends on the number of method literals.
            if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&ROOT).unwrap().body
            {
                fields[1] = oop(7);
            }
            let (width, height) = match case {
                0 => (0, 16),
                1 => (-1, 16),
                2 => (16, 0),
                3 => (16, -1),
                13 => (if primitive == 101 { 8 } else { 129 }, 16),
                14 => (16, if primitive == 101 { 8 } else { 129 }),
                _ => (16, 16),
            };
            let mut fields = vec![226, oop(width), oop(height), 2];
            if case == 4 {
                fields[1] = 2;
            }
            if case == 5 {
                fields[2] = 4;
            }
            if case == 7 {
                fields.truncate(2);
            }
            pointers(&mut image, 224, CLASS, fields);
            if case == 16 {
                image.objects.get_mut(&224).unwrap().body = source::Body::Words(vec![0; 4]);
            }
            let words = vec![
                0x5555;
                if case == 10 {
                    15
                } else {
                    ((width.max(1) + 15) / 16 * height.max(1)) as usize
                }
            ];
            image.objects.insert(
                226,
                source::Object {
                    oop: 226,
                    class: 16,
                    body: match case {
                        8 => source::Body::Bytes(vec![0; 16]),
                        9 => source::Body::Pointers(vec![2; 16]),
                        _ => source::Body::Words(words),
                    },
                },
            );
            let e = execute_with_device(
                image,
                ROOT,
                true,
                1024,
                |image| {
                    if case == 6 {
                        image
                            .records
                            .iter_mut()
                            .find(|r| r.source_oop == 224)
                            .unwrap()
                            .body[2] = i(16384);
                    }
                    if [11, 12, 15].contains(&case) {
                        image
                            .records
                            .iter_mut()
                            .find(|r| r.source_oop == 226)
                            .unwrap()
                            .body[16] = match case {
                            11 => Word::raw(65536).unwrap(),
                            12 => i(1),
                            _ => Word::raw(1 << 32).unwrap(),
                        };
                    }
                },
                bitmap_device(),
            )?;
            assert_eq!(
                (e.status, e.result, e.allocations),
                (1, i(-1), 2),
                "primitive {primitive} case {case}"
            );
            let frame = if primitive == 101 {
                e.cursor_frame.as_ref()
            } else {
                e.display_frame.as_ref()
            }
            .unwrap();
            assert_eq!(frame.words, vec![0xaaaa0000; 16]);
            assert_eq!(
                e.bitmap_publications,
                if primitive == 101 { [1, 0] } else { [0, 1] }
            );
            let state = &e.records[&e.roots[29].identity()?];
            assert_eq!(
                state.body[if primitive == 101 { 37 } else { 38 }],
                r(RECEIVER)
            );
            assert_eq!(e.device_requests.iter().filter(|r| r.write).count(), 21);
        }
    }
    Ok(())
}

#[test]
fn bitmap_registration_rejects_wrong_arity_and_missing_devices() -> Result<()> {
    for primitive in [101, 102] {
        for args in [0, 1] {
            let mut image = bitmap_fixture(primitive, 16, 16, vec![0; 16]);
            if args == 1 {
                method(&mut image, CALLER, 0, 0, &[SELECTOR], &[112, 118, 224, 124]);
                primitive_method(&mut image, primitive, 1, &[16, 124]);
            }
            let device = if args == 0 {
                rekursiv_sim::device::Device::default()
            } else {
                bitmap_device()
            };
            let e = execute_with_device(image, ROOT, true, 512, |_| {}, device)?;
            assert_eq!(
                (e.status, e.result, e.allocations),
                (1, i(if args == 0 { -1 } else { 1 }), 1)
            );
            assert_eq!(e.roots[29], r(2));
            assert_eq!(e.bitmap_publications, [0, 0]);
            assert_eq!(e.device_requests.len(), usize::from(args == 0));
            assert!(e.device_requests.iter().all(|r| !r.write));
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn original_cursor_and_display_registration_execute_on_rtl() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let bytes = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    ensure!(rekursiv_smalltalk::checksum(&bytes) == rekursiv_smalltalk::IMAGE_SHA256);
    let original = source::Image::parse(&bytes)?;
    for (class, primitive) in [(0x2e8, 101), (0x342, 102)] {
        let form = original
            .objects
            .values()
            .find(|o| o.class == class)
            .unwrap();
        let width = layout::small_integer(form.pointer(1)?)? as usize;
        let height = layout::small_integer(form.pointer(2)?)? as usize;
        let source::Body::Words(words) = &original.object(form.pointer(0)?)?.body else {
            panic!("original bitmap is not words")
        };
        let dictionary = original.object(original.object(class)?.pointer(1)?)?;
        let source::Body::Pointers(methods) = &original.object(dictionary.pointer(1)?)?.body else {
            unreachable!()
        };
        let (slot, _) = methods
            .iter()
            .enumerate()
            .find(|(_, m)| {
                original
                    .object(**m)
                    .is_ok_and(|o| o.primitive().ok() == Some(Some(primitive)))
            })
            .unwrap();
        let selector = dictionary.pointer(slot + 2)?;
        let mut image = original.clone();
        for id in [65524, 65526] {
            ensure!(!image.objects.contains_key(&id));
        }
        method(
            &mut image,
            65524,
            0,
            0,
            &[form.oop, selector],
            &[32, 209, 124],
        );
        let mut context = vec![2, oop(7), oop(0), 65524, 2, form.oop];
        context.resize(18, 2);
        pointers(&mut image, 65526, 22, context);
        let mut device = bitmap_device();
        device.display_bitmap = Some(rekursiv_sim::device::Bitmap::new(4096, 4096));
        let e = execute_with_device(image, 65526, true, 65536, |_| {}, device)?;
        assert_eq!((e.status, e.result), (1, r(form.oop)));
        let frame = if primitive == 101 {
            e.cursor_frame.as_ref()
        } else {
            e.display_frame.as_ref()
        }
        .unwrap();
        assert_eq!(
            (frame.width as usize, frame.height as usize),
            (width, height)
        );
        for y in 0..height {
            for x in 0..width {
                assert_eq!(
                    (frame.words[y * frame.stride as usize + x / 32] >> (31 - x % 32)) & 1,
                    u32::from((words[y * width.div_ceil(16) + x / 16] >> (15 - x % 16)) & 1)
                );
            }
        }
        assert_eq!(
            e.bitmap_publications,
            if primitive == 101 { [1, 0] } else { [0, 1] }
        );
        assert!(e.collections > 0);
    }
    Ok(())
}

fn serial_bytes(image: &mut source::Image, id: u16, bytes: &[u8]) {
    image.objects.insert(
        id,
        source::Object {
            oop: id,
            class: 14,
            body: source::Body::Bytes(bytes.to_vec()),
        },
    );
}
fn snapshot_target(e: &Execution) -> &Record {
    let state = &e.records[&e.roots[29].identity().unwrap()];
    &e.records[&state.body[39].identity().unwrap()]
}
#[test]
fn snapshot_target_copies_serial_bytes_and_full_unsigned_leader_addresses() -> Result<()> {
    for leader in [0, 16383, 16384, 32768, 65535] {
        let argument = if leader <= 16383 { oop(leader) } else { 222 };
        let mut image = fixture(&[112, 32, 33, 242, 124], &[220, argument, SELECTOR]);
        serial_bytes(&mut image, 220, &[0, 255, 128, 171]);
        positive_bytes(&mut image, 222, leader as u64);
        primitive_method(&mut image, 135, 2, &[16, 124]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!((e.status, e.result, e.allocations), (1, r(RECEIVER), 2));
        assert_eq!(snapshot_target(&e).class, r(16));
        assert_eq!(
            snapshot_target(&e).body,
            [
                Word::raw(48)?,
                i(0),
                i(255),
                i(128),
                i(171),
                i(leader & 16383),
                i(leader >> 14)
            ]
        );
        assert!(e.device_requests.is_empty());
        assert!(e.collections > 0);
    }
    Ok(())
}

#[test]
fn snapshot_target_replacement_rejects_bad_values_without_changing_the_old_target() -> Result<()> {
    for case in 0..17 {
        let serial = if case == 5 { oop(7) } else { 224 };
        let leader = if case == 9 { oop(-1) } else { 226 };
        let mut image = fixture(
            &[112, 32, 33, 244, 135, 112, 34, 35, 244, 124],
            &[220, 222, serial, leader, SELECTOR],
        );
        serial_bytes(&mut image, 220, &[10, 20, 30, 40]);
        serial_bytes(
            &mut image,
            224,
            match case {
                1 => &[1, 2, 3],
                2 => &[1, 2, 3, 4, 5],
                _ => &[1, 2, 3, 4],
            },
        );
        positive_bytes(&mut image, 222, 16384);
        positive_bytes(
            &mut image,
            226,
            match case {
                10 => 65536,
                11 => 1 << 32,
                _ => 65535,
            },
        );
        match case {
            3 => image.objects.get_mut(&224).unwrap().body = source::Body::Words(vec![0; 2]),
            4 => image.objects.get_mut(&224).unwrap().body = source::Body::Pointers(vec![2; 4]),
            12 => image.objects.get_mut(&226).unwrap().body = source::Body::Bytes(vec![]),
            13 => image.objects.get_mut(&226).unwrap().class = 14,
            15 => image.objects.get_mut(&226).unwrap().body = source::Body::Bytes(vec![0; 6]),
            _ => {}
        }
        primitive_method(&mut image, 135, 2, &[16, 124]);
        let e = execute_prepared(image, ROOT, true, |image| {
            if [6, 7, 8].contains(&case) {
                image
                    .records
                    .iter_mut()
                    .find(|r| r.source_oop == 224)
                    .unwrap()
                    .body[4] = match case {
                    6 => i(1),
                    7 => Word::raw(256).unwrap(),
                    _ => Word::raw(1 << 32).unwrap(),
                };
            }
            if case == 14 {
                image
                    .records
                    .iter_mut()
                    .find(|r| r.source_oop == 226)
                    .unwrap()
                    .body[1] = i(1);
            }
            if case == 16 {
                // Replace the leader literal with a noncanonical compact value.
                image
                    .records
                    .iter_mut()
                    .find(|r| r.source_oop == CALLER)
                    .unwrap()
                    .body[5] = i(16384);
            }
        })?;
        assert_eq!(e.status, 1, "case {case}");
        assert_eq!(e.allocations, 3, "case {case}");
        assert_eq!(
            e.result,
            if case == 0 {
                r(RECEIVER)
            } else {
                target::value(serial)?
            },
            "case {case}"
        );
        assert_eq!(
            snapshot_target(&e).body[1..],
            if case == 0 {
                [i(1), i(2), i(3), i(4), i(16383), i(3)]
            } else {
                [i(10), i(20), i(30), i(40), i(0), i(1)]
            },
            "case {case}"
        );
        assert!(e.device_requests.is_empty());
    }
    Ok(())
}

#[test]
fn snapshot_target_rejects_wrong_arity_before_allocating_or_touching_devices() -> Result<()> {
    for args in [0, 1, 3] {
        let mut image = match args {
            0 => fixture(&[112, 208, 124], &[SELECTOR]),
            1 => fixture(&[112, 32, 225, 124], &[220, SELECTOR]),
            _ => fixture(
                &[112, 32, 33, 34, 132, 3, 3, 124],
                &[220, oop(1), oop(0), SELECTOR],
            ),
        };
        serial_bytes(&mut image, 220, &[1, 2, 3, 4]);
        primitive_method(&mut image, 135, args, &[116, 124]);
        let e = execute_root(image, ROOT, true)?;
        assert_eq!((e.status, e.result, e.allocations), (1, i(-1), 1));
        assert_eq!(e.roots[29], r(2));
        assert!(e.device_requests.is_empty());
    }
    Ok(())
}

#[test]
fn snapshot_target_copies_mutable_arguments_and_survives_repeated_collection() -> Result<()> {
    let code = [
        112, 32, 33, 242, 135, 32, 118, 35, 244, 135, 33, 118, 35, 244, 135, 37, 104, 38, 215, 135,
        16, 118, 177, 104, 16, 117, 182, 168, 2, 163, 242, 119, 124,
    ];
    let literals = [220, 222, SELECTOR, oop(255), 132, oop(80), CLASS, 116];
    let mut image = fixture(&code, &literals);
    method(&mut image, CALLER, 0, 1, &literals, &code);
    if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&ROOT).unwrap().body {
        fields[2] = oop(1);
    }
    serial_bytes(&mut image, 220, &[0, 255, 128, 171]);
    positive_bytes(&mut image, 222, 32768);
    primitive_method(&mut image, 135, 2, &[116, 124]);
    method(
        &mut image,
        228,
        7,
        2,
        &[oop((2 << 8) | 61), 216],
        &[116, 124],
    );
    for (class, dict, methods) in [(14, 230, 232), (28, 234, 236)] {
        dictionary(&mut image, class, 2, dict, methods, &[(132, 228)]);
        if let source::Body::Pointers(fields) = &mut image.objects.get_mut(&class).unwrap().body {
            fields[2] = 0x2001;
        }
    }
    method(&mut image, 336, 7, 0, &[oop(70), 216], &[116, 124]);
    dictionary(&mut image, META, 2, 338, 340, &[(116, 336)]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(2), 82));
    assert!(e.collections >= 2);
    assert_eq!(e.records[&r(220).identity()?].body[1], Word::raw(255)?);
    assert_eq!(e.records[&r(222).identity()?].body[1], Word::raw(255)?);
    assert_eq!(
        &snapshot_target(&e).body[1..],
        &[i(0), i(255), i(128), i(171), i(0), i(2)]
    );
    assert!(e.device_requests.is_empty());
    Ok(())
}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn original_alto_snapshot_registration_executes_on_rtl() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let bytes = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    ensure!(rekursiv_smalltalk::checksum(&bytes) == rekursiv_smalltalk::IMAGE_SHA256);
    let mut image = source::Image::parse(&bytes)?;
    let class = 0x2e0a; // AltoFile, from the distribution's class.oops.
    let dictionary = image.object(image.object(class)?.pointer(1)?)?;
    let source::Body::Pointers(methods) = &image.object(dictionary.pointer(1)?)?.body else {
        unreachable!()
    };
    let slot = methods
        .iter()
        .position(|&m| {
            image
                .object(m)
                .is_ok_and(|m| m.primitive().ok() == Some(Some(135)))
        })
        .unwrap();
    let selector = dictionary.pointer(slot + 2)?;
    for id in [65520, 65522, 65524, 65526, 65528] {
        ensure!(!image.objects.contains_key(&id));
    }
    pointers(&mut image, 65520, class, vec![2; 11]);
    image.objects.insert(
        65522,
        source::Object {
            oop: 65522,
            class: 0x1b4c,
            body: source::Body::Bytes(vec![0x12, 0x34, 0x56, 0x78]),
        },
    );
    positive_bytes(&mut image, 65528, 65535);
    method(
        &mut image,
        65524,
        0,
        0,
        &[65520, 65522, 65528, selector],
        &[32, 33, 34, 243, 124],
    );
    let mut context = vec![2, oop(11), oop(0), 65524, 2, 65520];
    context.resize(18, 2);
    pointers(&mut image, 65526, 22, context);
    let e = execute_sized(image, 65526, true, 65536, |_| {})?;
    assert_eq!((e.status, e.result, e.allocations), (1, r(65520), 2));
    assert_eq!(
        &snapshot_target(&e).body[1..],
        &[i(0x12), i(0x34), i(0x56), i(0x78), i(16383), i(3)]
    );
    assert!(e.device_requests.is_empty());
    assert!(e.collections > 0);
    Ok(())
}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn original_alto_disk_primitive_reports_unavailable_storage_through_guest_fallback() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let bytes = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    ensure!(rekursiv_smalltalk::checksum(&bytes) == rekursiv_smalltalk::IMAGE_SHA256);
    let mut image = source::Image::parse(&bytes)?;
    let dictionary = image.object(image.object(0x2e0a)?.pointer(1)?)?;
    let source::Body::Pointers(methods) = &image.object(dictionary.pointer(1)?)?.body else {
        unreachable!()
    };
    let slot = methods.iter().position(|&m| m == 0x2fca).unwrap();
    let selector = dictionary.pointer(slot + 2)?;
    for id in [65520, 65522, 65524, 65526, 65528, 65530] {
        ensure!(!image.objects.contains_key(&id));
    }
    pointers(&mut image, 65520, 0x2e0a, vec![2; 11]);
    pointers(&mut image, 65528, 38, vec![2, 2, oop(0)]);
    positive_bytes(&mut image, 65530, 18496); // Alto CRR command, encoded as a guest integer.
    image.objects.insert(
        65522,
        source::Object {
            oop: 65522,
            class: 0x1b4c,
            body: source::Body::Bytes(vec![0x5a; 528]),
        },
    );
    method(
        &mut image,
        65524,
        0,
        0,
        &[65520, oop(0), oop(0), 65530, 65522, 65528, selector],
        &[32, 33, 34, 35, 36, 37, 132, 5, 6, 124],
    );
    let mut context = vec![2, oop(17), oop(0), 65524, 2, 65520];
    context.resize(18, 2);
    pointers(&mut image, 65526, 22, context);
    let e = execute_sized(image, 65526, true, 65536, |_| {})?;
    assert_eq!((e.status, e.result), (1, r(65520)));
    let fallback = e.trace.iter().find(|f| f.method == r(0x2fca)).unwrap();
    assert_eq!(fallback.receiver, r(65520));
    assert_eq!(fallback.slots, [i(0), i(0), r(65530), r(65522), r(65528)]);
    let file = &e.records[&r(65520).identity()?].body;
    assert_eq!(file[8], i(-1)); // File>>error, guest field 7.
    assert!(file[1..]
        .iter()
        .enumerate()
        .all(|(n, &v)| v == if n == 7 { i(-1) } else { r(2) }));
    assert!(e.records[&r(65522).identity()?].body[1..]
        .iter()
        .all(|w| w.bits() == 0x5a));
    assert_eq!(
        e.records[&r(65528).identity()?].body[1..],
        [r(2), r(2), i(0)]
    );
    assert!(e.device_requests.is_empty());
    assert_eq!(e.event_acknowledgements, [0; 4]);
    assert!(e.collections > 0);
    Ok(())
}

#[test]
fn three_argument_equality_primitive_shell_preserves_all_arguments_for_fallback() -> Result<()> {
    // The pinned image declares Object>>tryPrimitive3:with:with: with primitive
    // 7, although integer equality takes one argument. Exercise its declared
    // arity with a valid SmallInteger receiver so type rejection cannot mask it.
    let mut image = fixture(&[118, 118, 119, 32, 132, 3, 1, 124], &[oop(3), SELECTOR]);
    primitive_method(&mut image, 7, 3, &[18, 124]);
    dictionary(&mut image, 12, 2, 220, 222, &[(SELECTOR, CALLEE)]);
    let e = execute_root(image, ROOT, true)?;
    assert_eq!((e.status, e.result, e.allocations), (1, i(3), 1));
    let fallback = e.trace.iter().find(|f| f.method == r(CALLEE)).unwrap();
    assert_eq!(fallback.receiver, i(1));
    assert_eq!(fallback.slots, [i(1), i(2), i(3)]);
    Ok(())
}
