//! Trace the original saved process on RTL up to the first stage-5 operation.
//! Observation never supplies a guest result, context, or primitive handler.
use eyre::{ensure, Result, WrapErr};
use rekursiv_asm::{Service, Word};
use rekursiv_model::{
    processor::{Image as Program, Processor},
    store::Record,
};
use rekursiv_sim::{
    device::{Bitmap, Clocks, Device, Events, Input, Pointer},
    Harness, Timing,
};
use rekursiv_smalltalk::{audit, interpreter, layout, source, target};

#[derive(Debug)]
struct StageFiveBoundary(u32);
impl std::fmt::Display for StageFiveBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stage-5 primitive {} reached", self.0)
    }
}
impl std::error::Error for StageFiveBoundary {}

#[test]
#[ignore = "requires the pinned Xerox distribution; run scripts/check-smalltalk-image.sh"]
fn saved_image_startup_reaches_stage_five_on_rtl() -> Result<()> {
    let directory = std::env::var("REKURSIV_ST80_DIR")?;
    let bytes = std::fs::read(std::path::Path::new(&directory).join("VirtualImage"))?;
    ensure!(rekursiv_smalltalk::checksum(&bytes) == rekursiv_smalltalk::IMAGE_SHA256);
    let source = source::Image::parse(&bytes)?;
    let reference = std::fs::read_to_string(std::path::Path::new(&directory).join("trace2"))?;
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/xerox-v2.json"))?;
    ensure!(
        rekursiv_smalltalk::checksum(reference.as_bytes())
            == manifest["files"]["trace2"].as_str().unwrap(),
        "startup reference trace does not match the pinned distribution"
    );
    let reference_bytecodes: Vec<u8> = reference
        .lines()
        .filter_map(|line| {
            line.strip_prefix("Bytecode <")?
                .split_once('>')?
                .0
                .parse()
                .ok()
        })
        .collect();
    let context = source.initial_context()?;
    let inventory = audit::inventory(&source)?;
    let converted = target::Image::convert(&source, &layout::BOOT_ROOTS)?;
    converted.verify_against(&source)?;
    let active = layout::reference(context)?;
    let assembly = interpreter::assemble(active)?;
    let mut program = Program::from_assembly(&assembly)?
        .with_ram_collector(interpreter::COLLECTOR_ENTRY, rekursiv_sim::PAGER_ENTRIES)?;
    program.roots[..26].copy_from_slice(&converted.roots[..26]);
    // The saved startup allocates a full display bitmap. Tiny unit-test heaps
    // cannot hold that one object, even after successful collection.
    let rt = rekursiv_sim::runtime_with_memory(131072)?;
    let timing = Timing {
        request_delay: 2,
        memory_latency: 3,
        response_stall: 2,
    };
    let mut h = Harness::new(&rt, timing, None)?;
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
        .find(|r| r.source_oop == context)
        .unwrap();
    h.install(root.reference, root.class, 0, &root.body)?;
    h.service(Service::ReserveIdentities(converted.next_identity))?;
    h.service(Service::CompactClass {
        code: 2,
        class: layout::reference(12)?,
    })?;
    h.load_processor(&program)?;
    let mut device = Device::default();
    device.timing = timing;
    device.events = Some(Events::default());
    device.clocks = Some(Clocks::new(0, 0, 1000));
    device.pointer = Some(Pointer::default());
    device.input = Some(Input::default());
    device.cursor_bitmap = Some(Bitmap::new(16, 16));
    device.display_bitmap = Some(Bitmap::new(1024, 1024));
    h.device = device;
    let services = h.stats.services;
    h.start_processor(assembly.entry.unwrap())?;
    let mut cpu = Processor::default();
    let mut trace = Vec::new();
    let mut bytecodes = Vec::new();
    let mut boundaries = 0usize;
    let result = h.run_processor_observed(&program, &mut cpu, 10_000_000, |h, cpu| {
        let pc = h.rtl.cpu_pc_o;
        if pc == assembly.symbols["cycle"] as u16 {
            boundaries += 1;
        }
        if pc == assembly.symbols["decoded"] as u16 {
            bytecodes.push(cpu.rf[0] as u8);
        }
        if pc == assembly.symbols["primitive_dispatch"] as u16 {
            let number = cpu.rf[0];
            let method = h.oracle.state.vr[7];
            let bindings = inventory
                .primitives
                .get(&(number as u8))
                .into_iter()
                .flatten()
                .find(|m| layout::reference(m.oop).ok() == Some(method))
                .map(|m| m.bindings.join(", "))
                .unwrap_or_default();
            trace.push(format!(
                "bytecode {boundaries}: primitive {number}, method identity {:x}, {bindings}",
                method.identity()?
            ));
            if matches!(number, 96 | 97 | 128) {
                return Err(StageFiveBoundary(number).into());
            }
        }
        Ok(())
    });
    eprintln!(
        "saved context {context:04x}; {boundaries} bytecode boundaries; {} collections\n{}",
        h.stats.collections,
        trace.join("\n")
    );
    let matched = bytecodes
        .iter()
        .zip(&reference_bytecodes)
        .take_while(|(a, b)| a == b)
        .count();
    eprintln!(
        "Xerox bytecode prefix matches {matched} instructions; next RTL {:?}, reference {:?}",
        bytecodes.get(matched),
        reference_bytecodes.get(matched)
    );
    eprintln!(
        "display publications {}, dimensions {:?}; {} device requests",
        h.device.display_bitmap.as_ref().unwrap().publications,
        h.device
            .display_bitmap
            .as_ref()
            .unwrap()
            .visible
            .as_ref()
            .map(|f| (f.width, f.height)),
        h.device.requests.len()
    );
    ensure!(h.stats.services == services, "host service during startup");
    ensure!(
        h.rtl.cpu_fault_o == 0,
        "RTL fault {} at pc {} rf {:?}, last command {:?}",
        h.rtl.cpu_last_status_o,
        cpu.pc,
        cpu.rf,
        h.commands.last()
    );
    match result {
        Err(error) if error.downcast_ref::<StageFiveBoundary>().is_some() => {
            ensure!(
                error.downcast_ref::<StageFiveBoundary>().unwrap().0 == 96,
                "startup reached a different stage-5 dependency"
            );
            ensure!(
                matched == reference_bytecodes.len() && matched == 499,
                "startup diverged from the supplied Xerox bytecode trace"
            );
            ensure!(
                h.stats.collections > 0,
                "startup did not exercise collection"
            );
            ensure!(
                h.device.display_bitmap.as_ref().unwrap().publications == 2,
                "startup did not publish both display registrations"
            );
            let frame = h
                .device
                .display_bitmap
                .as_ref()
                .unwrap()
                .visible
                .as_ref()
                .unwrap();
            ensure!(
                (frame.width, frame.height) == (640, 480),
                "startup display dimensions changed"
            );
            Ok(())
        }
        Err(error) => Err(error).wrap_err_with(|| {
            format!(
                "startup pc {} status {} rf {:?}",
                cpu.pc, cpu.rf[15], cpu.rf
            )
        }),
        Ok(_) => eyre::bail!(
            "startup stopped before a stage-5 boundary: status {}, result {:?}",
            cpu.rf[15],
            Word::from_bits(cpu.object)?
        ),
    }
}
