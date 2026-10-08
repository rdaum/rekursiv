//! Calibrate the passive model against clocks of the real RTL state machines.
//! No adversarial command gate or response-consumption gate is enabled here.
use eyre::Result;
use rekursiv_emulator::{
    boot,
    timing::{Channel, Config},
    Machine,
};
use rekursiv_sim::{Harness, Timing};

fn transport(c: Channel) -> Timing {
    Timing {
        request_delay: c.request_wait,
        memory_latency: c.response_cycles - 1,
        response_stall: 0,
    }
}

fn native(source: &str, config: Option<Config>, jit: bool, budget: u64) -> Result<Machine> {
    let mut m = boot::microcode_with_pager(source, 512, 16)?.machine;
    m.devices.registers.insert(0, 0);
    if let Some(config) = config {
        m.enable_cycle_estimate(config)?;
    }
    if jit {
        m.enable_jit()?;
    }
    for _ in 0..1_000_000 {
        m.run_steps(budget)?;
        if m.cpu.halted || m.cpu.service {
            return Ok(m);
        }
    }
    eyre::bail!("native timeout")
}

fn rtl_cycles(m: &Machine, config: Config) -> Result<u64> {
    let runtime = rekursiv_sim::runtime()?;
    let mut h = Harness::new(&runtime, transport(config.ram), None)?;
    h.store_timing = transport(config.backing);
    h.device.timing = transport(config.device);
    h.device.registers.insert(0, 0);
    h.load_processor(m.image())?;
    h.start_processor(0)?;
    let start = h.stats.cycles;
    for _ in 0..2_000_000 {
        if h.rtl.cpu_halted_o != 0 || h.rtl.cpu_service_o != 0 {
            break;
        }
        h.tick()?;
    }
    assert!(
        h.rtl.cpu_halted_o != 0 || h.rtl.cpu_service_o != 0,
        "RTL timeout"
    );
    h.compare_processor(&m.cpu)?;
    let measured = h.stats.cycles - start;
    h.oracle = m.objekt.clone();
    h.compare_state()?;
    Ok(measured)
}

fn calibrate(source: &str, config: Config) -> Result<()> {
    let m = native(source, Some(config), true, 1000)?;
    let measured = rtl_cycles(&m, config)?;
    let estimate = m.cycle_estimate().unwrap();
    assert_eq!(estimate.work().approximate_objects, 0);
    assert_eq!(estimate.work().approximate_floats, 0);
    assert_eq!(
        estimate.cycles().total(),
        measured,
        "{config:?}\n{source}\n{estimate:?}"
    );
    Ok(())
}

#[test]
fn resident_transfer_directory_and_device_costs_match_rtl() -> Result<()> {
    for config in [
        Config::SRAM,
        Config::DRAM,
        Config {
            ram: Channel {
                request_wait: 3,
                response_cycles: 7,
            },
            backing: Channel {
                request_wait: 2,
                response_cycles: 4,
            },
            device: Channel {
                request_wait: 2,
                response_cycles: 3,
            },
        },
    ] {
        for source in [
            "d=7, r=Bus, ldrb\nalu=MultiplyUnsigned\nhalt",
            "idx=One\nread=FreeWords\nhalt",
            "d=0xa000000064, page=Allocate, size=0, scan=0\nhalt",
            "d=0xa000000064, page=Allocate, size=3, scan=0\nidx=One\nmem=Read\nidx=Two\nmem=Read\nd=73, mem=Write\nhalt",
            "d=0xa000000064, page=Allocate, size=0, scan=0\nd=1, page=FindObject\nd=0, page=NextObject\nhalt",
            "d=17, io=Write\nio=Read\nhalt",
            "d=9, r=Bus, alu=Float, fp=FromUnsigned, rb=7, ldrb\nhalt",
            "d=0xa000000064, page=Allocate, size=2, scan=0\nd=Object, ldvr, vr=0\nd=0x8000000001, page=Exchange, vr=0\nd=0xa000000064, page=Allocate, size=3, scan=0\nd=Object, page=Exchange, vr=0\nd=0x8000000001, page=Fetch\nhalt",
            include_str!("../../../microcode/allocation.uc"),
        ] { calibrate(source, config)?; }
        // The first exchange uses one resident and one evicted object; a
        // repeated exchange then reads both bodies from backing storage.
        let exchange = include_str!("../../../microcode/allocation.uc").replace(
            "    halt", "    d=0xa000000011, ldvr, vr=0\n    d=0xa000000001, page=Exchange, vr=0\n    d=0xa000000001, page=Exchange, vr=0\n    halt");
        calibrate(&exchange, config)?;
        let dirty = include_str!("../../../microcode/allocation.uc").replace(
            "    halt", "    d=7, mem=Write\n    d=0xa000000011, page=Fetch\n    d=0xa000000001, page=Fetch\n    halt");
        calibrate(&dirty, config)?;
    }
    Ok(())
}

#[test]
fn asynchronous_overlap_and_collector_costs_match_rtl() -> Result<()> {
    for config in [Config::SRAM, Config::DRAM] {
        calibrate(include_str!("../../../microcode/pipeline.uc"), config)?;
        calibrate(
            &include_str!("../../../microcode/pipeline.uc").replace(", launch", ""),
            config,
        )?;
        calibrate(
            "gc=Collect\nd=0xa000000064, page=Allocate, size=3, scan=0\ngc=Collect\nhalt",
            config,
        )?;
        calibrate(include_str!("../../../microcode/collection.uc"), config)?;
        // A floating-point operation is independent of a pending RAM reply.
        calibrate("d=0xa000000064, page=Allocate, size=2, scan=0\nidx=Two, prepare\nmem=Read, prepared, launch\nd=9, r=Bus, alu=Float, fp=FromUnsigned, rb=7, ldrb\nhalt", config)?;
    }
    Ok(())
}

#[test]
fn estimates_are_engine_and_batch_independent_and_passive() -> Result<()> {
    for source in [
        include_str!("../../../microcode/pipeline.uc"),
        "d=0xa000000064, page=Allocate, size=3, scan=0\ngc=Collect\nhalt",
    ] {
        let plain = native(source, None, true, 1000)?;
        let expected = native(source, Some(Config::DRAM), false, 1)?;
        for (jit, budget) in [(false, 1000), (true, 1), (true, 3), (true, 1000)] {
            let m = native(source, Some(Config::DRAM), jit, budget)?;
            assert_eq!(m.cpu, plain.cpu);
            assert_eq!(m.objekt, plain.objekt);
            assert_eq!(m.stats, plain.stats);
            assert_eq!(m.devices.requests, plain.devices.requests);
            assert_eq!(m.devices.completions, plain.devices.completions);
            assert_eq!(m.cycle_estimate(), expected.cycle_estimate());
        }
        assert!(expected.cycle_estimate().unwrap().cycles().total() > expected.stats.retired);
    }
    Ok(())
}

#[test]
fn reporting_does_not_drain_an_outstanding_reply() -> Result<()> {
    let source = "d=0xa000000064, page=Allocate, size=2, scan=0\nidx=Two, prepare\nmem=Read, prepared, launch\nhalt";
    let mut m = boot::microcode_with_pager(source, 512, 16)?.machine;
    m.enable_cycle_estimate(Config::DRAM)?;
    m.run_steps(3)?;
    let e = m.cycle_estimate().unwrap().clone();
    assert!(e.pending());
    assert!(e.pending_cycles() > 0);
    assert!(e.report(&m.stats, 100.0)?.contains("pending=true"));
    assert_eq!(m.cycle_estimate(), Some(&e));
    m.run_steps(10)?;
    let e = m.cycle_estimate().unwrap();
    assert!(!e.pending());
    assert_eq!(e.cycles().async_wait, 14);
    assert!(e.report(&m.stats, f64::NAN).is_err());
    assert!(m.enable_cycle_estimate(Config::SRAM).is_err());
    Ok(())
}

#[test]
fn iterative_float_allowances_cover_normal_and_exceptional_rtl_paths() -> Result<()> {
    for precision in ["Binary32", "Binary64"] {
        for op in ["Divide", "Sqrt"] {
            for high in [0, 0x3ff00000, 0x7ff00000] {
                // R0:R1 = binary64 operand; R2:R3 = 2.0. In binary32
                // the low words supply an ordinary value or zero.
                let source = format!(
                    "d=0x3f800000, r=Bus, rb=0, ldrb\n\
                    d={high}, r=Bus, rb=1, ldrb\n\
                    d=0x40000000, r=Bus, rb=2, ldrb\n\
                    d=0x40000000, r=Bus, rb=3, ldrb\n\
                    ra=0, rb=2, alu=Float, precision={precision}, fp={op}\nhalt"
                );
                let m = native(&source, Some(Config::SRAM), true, 1000)?;
                let measured = rtl_cycles(&m, Config::SRAM)?;
                let estimate = m.cycle_estimate().unwrap();
                assert_eq!(estimate.work().approximate_floats, 1);
                assert!(
                    estimate.cycles().total() >= measured,
                    "{source}: measured {measured}, {estimate:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn asynchronous_fault_timing_matches_across_engines() -> Result<()> {
    for index in ["Clear", "Two"] {
        let source = format!(
            "d=0xa000000064, page=Allocate, size=2, scan=0\n\
            idx={index}, prepare\nmem=Read, prepared, launch\n\
            d=7, r=Bus, rb=7, ldrb\nd=32, esp=Bus\nhalt"
        );
        let mut expected = None;
        for (jit, budget) in [(false, 1), (false, 100), (true, 1), (true, 100)] {
            let mut m = boot::microcode_with_pager(&source, 512, 16)?.machine;
            m.enable_cycle_estimate(Config::DRAM)?;
            if jit {
                m.enable_jit()?;
            }
            for _ in 0..10 {
                if m.run_steps(budget).is_err() {
                    break;
                }
            }
            assert!(m.fault.is_some());
            assert!(!m.cycle_estimate().unwrap().pending());
            let estimate = m.cycle_estimate().unwrap().clone();
            let result = (m.cpu, m.fault, estimate);
            if let Some(expected) = &expected {
                assert_eq!(&result, expected);
            } else {
                expected = Some(result);
            }
        }
    }
    Ok(())
}
