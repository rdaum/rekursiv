use eyre::{ensure, Result};
use rekursiv_asm::{Status, Word};
use rekursiv_emulator::{boot, Step};
use rekursiv_model::processor::Processor;
use rekursiv_sim::{Harness, Timing};

fn compare_program(source: &str, timing: Timing) -> Result<(u64, rekursiv_emulator::Machine)> {
    let runtime = rekursiv_sim::runtime()?;
    let mut loaded = boot::microcode_with_pager(source, 512, 16)?;
    let m = &mut loaded.machine;
    m.enable_jit()?;
    let mut h = Harness::new(&runtime, timing, None)?;
    // One external scratch register for the device-barrier case.
    m.devices.registers.insert(0, 0);
    h.device.registers.insert(0, 0);
    h.load_processor(m.image())?;
    h.start_processor(m.cpu.pc)?;
    let start = h.stats.cycles;
    let mut oracle = Processor {
        pc: m.cpu.pc,
        ..Default::default()
    };
    h.run_processor_observed(&m.image().clone(), &mut oracle, 5_000_000, |h, expected| {
        for _ in 0..200_000 {
            if m.step()? == Step::Retired && !m.recovering() {
                ensure!(m.cpu == *expected, "CPU mismatch at {}", expected.pc);
                ensure!(m.objekt == h.oracle, "OBJEKT mismatch at {}", expected.pc);
                return Ok(());
            }
        }
        eyre::bail!("native retirement timed out")
    })?;
    h.compare_state()?;
    assert!(m.cpu.halted || m.cpu.service);
    Ok((h.stats.cycles - start, loaded.machine))
}

#[test]
fn shared_stream_microcode_overlaps_numeric_work_and_reduces_rtl_cycles() -> Result<()> {
    let source = include_str!("../../../microcode/pipeline.uc");
    let blocking = source.replace(", launch", "");
    for timing in [
        Timing::default(),
        Timing {
            request_delay: 4,
            memory_latency: 13,
            response_stall: 5,
        },
    ] {
        let (serial_cycles, serial) = compare_program(&blocking, timing)?;
        let (pipeline_cycles, pipeline) = compare_program(source, timing)?;
        assert_eq!(pipeline.cpu, serial.cpu);
        assert_eq!(pipeline.objekt, serial.objekt);
        assert_eq!(pipeline.cpu.rf[..4], [110, 0, 16, 4]);
        assert_eq!(pipeline.objekt.state.prepared.status, Status::BoundsError);
        eprintln!(
            "stream {timing:?}: blocking {serial_cycles}, pipelined {pipeline_cycles} cycles"
        );
        assert!(pipeline_cycles < serial_cycles);
    }
    Ok(())
}

#[test]
fn prepared_reference_is_a_gc_root_and_its_address_is_relocated() -> Result<()> {
    // A has no reference in VRs, CPU stacks, literals or current selection at
    // collection. The address pipeline is its only root. B becomes selection.
    let source = "
        d=0xa000000064, page=Allocate, size=2, scan=0
        idx=Two
        d=73, mem=Write
        prepare
        d=0xa000000064, page=Allocate, size=1, scan=0
        gc=Collect
        mem=Read, prepared, launch
        d=17, r=Bus, rb=7, ldrb
        d=Object, estk=Bus
        halt";
    let (_, m) = compare_program(
        source,
        Timing {
            request_delay: 2,
            memory_latency: 5,
            response_stall: 3,
        },
    )?;
    assert_eq!(m.cpu.estkr, 73);
    assert_eq!(m.cpu.rf[7], 17);
    assert_eq!(m.stats.collections, 1);
    assert_eq!(m.objekt.state.prepared.address, 257);
    assert_eq!(
        m.objekt.state.prepared.reference,
        Word::reference(1, false)?
    );
    Ok(())
}

#[test]
fn launch_defers_bounds_fault_until_barrier_and_keeps_intervening_work() -> Result<()> {
    let source = "
        d=0xa000000064, page=Allocate, size=2, scan=0
        idx=Clear, prepare
        mem=Read, prepared, launch
        d=17, r=Bus, rb=7, ldrb
        d=Object, r=Bus, rb=8, ldrb
        halt";
    let runtime = rekursiv_sim::runtime()?;
    let mut m = boot::microcode_with_pager(source, 512, 16)?.machine;
    m.enable_jit()?;
    let mut h = Harness::new(
        &runtime,
        Timing {
            request_delay: 3,
            memory_latency: 5,
            response_stall: 2,
        },
        None,
    )?;
    h.load_processor(m.image())?;
    h.start_processor(0)?;
    for _ in 0..1000 {
        if h.rtl.cpu_halted_o != 0 {
            break;
        }
        h.tick()?;
    }
    assert!(m.step().is_ok());
    assert!(m.step().is_ok());
    assert!(
        m.step().is_ok(),
        "launch itself retires despite deferred fault"
    );
    assert!(m.step().is_ok());
    assert!(m.step().is_err());
    assert_eq!(m.fault.unwrap().status, Some(Status::BoundsError));
    assert_eq!(m.fault.unwrap().pc, 4);
    assert_eq!(m.cpu.rf[7], 17);
    assert_eq!(m.cpu.rf[8], 0);
    assert_eq!(h.rtl.cpu_fault_o, 4);
    assert_eq!(h.rtl.cpu_last_status_o, Status::BoundsError as u8);
    h.compare_processor(&m.cpu)?;
    h.oracle = m.objekt;
    h.compare_state()?;
    Ok(())
}

#[test]
fn implicit_barriers_and_slow_numeric_work_publish_the_same_result() -> Result<()> {
    for barrier in [
        "halt",
        "d=Object, ldvr, vr=7\nhalt",
        "gc=Collect\nhalt",
        "seq=Service, brch=3",
        "d=17, io=Write\nhalt",
        "cc=ObjectOk, seq=ConditionalJump, brch=done\nd=999, r=Bus, rb=0, ldrb\ndone: halt",
    ] {
        let source = format!(
            "d=0xa000000064, page=Allocate, size=2, scan=0\n\
            idx=Two, prepare\n\
            d=73, mem=Write\n\
            prepare\n\
            mem=Read, prepared, launch\n\
            d=9, r=Bus, alu=Float, fp=FromUnsigned, rb=7, ldrb\n{barrier}"
        );
        let (_, m) = compare_program(
            &source,
            Timing {
                request_delay: 3,
                memory_latency: 11,
                response_stall: 7,
            },
        )?;
        assert_eq!(m.cpu.rf[7], 9.0f32.to_bits());
        assert_eq!(m.cpu.rf[0], 0);
        if barrier.contains("io=Write") {
            assert_eq!(m.devices.registers.get(&0), Some(&17));
        }
        if barrier.starts_with("d=Object") {
            assert_eq!(m.objekt.state.vr[7].bits(), 73);
        } else {
            assert_eq!(m.cpu.object, 73);
        }
    }
    Ok(())
}

#[test]
fn local_fault_drains_the_launched_access_and_preserves_error_priority() -> Result<()> {
    let rt = rekursiv_sim::runtime()?;
    for invalid_address in [false, true] {
        let index = if invalid_address { "Clear" } else { "Two" };
        let source = format!(
            "d=0xa000000064, page=Allocate, size=2, scan=0\n\
            idx=Two\n\
            d=73, mem=Write\n\
            idx={index}, prepare\n\
            mem=Read, prepared, launch\n\
            d=32, esp=Bus\n\
            halt"
        );
        let mut m = boot::microcode_with_pager(&source, 512, 16)?.machine;
        m.enable_jit()?;
        let mut h = Harness::new(
            &rt,
            Timing {
                request_delay: 5,
                memory_latency: 13,
                response_stall: 0,
            },
            None,
        )?;
        h.load_processor(m.image())?;
        h.start_processor(0)?;
        for cycle in 0..1000 {
            if h.rtl.cpu_halted_o != 0 {
                break;
            }
            h.rtl.cpu_response_enable_i = (cycle % 11 > 6) as u8;
            h.tick()?;
        }
        for _ in 0..5 {
            assert_eq!(m.step()?, Step::Retired);
        }
        assert!(m.step().is_err());
        assert_eq!(m.fault.unwrap().pc, 5);
        assert_eq!(m.fault.unwrap().code, if invalid_address { 4 } else { 3 });
        assert_eq!(
            m.cpu.object,
            if invalid_address {
                Word::NIL.bits()
            } else {
                73
            }
        );
        assert_eq!(h.rtl.cpu_fault_o, m.fault.unwrap().code);
        assert_eq!(h.stats.commands, h.stats.responses);
        h.compare_processor(&m.cpu)?;
        h.oracle = m.objekt;
        h.compare_state()?;
    }
    Ok(())
}
