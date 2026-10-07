//! Real RTL execution with a cycle-independent oracle and adversarial memory timing.
use camino::{Utf8Path, Utf8PathBuf};
use eyre::{ensure, eyre, Result};
use marlin::{
    verilator::{vcd::Vcd, VerilatedModelConfig, VerilatorRuntime, VerilatorRuntimeOptions},
    verilog::prelude::*,
};
use rekursiv_asm::*;
use rekursiv_model::{MemoryEffect, Model, Outcome, State};
pub mod device;
pub mod microprograms;
pub mod processor;
pub mod programs;
mod recovery;
mod store;
use recovery::RecoveryJob;
use rekursiv_model::store::{BackingStore, Record};
use store::PendingStore;

#[verilog(src = "../../objekt_tb.sv", name = "objekt_tb")]
pub struct Objekt;
pub const PAGER_ENTRIES: usize = 16;
pub const MEMORY_WORDS: usize = 512;
pub fn runtime() -> Result<VerilatorRuntime> {
    runtime_with_memory(MEMORY_WORDS)
}
/// Compile the same machine with a different external RAM capacity. The pager,
/// processor, and instruction semantics do not change. Each capacity has its
/// own compilation cache, so small adversarial tests can run alongside image
/// tests without changing global files or rebuilding each other's machine.
pub fn runtime_with_memory(memory_words: usize) -> Result<VerilatorRuntime> {
    ensure!((2..=1 << 24).contains(&memory_words) && memory_words.is_multiple_of(2));
    let root = Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join(format!("artifacts/marlin/memory-{memory_words}"));
    let configuration = directory.join("memory_config.sv");
    {
        // Several tests can ask for the same profile concurrently. Do not let
        // another compiler observe a partially written preprocessor input.
        static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = CONFIG_LOCK.lock().unwrap();
        std::fs::create_dir_all(&directory)?;
        let text = format!("`define REKURSIV_SIM_MEMORY_WORDS {memory_words}\n");
        if std::fs::read_to_string(&configuration).ok().as_ref() != Some(&text) {
            std::fs::write(&configuration, text)?;
        }
    }
    let files = [
        configuration,
        root.join("objekt_tb.sv"),
        root.join("rtl/objekt.sv"),
        root.join("rtl/objekt_transfer.sv"),
        root.join("rtl/objekt_exchange.sv"),
        root.join("rtl/objekt_directory.sv"),
        root.join("rtl/objekt_gc.sv"),
        root.join("rtl/logik.sv"),
        root.join("rtl/logik_io.sv"),
        root.join("rtl/numerik.sv"),
        root.join("rtl/numerik_alu.sv"),
        root.join("rtl/numerik_fp32.sv"),
        root.join("rtl/hardfloat.sv"),
        root.join("rtl/logik_store.sv"),
        root.join("rtl/logik_stacks.sv"),
        root.join("rtl/logik_sequencer.sv"),
    ];
    let refs: Vec<_> = files.iter().map(|p| p.as_path()).collect();
    VerilatorRuntime::new(
        &directory,
        &refs,
        &[
            root.as_path(),
            root.join("rtl").as_path(),
            root.join("vendor/hardfloat/source").as_path(),
            root.join("vendor/hardfloat/source/RISCV").as_path(),
        ],
        [],
        VerilatorRuntimeOptions::default_logging(),
    )
    .map_err(|e| eyre!("Verilator runtime: {e}"))
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Timing {
    pub request_delay: u32,
    pub memory_latency: u32,
    pub response_stall: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statistics {
    pub cycles: u64,
    pub commands: u64,
    pub services: u64,
    pub responses: u64,
    pub reads: u64,
    pub writes: u64,
    pub errors: u64,
    pub store_transactions: u64,
    pub saved_objects: u64,
    pub compactions: u64,
    pub collections: u64,
}
#[derive(Clone, Copy, Debug)]
struct Pending {
    effect: MemoryEffect,
    left: u32,
    error: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Edges {
    pub command: bool,
    pub service: bool,
    pub response: bool,
}

pub struct Harness<'a> {
    // Trace must close before the Verilated model is destroyed.
    trace: Option<Vcd<'a>>,
    pub rtl: Objekt<'a>,
    pub oracle: Model,
    pub backing: Vec<Word>,
    pub timing: Timing,
    pub stats: Statistics,
    pending: Option<Pending>,
    request_wait: u32,
    pub fail_next_memory: bool,
    held_request: Option<MemoryEffect>,
    held_response: Option<(u8, u64)>,
    pub transfers: Vec<MemoryEffect>,
    pub commands: Vec<(Ports, Response)>,
    ticks: u64,
    pub store: BackingStore,
    pub device: device::Device,
    pub store_requests: Vec<StoreRequest>,
    pub store_timing: Timing,
    pending_store: Option<PendingStore>,
    store_wait: u32,
    held_store: Option<StoreRequest>,
    memory_fault: Option<usize>,
    store_fault: Option<usize>,
    recovery_job: Option<RecoveryJob>,
}
impl<'a> Harness<'a> {
    pub fn new(
        runtime: &'a VerilatorRuntime,
        timing: Timing,
        trace: Option<&Utf8Path>,
    ) -> Result<Self> {
        let mut rtl = runtime
            .create_model::<Objekt>(&VerilatedModelConfig {
                enable_tracing: trace.is_some(),
                ..Default::default()
            })
            .map_err(|e| eyre!("Build RTL model: {e:?}"))?;
        rtl.eval();
        let memory_words = rtl.dbg_memory_words_o as usize;
        ensure!(
            (2..=1 << 24).contains(&memory_words),
            "invalid RTL RAM capacity"
        );
        let trace = trace.map(|p| rtl.open_vcd(p.as_str()));
        let mut h = Self {
            trace,
            rtl,
            oracle: Model::new(PAGER_ENTRIES, memory_words),
            backing: vec![Word::ZERO; memory_words],
            timing,
            stats: Statistics::default(),
            pending: None,
            request_wait: 0,
            fail_next_memory: false,
            held_request: None,
            held_response: None,
            transfers: Vec::new(),
            commands: Vec::new(),
            ticks: 0,
            store: BackingStore::default(),
            device: device::Device::default(),
            store_requests: Vec::new(),
            store_timing: timing,
            pending_store: None,
            store_wait: 0,
            held_store: None,
            memory_fault: None,
            store_fault: None,
            recovery_job: None,
        };
        h.reset()?;
        h.device.timing = timing;
        Ok(h)
    }
    fn eval(&mut self) {
        self.rtl.eval();
        if let Some(t) = self.trace.as_mut() {
            t.dump(self.ticks);
        }
        self.ticks += 1;
    }
    pub fn reset(&mut self) -> Result<()> {
        self.rtl.cpu_enable_i = 0;
        self.rtl.cpu_gc_enable_i = 0;
        self.rtl.cpu_gc_entry_i = 0;
        self.rtl.cpu_start_i = 0;
        self.rtl.cpu_boot_valid_i = 0;
        self.rtl.cpu_resume_i = 0;
        self.rtl.cpu_irq_i = 0;
        self.rtl.io_ready_i = 0;
        self.rtl.io_response_i = 0;
        self.rtl.io_error_i = 0;
        self.rtl.io_result_i = 0;
        self.rtl.cpu_command_enable_i = 1;
        self.rtl.cpu_response_enable_i = 1;
        self.rtl.cmd_valid_i = 0;
        self.rtl.svc_valid_i = 0;
        self.rtl.rsp_ready_i = 0;
        self.rtl.run_i = 0;
        self.rtl.rst_i = 1;
        self.rtl.store_ready_i = 0;
        self.rtl.store_rsp_valid_i = 0;
        self.rtl.mem_ready_i = 0;
        self.rtl.mem_rsp_valid_i = 0;
        self.rtl.mem_rsp_error_i = 0;
        self.rtl.mem_rsp_data_i = 0;
        self.rtl.clk_i = 0;
        self.eval();
        self.rtl.clk_i = 1;
        self.eval();
        self.rtl.clk_i = 0;
        self.eval();
        self.rtl.rst_i = 0;
        self.eval();
        self.oracle.reset();
        self.recovery_job = None;
        self.store = BackingStore::default();
        self.device = device::Device::default();
        self.store_requests.clear();
        self.pending_store = None;
        self.store_wait = 0;
        self.held_store = None;
        self.memory_fault = None;
        self.store_fault = None;
        self.backing.fill(Word::ZERO);
        self.pending = None;
        self.request_wait = 0;
        self.held_request = None;
        self.held_response = None;
        self.fail_next_memory = false;
        self.transfers.clear();
        self.commands.clear();
        self.stats = Statistics::default();
        self.compare_state()
    }
    fn request(&self) -> MemoryEffect {
        MemoryEffect {
            address: self.rtl.mem_addr_o,
            write: self.rtl.mem_write_o != 0,
            value: Word::from_bits(self.rtl.mem_data_o).unwrap(),
        }
    }
    /// One full clock with independent request acceptance and completion delays.
    pub fn tick(&mut self) -> Result<Edges> {
        self.rtl.clk_i = 0;
        self.rtl.eval();
        if let Some(req) = self.held_request {
            ensure!(
                self.rtl.mem_valid_o != 0 && self.request() == req,
                "memory request changed under backpressure"
            );
        }
        if let Some(rsp) = self.held_response {
            ensure!(
                self.rtl.rsp_valid_o != 0 && (self.rtl.rsp_status_o, self.rtl.rsp_data_o) == rsp,
                "response changed under backpressure"
            );
        }
        self.rtl.mem_ready_i =
            (self.pending.is_none() && self.request_wait >= self.timing.request_delay) as u8;
        self.rtl.mem_rsp_valid_i = 0;
        self.rtl.mem_rsp_error_i = 0;
        if let Some(p) = self.pending {
            if p.left == 0 {
                self.rtl.mem_rsp_valid_i = 1;
                self.rtl.mem_rsp_error_i = p.error as u8;
                self.rtl.mem_rsp_data_i = if p.effect.write {
                    p.effect.value.bits()
                } else {
                    self.backing[p.effect.address as usize].bits()
                };
            }
        }
        self.tick_store()?;
        self.tick_device()?;
        self.rtl.eval();
        let edges = Edges {
            command: (if self.rtl.cpu_enable_i != 0 {
                self.rtl.cpu_cmd_valid_o & self.rtl.cpu_command_enable_i
            } else {
                self.rtl.cmd_valid_i
            }) != 0
                && self.rtl.cmd_ready_o != 0,
            service: self.rtl.svc_valid_i != 0 && self.rtl.svc_ready_o != 0,
            response: self.rtl.rsp_valid_o != 0
                && (if self.rtl.cpu_enable_i != 0 {
                    self.rtl.cpu_rsp_ready_o & self.rtl.cpu_response_enable_i
                } else {
                    self.rtl.rsp_ready_i
                }) != 0,
        };
        let request = self.rtl.mem_valid_o != 0 && self.rtl.mem_ready_i != 0;
        let complete = self.rtl.mem_rsp_valid_i != 0 && self.rtl.mem_rsp_ready_o != 0;
        self.held_request = if self.rtl.mem_valid_o != 0 && !request {
            Some(self.request())
        } else {
            None
        };
        self.held_response = if self.rtl.rsp_valid_o != 0 && !edges.response {
            Some((self.rtl.rsp_status_o, self.rtl.rsp_data_o))
        } else {
            None
        };
        if complete {
            let p = self.pending.take().unwrap();
            if p.error {
                self.stats.errors += 1;
            } else if p.effect.write {
                self.backing[p.effect.address as usize] = p.effect.value;
                self.stats.writes += 1;
            } else {
                self.stats.reads += 1;
            }
        } else if let Some(p) = self.pending.as_mut() {
            p.left = p.left.saturating_sub(1);
        }
        if request {
            ensure!(
                self.pending.is_none(),
                "second outstanding memory operation"
            );
            let effect = self.request();
            ensure!(
                (effect.address as usize) < self.backing.len(),
                "RTL issued out-of-range request"
            );
            self.transfers.push(effect);
            self.pending = Some(Pending {
                effect,
                left: self.timing.memory_latency,
                error: self.fail_next_memory || self.memory_fault == Some(self.transfers.len() - 1),
            });
            self.fail_next_memory = false;
            self.request_wait = 0;
        } else if self.rtl.mem_valid_o != 0 {
            self.request_wait += 1;
        } else {
            self.request_wait = 0;
        }
        self.eval();
        self.rtl.clk_i = 1;
        self.eval();
        self.rtl.clk_i = 0;
        self.eval();
        // External interrupt changes take effect after the sampled edge. The
        // instruction oracle and RTL then see the same level at next issue.
        if let Some(events) = &self.device.events {
            self.rtl.cpu_irq_i = u8::from(events.status() != 0);
            self.eval();
        }
        self.stats.cycles += 1;
        self.stats.commands += edges.command as u64;
        self.stats.services += edges.service as u64;
        self.stats.responses += edges.response as u64;
        Ok(edges)
    }
    pub fn drive(&mut self, p: Ports) -> Result<()> {
        // Permit reserved opcode values for negative RTL tests, but reject host-width truncation.
        ensure!(
            p.alloc_size < ADDRESS_LIMIT
                && p.alloc_scan < 2
                && p.pager < 16
                && p.index < 16
                && p.register < 8
                && p.memory < 4
                && p.read < 16
                && p.vr < 8
                && p.load_vr < 2
                && p.check_type < 2
                && p.data <= WORD_MASK
                && p.expected_type <= WORD_MASK,
            "port value exceeds wire width"
        );
        self.rtl.alloc_size_i = p.alloc_size;
        self.rtl.alloc_scan_i = p.alloc_scan;
        self.rtl.pager_i = p.pager;
        self.rtl.index_i = p.index;
        self.rtl.register_i = p.register;
        self.rtl.memory_i = p.memory;
        self.rtl.read_i = p.read;
        self.rtl.load_vr_i = p.load_vr;
        self.rtl.vr_i = p.vr;
        self.rtl.data_i = p.data;
        self.rtl.check_type_i = p.check_type;
        self.rtl.expected_type_i = p.expected_type;
        Ok(())
    }
    pub fn execute(&mut self, c: Command) -> Result<Response> {
        self.execute_raw(c.encode()?, false)
    }
    pub fn execute_raw(&mut self, p: Ports, memory_error: bool) -> Result<Response> {
        self.execute_faults(
            p,
            Faults {
                memory_at: memory_error.then_some(0),
                store_at: None,
            },
        )
    }
    pub fn execute_faults(&mut self, p: Ports, faults: Faults) -> Result<Response> {
        self.processor_host_access()?;
        ensure!(
            self.recovery_job.is_none() && self.rtl.dbg_maintenance_o == 0,
            "mutator is locked for recovery"
        );
        let store_start = self.store_requests.len();
        self.memory_fault = faults.memory_at.map(|n| self.transfers.len() + n);
        self.store_fault = faults.store_at.map(|n| store_start + n);
        let start = self.transfers.len();
        let responses = self.stats.responses;
        self.drive(p)?;
        self.rtl.run_i = 1;
        self.rtl.cmd_valid_i = 1;
        self.rtl.svc_valid_i = 0;
        self.rtl.rsp_ready_i = 0;
        self.fail_next_memory = false;
        let mut accepted = false;
        for _ in 0..10000 {
            if self.tick()?.command {
                accepted = true;
                break;
            }
        }
        ensure!(accepted, "command acceptance timeout: {p:?}");
        self.rtl.cmd_valid_i = 0;
        let response = self.finish()?;
        let expected = self.oracle.execute_faults(p, faults);
        ensure!(
            self.store_requests[store_start..] == expected.store,
            "backing-store request sequence mismatch: actual {:?}, expected {:?}",
            &self.store_requests[store_start..],
            expected.store
        );
        self.memory_fault = None;
        self.store_fault = None;
        self.verify(expected, response, start, responses)
            .map_err(|e| eyre!("command {p:?}: {e}"))?;
        self.commands.push((p, response));
        Ok(response)
    }
    pub fn service(&mut self, s: Service) -> Result<Response> {
        self.service_with_error(s, false)
    }
    pub fn service_with_error(&mut self, s: Service, memory_error: bool) -> Result<Response> {
        ensure!(
            self.recovery_job.is_none(),
            "resume the pending recovery before other service operations"
        );
        self.service_inner(s, memory_error)
    }
    fn service_inner(&mut self, s: Service, memory_error: bool) -> Result<Response> {
        self.processor_host_access()?;
        let start = self.transfers.len();
        let responses = self.stats.responses;
        self.rtl.run_i = 0;
        self.rtl.cmd_valid_i = 0;
        self.rtl.svc_valid_i = 1;
        self.rtl.rsp_ready_i = 0;
        match s {
            Service::ReserveIdentities(next) => {
                ensure!(next < 1 << 40, "identity floor exceeds wire width");
                self.rtl.svc_op_i = 8;
                self.rtl.svc_repr_i = next;
            }
            Service::ReadMemory { address } => {
                ensure!(address < ADDRESS_LIMIT, "address exceeds wire width");
                self.rtl.svc_op_i = 4;
                self.rtl.svc_base_i = address;
            }
            Service::BeginRecovery => self.rtl.svc_op_i = 5,
            Service::SetBodyCursor(cursor) => {
                self.rtl.svc_op_i = 6;
                self.rtl.svc_repr_i = cursor as u64;
            }
            Service::EndRecovery => self.rtl.svc_op_i = 7,

            Service::Install(e) => {
                ensure!(
                    e.base < ADDRESS_LIMIT && e.size < ADDRESS_LIMIT,
                    "service metadata exceeds wire width"
                );
                self.rtl.svc_op_i = 0;
                self.rtl.svc_ref_i = e.reference.bits();
                self.rtl.svc_class_i = e.class.bits();
                self.rtl.svc_base_i = e.base;
                self.rtl.svc_size_i = e.size;
                self.rtl.svc_repr_i = e.representation.bits();
                self.rtl.svc_flags_i = e.flags();
            }
            Service::Invalidate(r) => {
                self.rtl.svc_op_i = 1;
                self.rtl.svc_ref_i = r.bits();
            }
            Service::CompactClass { code, class } => {
                ensure!(code < 64, "compact code exceeds wire width");
                self.rtl.svc_op_i = 2;
                self.rtl.svc_code_i = code;
                self.rtl.svc_class_i = class.bits();
            }
            Service::WriteMemory { address, value } => {
                ensure!(address < ADDRESS_LIMIT, "address exceeds wire width");
                self.rtl.svc_op_i = 3;
                self.rtl.svc_base_i = address;
                self.rtl.svc_repr_i = value.bits();
            }
        }
        self.fail_next_memory = memory_error;
        let mut accepted = false;
        for _ in 0..10000 {
            if self.tick()?.service {
                accepted = true;
                break;
            }
        }
        ensure!(accepted, "service acceptance timeout");
        self.rtl.svc_valid_i = 0;
        let response = self.finish()?;
        let expected = self.oracle.service(s, memory_error);
        self.verify(expected, response, start, responses)
            .map_err(|e| eyre!("service {s:?}: {e}"))?;
        Ok(response)
    }
    fn finish(&mut self) -> Result<Response> {
        for _ in 0..100000 {
            if self.rtl.rsp_valid_o != 0 {
                break;
            }
            self.tick()?;
        }
        ensure!(self.rtl.rsp_valid_o != 0, "response timeout");
        let response = Response {
            status: Status::try_from(self.rtl.rsp_status_o)?,
            data: Word::from_bits(self.rtl.rsp_data_o)?,
        };
        for _ in 0..self.timing.response_stall {
            self.tick()?;
        }
        self.rtl.rsp_ready_i = 1;
        ensure!(self.tick()?.response, "response disappeared");
        self.rtl.rsp_ready_i = 0;
        self.fail_next_memory = false;
        Ok(response)
    }
    fn verify(
        &mut self,
        expected: Outcome,
        response: Response,
        start: usize,
        responses: u64,
    ) -> Result<()> {
        ensure!(
            response == expected.response,
            "response {response:?} != model {:?}",
            expected.response
        );
        ensure!(
            self.stats.responses == responses + 1,
            "expected exactly one response"
        );
        let expected_effect: Vec<_> = expected.memory.into_iter().collect();
        ensure!(
            self.transfers[start..] == expected_effect,
            "memory effects {:?} != {:?}",
            &self.transfers[start..],
            expected_effect
        );
        ensure!(
            self.pending.is_none(),
            "memory operation remained after retirement"
        );
        self.compare_state()
    }
    pub fn snapshot(&mut self) -> Result<State> {
        let mut vr = [Word::NIL; 8];
        for (i, w) in vr.iter_mut().enumerate() {
            self.rtl.dbg_vr_i = i as u8;
            self.rtl.eval();
            *w = Word::from_bits(self.rtl.dbg_vr_o)?;
        }
        let selected = if self.rtl.dbg_selected_o != 0 {
            Some(Entry {
                reference: Word::from_bits(self.rtl.dbg_ref_o)?,
                class: Word::from_bits(self.rtl.dbg_class_o)?,
                size: self.rtl.dbg_size_o,
                base: self.rtl.dbg_base_o,
                representation: Word::from_bits(self.rtl.dbg_repr_o)?,
                new: self.rtl.dbg_flags_o & 1 != 0,
                modified: self.rtl.dbg_flags_o & 2 != 0,
                cond: self.rtl.dbg_flags_o & 4 != 0,
            })
        } else {
            None
        };
        Ok(State {
            vr,
            index: Word::from_bits(self.rtl.dbg_idx_o)?.as_index(),
            index_reg: Word::from_bits(self.rtl.dbg_reg_o)?.as_index(),
            selected,
        })
    }
    pub fn compare_state(&mut self) -> Result<()> {
        ensure!(
            (self.rtl.dbg_maintenance_o != 0) == self.oracle.maintenance,
            "maintenance lock differs"
        );
        for code in 0..4 {
            self.rtl.dbg_class_code_i = code as u8;
            self.rtl.eval();
            let class = if self.rtl.dbg_class_valid_o != 0 {
                Some(Word::from_bits(self.rtl.dbg_compact_class_o)?)
            } else {
                None
            };
            ensure!(
                class == self.oracle.classes[code],
                "compact root mapping differs"
            );
        }
        ensure!(
            self.rtl.dbg_next_identity_o == self.oracle.next_identity,
            "identity high-water mismatch: {} != {}",
            self.rtl.dbg_next_identity_o,
            self.oracle.next_identity
        );
        ensure!(
            self.rtl.dbg_body_cursor_o == self.oracle.body_cursor,
            "body cursor mismatch: {} != {}",
            self.rtl.dbg_body_cursor_o,
            self.oracle.body_cursor
        );
        ensure!(
            self.store == self.oracle.store,
            "backing store differs from model"
        );
        let actual = self.snapshot()?;
        ensure!(
            actual == self.oracle.state,
            "state mismatch\nRTL: {actual:?}\nmodel: {:?}",
            self.oracle.state
        );
        ensure!(self.backing == self.oracle.memory, "memory contents differ");
        for (i, expected) in self.oracle.entries.iter().enumerate() {
            self.rtl.dbg_slot_i = i as u8;
            self.rtl.eval();
            let actual = if self.rtl.dbg_valid_o != 0 {
                Some(Entry {
                    reference: Word::from_bits(self.rtl.dbg_entry_ref_o)?,
                    class: Word::from_bits(self.rtl.dbg_entry_class_o)?,
                    size: self.rtl.dbg_entry_size_o,
                    base: self.rtl.dbg_entry_base_o,
                    representation: Word::from_bits(self.rtl.dbg_entry_repr_o)?,
                    new: self.rtl.dbg_entry_flags_o & 1 != 0,
                    modified: self.rtl.dbg_entry_flags_o & 2 != 0,
                    cond: self.rtl.dbg_entry_flags_o & 4 != 0,
                })
            } else {
                None
            };
            ensure!(
                &actual == expected,
                "pager slot {i}: RTL {actual:?} != model {expected:?}"
            );
        }
        Ok(())
    }
    pub fn install(
        &mut self,
        reference: Word,
        class: Word,
        base: u32,
        body: &[Word],
    ) -> Result<()> {
        ensure!(
            body.len() < ADDRESS_LIMIT as usize
                && base as u64 + body.len() as u64 <= self.backing.len() as u64,
            "object exceeds memory"
        );
        for (i, value) in body.iter().enumerate() {
            let response = self.service(Service::WriteMemory {
                address: base + i as u32,
                value: *value,
            })?;
            ensure!(
                response.status == Status::Ok,
                "body initialization: {:?}",
                response.status
            );
        }
        let response = self.service(Service::Install(Entry {
            reference,
            class,
            base,
            size: body.len() as u32,
            representation: body.first().copied().unwrap_or(Word::NIL),
            new: false,
            modified: false,
            cond: false,
        }))?;
        ensure!(
            response.status == Status::Ok,
            "metadata initialization: {:?}",
            response.status
        );
        let record = Record {
            reference,
            class,
            cond: false,
            body: body.to_vec(),
        };
        self.store
            .records
            .insert(reference.identity()?, record.clone());
        self.oracle
            .store
            .records
            .insert(reference.identity()?, record);
        Ok(())
    }
    pub fn flush_trace(&mut self) {
        if let Some(t) = self.trace.as_mut() {
            t.flush();
        }
    }
}
pub fn trace_path(path: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(path)
}
