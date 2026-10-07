//! Reproducible bytecode-loop work counts. The profiler observes retired
//! microinstructions and guest boundaries; it never supplies runtime results.
//! Run the ignored test with --nocapture. DISPATCH_PROFILE_OUTPUT saves JSON.
use super::*;
use rekursiv_emulator::Machine;

/// A phase owns helper calls until another named phase or handler takes over.
fn profile(
    mut m: Machine,
    assembly: &rekursiv_asm::text::Assembly,
) -> Result<(Machine, serde_json::Value)> {
    let pc = |name: &str| assembly.symbols[name] as u16;
    let mut phase = 5;
    let mut counts = [[0u64; 3]; 6];
    let mut opcodes = [0u64; 256];
    let mut trace = Vec::new();
    for _ in 0..5_000_000 {
        if m.cpu.halted {
            break;
        }
        let address = m.cpu.pc;
        if !m.recovering() {
            if address == pc("boundary") || address == pc("save_context") {
                phase = 0;
            }
            if address == pc("check_process_switch") {
                phase = 1;
            }
            if address == pc("check_low_space") {
                phase = 2;
            }
            if address == pc("check_device_events") {
                phase = 3;
            }
            if address == pc("load_context") {
                phase = 5;
            }
            if (pc("cycle")..pc("push_receiver_field")).contains(&address) {
                phase = 4;
            }
            if address == pc("decoded") {
                let opcode = m.cpu.rf[0] as usize;
                ensure!(opcode < 256);
                opcodes[opcode] += 1;
                trace.push((
                    m.objekt.state.vr[0].bits(),
                    m.cpu.rf[8],
                    m.cpu.rf[9],
                    opcode,
                ));
            }
            if address == pc("cycle") {
                // The optimized loop must continue to expose IP/SP in the
                // actual context object before every guest bytecode.
                let e = m.objekt.resolve(m.objekt.state.vr[0])?;
                assert_eq!(m.objekt.memory[e.base as usize + 2], i(m.cpu.rf[8] as i32));
                assert_eq!(m.objekt.memory[e.base as usize + 3], i(m.cpu.rf[9] as i32));
            }
        }
        let dispatch = m.image().code[address as usize]
            .is_some_and(|i| i.seq == rekursiv_asm::processor::Seq::Dispatch);
        let byte_return = (pc("fetch_byte")..pc("push_receiver_field")).contains(&address)
            && m.image().code[address as usize]
                .is_some_and(|i| i.seq == rekursiv_asm::processor::Seq::Bus);
        let before = [
            m.stats.retired,
            m.stats.object_commands,
            m.stats.device_requests,
        ];
        m.step()?;
        let after = [
            m.stats.retired,
            m.stats.object_commands,
            m.stats.device_requests,
        ];
        for n in 0..3 {
            counts[phase][n] += after[n] - before[n];
        }
        if dispatch || byte_return {
            phase = 5;
        }
    }
    ensure!(
        m.cpu.halted && m.cpu.rf[15] == 1,
        "profile did not return normally"
    );
    assert_eq!(m.stats.collections, 0);
    let phases: serde_json::Map<String, serde_json::Value> = [
        "boundary",
        "scheduler",
        "low_space",
        "events",
        "fetch_dispatch",
        "handler_activation",
    ]
    .into_iter()
    .enumerate()
    .map(|(n, name)| {
        (
            name.to_owned(),
            serde_json::json!({
                "instructions": counts[n][0],
                "object_commands": counts[n][1],
                "device_requests": counts[n][2],
            }),
        )
    })
    .collect();
    let bytecodes = opcodes.iter().sum::<u64>();
    let report = serde_json::json!({
        "phases": phases,
        "bytecodes": bytecodes,
        "instructions": m.stats.retired,
        "instructions_per_bytecode": m.stats.retired as f64 / bytecodes as f64,
        "opcodes": opcodes.to_vec(),
        "trace_hash": rekursiv_smalltalk::checksum(&serde_json::to_vec(&trace)?),
    });
    Ok((m, report))
}

/// Run enough repetitions to amortize boot and the final root return.
#[test]
#[ignore = "explicit interpreter work measurement"]
fn profile_bytecode_loop() -> Result<()> {
    let mut reports = Vec::new();
    for (name, pattern, literals) in [
        ("push_pop", vec![113, 135], vec![]),
        ("integer_add", vec![118, 119, 176, 135], vec![]),
        ("receiver_field", vec![0, 135], vec![]),
        ("literal", vec![32, 135], vec![oop(7)]),
        ("extended_literal", vec![128, 128, 135], vec![oop(7)]),
        ("conditional", vec![114, 152, 113], vec![]), // false skips a push
        ("send", vec![112, 208, 135], vec![SELECTOR]),
    ] {
        for registered in [false, true] {
            let mut bytes = pattern.repeat(128);
            bytes.push(120); // return receiver
            let mut source = fixture(&bytes, &literals);
            if registered {
                // Bootstrap the runtime's private, copied registration state.
                // Zero thresholds and an empty notification queue perform checks
                // without signalling, collection, or external device traffic.
                pointers(&mut source, 38, META, vec![2, 2, 0xc007]);
                pointers(&mut source, 302, 38, vec![2, 2, oop(0)]);
                pointers(
                    &mut source,
                    300,
                    16,
                    vec![302, oop(0), oop(0), oop(0), oop(0), oop(0), oop(0)],
                );
                pointers(&mut source, 304, 16, vec![2, 2, 2, oop(0)]);
            }
            let assembly = if let Ok(path) = std::env::var("DISPATCH_PROFILE_SOURCE") {
                rekursiv_asm::text::assemble(
                    &std::fs::read_to_string(path)?,
                    0,
                    &[("ACTIVE_CONTEXT", r(ROOT).bits() as i64)],
                )?
            } else {
                interpreter::assemble(r(ROOT))?
            };
            let build = || -> Result<Machine> {
                let mut m =
                    native::machine(&source, Default::default(), |_| {}, 65_536, 65_536, false)?;
                // The optional source snapshot permits before/after runs
                // with identical fixtures and without changing the checkout.
                *m.image_mut() = Program::from_assembly(&assembly)?
                    .with_ram_collector(interpreter::COLLECTOR_ENTRY, 65_536)?;
                m.cpu.pc = assembly.entry.unwrap();
                if registered {
                    let roots = m.cpu.roots.as_mut().unwrap();
                    roots[27] = r(302);
                    roots[29] = r(304);
                    roots[30] = r(300);
                }
                Ok(m)
            };
            let (m, mut report) = profile(build()?, &assembly)?;
            if registered {
                // An image edit alone does not update the processor's live
                // roots. Require actual registration-object reads here.
                ensure!(
                    report["phases"]["low_space"]["object_commands"]
                        .as_u64()
                        .unwrap()
                        > 0
                );
                ensure!(
                    report["phases"]["events"]["object_commands"]
                        .as_u64()
                        .unwrap()
                        > 0
                );
            }
            assert_eq!(m.objekt.state.vr[5], r(RECEIVER));
            let mut jit = build()?;
            jit.enable_jit()?;
            jit.run_steps(5_000_000)?;
            assert_eq!(jit.cpu, m.cpu, "{name}");
            assert_eq!(jit.objekt, m.objekt, "{name}");
            assert_eq!(jit.stats.retired, m.stats.retired);
            report["case"] = serde_json::json!(name);
            report["registered_checks"] = serde_json::json!(registered);
            reports.push(report);
        }
    }
    let json = serde_json::to_string_pretty(&reports)?;
    println!("{json}");
    if let Ok(path) = std::env::var("DISPATCH_PROFILE_OUTPUT") {
        std::fs::write(path, json)?;
    }
    Ok(())
}
