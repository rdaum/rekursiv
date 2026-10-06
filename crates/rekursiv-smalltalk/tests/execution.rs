//! Guest expectations use source oops and byte offsets. Only the test runner
//! knows this model: it never supplies a value or next state to the processor.
use eyre::{ensure, Result, WrapErr};
use rekursiv_model::processor::{Image as Program, Processor};
use rekursiv_sim::{runtime, Harness, Timing};
use rekursiv_smalltalk::{interpreter, layout, source, target};
use std::collections::BTreeMap;

const CONTEXT: u16 = 100;
const METHOD: u16 = 102;
const RECEIVER: u16 = 104;
const ASSOCIATION: u16 = 106;

fn fixture(bytes: &[u8], literals: &[u16], temps: &[u16]) -> source::Image {
    let mut objects = BTreeMap::new();
    for oop in [12, 16, 22, 34, 52, 54, 56, 60] {
        objects.insert(
            oop,
            source::Object {
                oop,
                class: 60,
                body: source::Body::Pointers(vec![2, 2, 0xc007]),
            },
        );
    }
    for (oop, class) in [(2, 52), (4, 54), (6, 56)] {
        objects.insert(
            oop,
            source::Object {
                oop,
                class,
                body: source::Body::Pointers(vec![]),
            },
        );
    }
    objects.insert(
        48,
        source::Object {
            oop: 48,
            class: 16,
            body: source::Body::Pointers((0..32).flat_map(|_| [ASSOCIATION, 3]).collect()),
        },
    );
    let header =
        layout::MethodHeader(0x81 | ((literals.len() as u16) << 1) | ((temps.len() as u16) << 8));
    let mut context = vec![
        2,
        layout::integer_oop(header.initial_ip() as i32).unwrap(),
        layout::integer_oop(temps.len() as i32).unwrap(),
        METHOD,
        2,
        RECEIVER,
    ];
    context.extend(temps);
    context.resize(6 + 32, 2);
    for (oop, fields) in [
        (CONTEXT, context),
        (RECEIVER, vec![1, 3, 5, 2]),
        (ASSOCIATION, vec![2, 7]),
    ] {
        objects.insert(
            oop,
            source::Object {
                oop,
                class: if oop == CONTEXT { 22 } else { 16 },
                body: source::Body::Pointers(fields),
            },
        );
    }
    objects.insert(
        METHOD,
        source::Object {
            oop: METHOD,
            class: 34,
            body: source::Body::Method {
                header,
                literals: literals.to_vec(),
                bytes: bytes.to_vec(),
            },
        },
    );
    source::Image {
        sha256: String::new(),
        object_space_words: 0,
        object_table_words: 0,
        objects,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Boundary {
    ip: usize,
    slots: Vec<u16>,
}

fn run(bytes: &[u8], literals: &[u16], temps: &[u16]) -> Result<(Vec<Boundary>, u32, u16)> {
    run_source(fixture(bytes, literals, temps), false)
}

fn run_source(source: source::Image, force_collection: bool) -> Result<(Vec<Boundary>, u32, u16)> {
    let converted = target::Image::convert(&source, &[CONTEXT])?;
    converted.verify_against(&source)?;
    let assembly = interpreter::assemble(layout::reference(CONTEXT)?)?;
    let cycle = assembly.symbols["cycle"] as u16;
    let mut program = Program::from_assembly(&assembly)?;
    if force_collection {
        program = program
            .with_ram_collector(interpreter::COLLECTOR_ENTRY, rekursiv_sim::PAGER_ENTRIES)?;
    }
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
    let mut base = 0;
    for record in &converted.records {
        h.install(record.reference, record.class, base, &record.body)?;
        base += record.body.len() as u32;
    }
    if force_collection {
        // A clean, unrooted object fills the active semispace and collides
        // with the receiver's pager slot. Its next fetch must collect/retry.
        h.install(
            rekursiv_asm::Word::reference(68, true)?,
            layout::reference(60)?,
            240,
            &[rekursiv_asm::Word::ZERO; 16],
        )?;
    }
    h.load_processor(&program)?;
    let boot_services = h.stats.services;
    h.start_processor(assembly.entry.unwrap())?;
    let mut processor = Processor::default();
    let mut boundaries = Vec::new();
    h.run_processor_observed(&program, &mut processor, 1_000_000, |h, p| {
        if h.rtl.cpu_pc_o == cycle {
            let ip = p.rf[8] as usize;
            let sp = p.rf[9] as usize;
            // Inspect external RAM written by actual RTL memory transactions.
            let context_base = h.oracle.resolve(layout::reference(CONTEXT)?)?.base as usize;
            let body = &h.backing[context_base..context_base + 39];
            ensure!(target::source_oop(body[2])? == layout::integer_oop(ip as i32)?);
            ensure!(target::source_oop(body[3])? == layout::integer_oop(sp as i32)?);
            let slots = body[7..7 + sp]
                .iter()
                .map(|&w| target::source_oop(w))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            boundaries.push(Boundary { ip, slots });
        }
        Ok(())
    })
    .wrap_err_with(|| {
        format!(
            "RTL at {:?}, status {}, rf {:?}",
            program.code[h.rtl.cpu_pc_o as usize], h.rtl.cpu_last_status_o, processor.rf
        )
    })?;
    ensure!(h.rtl.cpu_fault_o == 0);
    ensure!(
        h.stats.services == boot_services,
        "host service during execution"
    );
    if force_collection {
        ensure!(h.stats.collections > 0, "collection was not exercised");
    }
    if processor.rf[15] == 5 {
        return Ok((boundaries, 5, 0));
    }
    let context_base = h.oracle.resolve(layout::reference(CONTEXT)?)?.base as usize;
    let body = &h.backing[context_base..context_base + 39];
    let sp = processor.rf[9] as usize;
    ensure!(
        body[7 + sp..]
            .iter()
            .all(|&v| v == layout::reference(layout::NIL).unwrap()),
        "dead stack slot retains a reference"
    );
    if matches!(processor.rf[15], 3 | 4 | 6) {
        let slots = body[7..7 + sp]
            .iter()
            .map(|&w| target::source_oop(w))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            slots == boundaries.last().unwrap().slots,
            "failure changed operand stack"
        );
    }
    let result = if processor.rf[15] == 1 {
        h.oracle.state.vr[5]
    } else {
        rekursiv_asm::Word::ZERO
    };
    Ok((
        boundaries,
        processor.rf[15],
        if result.is_reference() || result.compact_parts().is_ok() {
            target::source_oop(result).unwrap_or(0)
        } else {
            0
        },
    ))
}

#[test]
fn converted_method_adds_and_returns_on_rtl() -> Result<()> {
    let (boundaries, status, result) = run(&[118, 119, 176, 124], &[], &[])?;
    assert_eq!(status, 1);
    assert_eq!(result, 7);
    assert_eq!(
        boundaries,
        vec![
            Boundary {
                ip: 3,
                slots: vec![]
            },
            Boundary {
                ip: 4,
                slots: vec![3]
            },
            Boundary {
                ip: 5,
                slots: vec![3, 5]
            },
            Boundary {
                ip: 6,
                slots: vec![7]
            }
        ]
    );
    Ok(())
}

/// Independent source-oop interpreter, confined to integration tests. It uses
/// guest field arrays and a Vec stack, not physical descriptors or microcode.
fn expected(bytes: &[u8], literals: &[u16], temps: &[u16]) -> (Vec<Boundary>, u32, u16) {
    let mut pc = 0usize;
    let mut stack = Vec::new();
    let mut temps = temps.to_vec();
    let mut receiver = [1, 3, 5, 2];
    let mut association = 7;
    let mut trace = Vec::new();
    let initial_ip = literals.len() * 2 + 3;
    for _ in 0..1000 {
        let mut slots = temps.clone();
        slots.extend(&stack);
        trace.push(Boundary {
            ip: initial_ip + pc,
            slots,
        });
        let Some(&op) = bytes.get(pc) else {
            return (trace, 5, 0);
        };
        pc += 1;
        match op {
            0..=15 => stack.push(receiver[op as usize]),
            16..=31 => stack.push(temps[op as usize - 16]),
            32..=63 => stack.push(literals[op as usize - 32]),
            64..=95 => {
                assert_eq!(literals[op as usize - 64], ASSOCIATION);
                stack.push(association);
            }
            96..=103 => receiver[op as usize - 96] = stack.pop().unwrap(),
            104..=111 => temps[op as usize - 104] = stack.pop().unwrap(),
            112..=119 => stack.push([RECEIVER, 6, 4, 2, 0xffff, 1, 3, 5][op as usize - 112]),
            120..=123 => return (trace, 1, [RECEIVER, 6, 4, 2][op as usize - 120]),
            124 => return (trace, 1, *stack.last().unwrap()),
            128..=130 => {
                let descriptor = bytes[pc];
                pc += 1;
                let index = usize::from(descriptor & 63);
                let kind = descriptor >> 6;
                if op == 128 {
                    stack.push(match kind {
                        0 => receiver[index],
                        1 => temps[index],
                        2 => literals[index],
                        3 => {
                            assert_eq!(literals[index], ASSOCIATION);
                            association
                        }
                        _ => unreachable!(),
                    });
                } else {
                    let value = *stack.last().unwrap();
                    match kind {
                        0 => receiver[index] = value,
                        1 => temps[index] = value,
                        2 => return (trace, 5, 0),
                        3 => {
                            assert_eq!(literals[index], ASSOCIATION);
                            association = value
                        }
                        _ => unreachable!(),
                    }
                    if op == 130 {
                        stack.pop();
                    }
                }
            }
            135 => {
                stack.pop().unwrap();
            }
            136 => stack.push(*stack.last().unwrap()),
            137 => stack.push(CONTEXT),
            144..=151 => pc += usize::from(op - 143),
            152..=159 => {
                let v = *stack.last().unwrap();
                if v != 4 && v != 6 {
                    return (trace, 4, 0);
                }
                stack.pop();
                if v == 4 {
                    pc += usize::from(op - 151);
                }
            }
            160..=167 => {
                let low = bytes[pc];
                pc += 1;
                pc = (pc as isize + (isize::from(op) - 164) * 256 + isize::from(low)) as usize;
            }
            168..=175 => {
                let offset = usize::from(op & 3) * 256 + usize::from(bytes[pc]);
                pc += 1;
                let v = *stack.last().unwrap();
                if v != 4 && v != 6 {
                    return (trace, 4, 0);
                }
                stack.pop();
                if v == if op < 172 { 6 } else { 4 } {
                    pc += offset;
                }
            }
            176..=184 | 190..=191 => {
                let a = stack[stack.len() - 2];
                let b = stack[stack.len() - 1];
                if a & 1 == 0 || b & 1 == 0 {
                    return (trace, 6, 0);
                }
                let a = i32::from((a as i16) >> 1);
                let b = i32::from((b as i16) >> 1);
                let result = match op {
                    176 => a + b,
                    177 => a - b,
                    184 => a * b,
                    190 => a & b,
                    191 => a | b,
                    178..=183 => {
                        let yes = match op {
                            178 => a < b,
                            179 => a > b,
                            180 => a <= b,
                            181 => a >= b,
                            182 => a == b,
                            _ => a != b,
                        };
                        stack.pop();
                        *stack.last_mut().unwrap() = if yes { 6 } else { 4 };
                        continue;
                    }
                    _ => unreachable!(),
                };
                let Ok(oop) = layout::integer_oop(result) else {
                    return (trace, 6, 0);
                };
                stack.pop();
                *stack.last_mut().unwrap() = oop;
            }
            _ => return (trace, 2, 0),
        }
    }
    panic!("guest test did not terminate")
}

fn compare(bytes: &[u8], literals: &[u16], temps: &[u16]) -> Result<()> {
    assert_eq!(
        run(bytes, literals, temps)
            .wrap_err_with(|| format!("bytes {bytes:?}, literals {literals:?}, temps {temps:?}"))?,
        expected(bytes, literals, temps),
        "bytecodes {bytes:?}, literals {literals:?}"
    );
    Ok(())
}

#[test]
fn constants_returns_and_stack_operations_match_guest_boundaries() -> Result<()> {
    for opcode in 112..=119 {
        compare(&[opcode, 136, 135, 124], &[], &[])?;
    }
    for opcode in 120..=123 {
        compare(&[opcode], &[], &[])?;
    }
    compare(&[137, 124], &[], &[])?;
    compare(&[118, 119, 135, 124], &[], &[])?;
    Ok(())
}

#[test]
fn variables_literals_and_extended_stores_match_guest_boundaries() -> Result<()> {
    compare(&[0, 1, 176, 124], &[], &[])?;
    compare(&[118, 96, 0, 124], &[], &[])?;
    compare(&[119, 104, 16, 124], &[], &[1])?;
    compare(&[32, 33, 176, 124], &[0xffff, 5], &[])?;
    compare(&[64, 124], &[ASSOCIATION], &[])?;
    // Each legal extended kind, with both pop-store and non-pop-store.
    for kind in [0, 1, 3] {
        for store in [129, 130] {
            let mut bytes = vec![119, store, kind * 64];
            if store == 129 {
                bytes.push(135);
            }
            bytes.extend([128, kind * 64, 124]);
            compare(&bytes, &[ASSOCIATION], &[1])?;
        }
    }
    compare(&[128, 128, 124], &[0x7fff], &[])?;
    // Use literal and temporary indices beyond the short bytecode ranges.
    let mut literals = vec![1; 63];
    literals[62] = 0x8001;
    compare(&[128, 190, 124], &literals, &[])?;
    let mut temps = vec![1; 20];
    temps[19] = 0xffff;
    compare(&[128, 83, 124], &[], &temps)?;
    Ok(())
}

#[test]
fn branches_and_backward_loop_match_guest_instruction_pointers() -> Result<()> {
    for constant in [113, 114] {
        compare(&[constant, 152, 118, 119, 124], &[], &[])?;
        compare(&[constant, 168, 1, 118, 119, 124], &[], &[])?;
        compare(&[constant, 172, 1, 118, 119, 124], &[], &[])?;
    }
    compare(&[144, 118, 119, 124], &[], &[])?;
    compare(&[164, 1, 118, 119, 124], &[], &[])?;
    // temp0 := temp0 - 1; repeat until temp0 = 0. Long signed branch -11.
    compare(
        &[16, 118, 177, 104, 16, 117, 182, 168, 2, 163, 245, 16, 124],
        &[],
        &[7],
    )?;
    // Exercise the high offset bits without running the skipped bytes.
    let mut bytes = vec![165, 0];
    bytes.resize(258, 126);
    bytes.extend([119, 124]);
    compare(&bytes, &[], &[])?;
    Ok(())
}

#[test]
fn integer_fast_paths_preserve_arguments_on_type_and_range_failure() -> Result<()> {
    let values = [-16384, -16383, -2, -1, 0, 1, 2, 16382, 16383];
    for opcode in (176..=184).chain([190, 191]) {
        for (a, b) in values.into_iter().zip(values.into_iter().rev()) {
            compare(
                &[32, 33, opcode, 124],
                &[layout::integer_oop(a)?, layout::integer_oop(b)?],
                &[],
            )?;
        }
        for pair in [[2, 1], [1, 2], [RECEIVER, RECEIVER]] {
            compare(&[32, 33, opcode, 124], &pair, &[])?;
        }
    }
    for (op, a, b) in [
        (176, 16383, 1),
        (176, -16384, -1),
        (177, -16384, 1),
        (177, 16383, -1),
        (184, 16383, 2),
        (184, -16384, -1),
    ] {
        compare(
            &[32, 33, op, 124],
            &[layout::integer_oop(a)?, layout::integer_oop(b)?],
            &[],
        )?;
    }
    Ok(())
}

#[test]
fn unsupported_operations_and_non_boolean_branches_stop_explicitly() -> Result<()> {
    for opcode in [125, 126, 127, 138, 143] {
        compare(&[opcode], &[], &[])?;
    }
    for bytes in [&[117, 152][..], &[112, 168, 0], &[115, 172, 0]] {
        compare(bytes, &[], &[])?;
    }
    Ok(())
}

#[test]
fn context_and_interpreter_roots_survive_machine_collection_during_refill() -> Result<()> {
    // Preserve a context self-reference in a temporary and a receiver
    // reference on the guest stack while the next field fetch collects.
    let bytes = [137, 104, 112, 0, 135, 16, 124];
    assert_eq!(
        run_source(fixture(&bytes, &[], &[1]), true)?,
        expected(&bytes, &[], &[1])
    );
    Ok(())
}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn original_xerox_methods_execute_on_rtl() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let original = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    let distribution = rekursiv_smalltalk::import_distribution(&original)?;
    for (oop, result) in [(0x8302, 4), (0x8356, 6)] {
        // Original Object>>isNil and Object>>notNil, including source trailer.
        // Keep their identity, header and bytes. Supply only a root context
        // and receiver; this is method execution, not image startup or lookup.
        let method = distribution
            .records
            .iter()
            .find(|r| r.source_oop == oop)
            .unwrap()
            .decode()?;
        let mut source = fixture(&[], &[], &[]);
        source.objects.remove(&METHOD);
        source.objects.insert(oop, method);
        if let source::Body::Pointers(fields) = &mut source.objects.get_mut(&CONTEXT).unwrap().body
        {
            fields[3] = oop;
        }
        let (_, status, value) = run_source(source, false)?;
        assert_eq!((status, value), (1, result));
    }
    Ok(())
}

#[test]
fn invalid_stack_indices_and_boot_state_are_explicit_errors() -> Result<()> {
    for bytes in [
        &[135][..],
        &[136],
        &[124],
        &[176],
        &[118, 176],
        &[16],
        &[32],
        &[163, 0],
        &[118, 129, 128],
    ] {
        assert_eq!(run(bytes, &[], &[])?.1, 5, "{bytes:?}");
    }
    let mut overflow = vec![117; 33];
    overflow.push(124);
    assert_eq!(run(&overflow, &[], &[])?.1, 5);
    for (field, value) in [(1, 2), (1, 0xffff), (2, 2), (2, 0xffff), (2, 67)] {
        let mut source = fixture(&[123], &[], &[]);
        if let source::Body::Pointers(fields) = &mut source.objects.get_mut(&CONTEXT).unwrap().body
        {
            fields[field] = value;
        }
        assert_eq!(
            run_source(source, false)?.1,
            5,
            "context field {field} = {value}"
        );
    }
    Ok(())
}
