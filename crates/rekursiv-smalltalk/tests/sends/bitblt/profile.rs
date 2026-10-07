//! Deterministic guest BitBlt work counts, checked against the pixel oracle.
//! Single stepping assigns instructions to phases. It measures guest work,
//! not native JIT throughput or FPGA cycles. Setup, initial registration, and
//! caller bytecodes are excluded. Counts stop at `bb_success`, before caller
//! restoration. Each fixture still runs to completion for result checks.
//! Run the ignored test with `--nocapture`. BITBLT_PROFILE_OUTPUT optionally
//! saves its JSON report.
use super::*;

#[test]
#[ignore = "explicit microcode work measurement"]
fn profile_bitblt_microcode() -> Result<()> {
    let large = Case {
        dimensions: (256, 128),
        dest: (0, 0),
        source: (0, 0),
        extent: (256, 128),
        clip: (0, 0, 256, 128),
        halftone: false,
        ..Default::default()
    };
    let cases = [
        (
            "fill",
            Case {
                nil_source: true,
                ..large.clone()
            },
        ),
        ("aligned", large.clone()),
        (
            "unaligned",
            Case {
                dest: (3, 0),
                source: (1, 0),
                extent: (250, 128),
                ..large.clone()
            },
        ),
        (
            "text",
            Case {
                dest: (7, 4),
                source: (2, 0),
                extent: (9, 13),
                rule: 7,
                ..large.clone()
            },
        ),
        (
            "halftone",
            Case {
                nil_source: true,
                halftone: true,
                rule: 6,
                ..large.clone()
            },
        ),
        (
            "scroll",
            Case {
                dest: (0, 0),
                source: (0, 8),
                extent: (256, 120),
                overlap: true,
                ..large
            },
        ),
    ];
    let assembly = interpreter::assemble(r(ROOT))?;
    let address = |name: &str| assembly.symbols[name] as u16;
    let mut rows = Vec::new();
    for (name, case) in cases {
        for display in [false, true] {
            let image = if display {
                registered(&case, 102)
            } else {
                make(&case)
            };
            let pixels = expected(&image, &case);
            let mut m = profile_machine(&image)?;
            let mut phase = None;
            let mut validated = false;
            let mut snapshot = false;
            let mut counts = [[0u64; 3]; 5];
            let mut completed = 0;
            for _ in 0..5_000_000 {
                if m.cpu.halted {
                    break;
                }
                if !m.recovering() {
                    let pc = m.cpu.pc;
                    if pc == address("primitive_bitblt") {
                        phase = Some(0);
                    }
                    if pc == address("bb_preflight_done") {
                        validated = true;
                        phase = Some(0);
                    }
                    if pc == address("bb_stage_allocate") {
                        snapshot = true;
                        phase = Some(2);
                    }
                    if pc == address("bb_pass_start") {
                        phase = Some(if !validated {
                            1
                        } else if snapshot {
                            2
                        } else {
                            3
                        });
                    }
                    if pc == address("bb_staged") {
                        phase = Some(3);
                    }
                    if pc == address("bb_refresh") {
                        phase = Some(4);
                    }
                    if pc == address("bb_success") {
                        completed += 1;
                        phase = None;
                    }
                    ensure!(pc != address("bb_failed"), "{name}: primitive fallback");
                }
                let before = [
                    m.stats.retired,
                    m.stats.object_commands,
                    m.stats.device_requests,
                ];
                m.step()?;
                if let Some(p) = phase {
                    let after = [
                        m.stats.retired,
                        m.stats.object_commands,
                        m.stats.device_requests,
                    ];
                    for k in 0..3 {
                        counts[p][k] += after[k] - before[k];
                    }
                }
            }
            ensure!(
                m.cpu.halted && completed == 1 && m.cpu.rf[15] == 1,
                "{name}: incomplete"
            );
            assert_eq!(
                m.stats.collections, 0,
                "profile fixture should isolate drawing"
            );
            assert!(body(&m, DEST_BITS)[1..].iter().all(|w| w.bits() <= 0xffff));
            assert_eq!(
                body(&m, DEST_BITS)[1..]
                    .iter()
                    .map(|w| w.bits() as u16)
                    .collect::<Vec<_>>(),
                pixels
            );
            if display {
                let bitmap = m.devices.display_bitmap.as_ref().unwrap();
                assert_eq!(bitmap.publications, 2);
                assert_eq!(
                    bitmap.visible.as_ref().unwrap().words,
                    packed(&pixels, case.dimensions)
                );
            }
            // Use native blocks as well as single stepping: combined ALU,
            // stack, and branch controls must agree in the actual CLI engine.
            let mut jit = profile_machine(&image)?;
            jit.enable_jit()?;
            jit.run_steps(5_000_000)?;
            assert_eq!(jit.cpu, m.cpu, "{name}: JIT processor state");
            assert_eq!(jit.objekt, m.objekt, "{name}: JIT object memory");
            assert_eq!(jit.stats.retired, m.stats.retired);
            assert_eq!(jit.stats.device_requests, m.stats.device_requests);
            if display {
                let bitmap = jit.devices.display_bitmap.as_ref().unwrap();
                assert_eq!(bitmap.publications, 2);
                assert_eq!(
                    bitmap.visible.as_ref().unwrap().words,
                    packed(&pixels, case.dimensions)
                );
            }
            let phases: serde_json::Map<String, serde_json::Value> =
                ["setup", "validation", "snapshot", "merge", "presentation"].into_iter().enumerate()
                .map(|(n, phase)| (phase.to_owned(), serde_json::json!({
                    "instructions": counts[n][0], "object_commands": counts[n][1], "device_requests": counts[n][2]
                }))).collect();
            rows.push(
                serde_json::json!({"case": name, "display": display, "phases": phases,
                "instructions": counts.iter().map(|p| p[0]).sum::<u64>()}),
            );
        }
    }
    let report = serde_json::to_string_pretty(&rows)?;
    println!("{report}");
    if let Ok(path) = std::env::var("BITBLT_PROFILE_OUTPUT") {
        std::fs::write(path, report)?;
    }
    Ok(())
}

/// Use ample pager/RAM capacity and a display large enough for the fixtures.
fn profile_machine(image: &source::Image) -> Result<Machine> {
    let mut device = bitmap_device();
    device.display_bitmap = Some(rekursiv_sim::device::Bitmap::new(256, 128));
    native_machine(image, device, |_| {}, 65_536, 65_536, false)
}
