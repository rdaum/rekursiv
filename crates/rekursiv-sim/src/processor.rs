//! Loading and observing an autonomous RTL processor. The host does not choose its next instruction.
use crate::{Harness, Result};
use eyre::ensure;
use rekursiv_asm::{Ports, Response, Status, Word};
use rekursiv_model::processor::{Image, Processor, STACK_WORDS};

impl Harness<'_> {
    pub(super) fn processor_host_access(&mut self) -> Result<()> {
        ensure!(
            self.rtl.cpu_halted_o != 0 || self.rtl.cpu_service_o != 0,
            "host access requires a halted processor or service break"
        );
        self.rtl.cpu_enable_i = 0;
        Ok(())
    }
    pub fn processor_roots(&mut self) -> Result<Vec<Word>> {
        ensure!(
            self.rtl.cpu_halted_o != 0 || self.rtl.cpu_service_o != 0,
            "processor roots require a stopped processor"
        );
        let mut words = vec![
            Word::from_bits(self.rtl.cpu_estkr_o)?,
            Word::from_bits(self.rtl.cpu_symbol_o)?,
            Word::from_bits(self.rtl.cpu_object_o)?,
        ];
        for address in 0..STACK_WORDS {
            self.rtl.cpu_dbg_addr_i = address as u16;
            self.rtl.eval();
            if address <= self.rtl.cpu_sp_o as usize {
                words.push(Word::from_bits(self.rtl.cpu_dbg_estk_o)?);
            }
        }
        for address in 0..rekursiv_model::processor::CODE_WORDS {
            self.rtl.cpu_dbg_addr_i = address as u16;
            self.rtl.eval();
            if self.rtl.cpu_dbg_code_valid_o != 0 {
                words.push(Word::from_bits(self.rtl.cpu_dbg_code_data_o)?);
                words.push(Word::from_bits(self.rtl.cpu_dbg_code_type_o)?);
            }
        }
        Ok(words)
    }
    pub fn resume_processor(&mut self) -> Result<()> {
        ensure!(
            self.rtl.cpu_service_o != 0
                && self.recovery_job.is_none()
                && self.rtl.dbg_maintenance_o == 0,
            "processor cannot resume outside a completed service break"
        );
        self.rtl.cpu_enable_i = 1;
        self.rtl.cpu_resume_i = 1;
        self.tick()?;
        self.rtl.cpu_resume_i = 0;
        Ok(())
    }

    pub fn load_processor(&mut self, image: &Image) -> Result<()> {
        ensure!(
            self.rtl.cpu_halted_o != 0,
            "processor must be halted before programming"
        );
        self.rtl.cpu_enable_i = 0;
        self.rtl.cmd_valid_i = 0;
        self.rtl.svc_valid_i = 0;
        self.rtl.cpu_gc_enable_i = image.collector_entry.is_some() as u8;
        if image.collector_entry.is_some() {
            self.oracle.allocation_limit = crate::MEMORY_WORDS as u32 / 2;
        }
        self.rtl.cpu_gc_entry_i = image.collector_entry.unwrap_or(0);
        for (address, word) in image.roots.iter().enumerate() {
            self.program_lane(3, address, 0, word.bits() as u32)?;
            self.program_lane(3, address, 1, (word.bits() >> 32) as u32)?;
        }
        for (address, instruction) in image.code.iter().enumerate() {
            if let Some(i) = instruction {
                for (lane, data) in i.encode()?.into_iter().enumerate() {
                    self.program_lane(0, address, lane, data)?;
                }
            }
        }
        for (address, word) in image.nam.iter().enumerate() {
            if let Some(word) = word {
                self.program_lane(1, address, 0, *word as u32)?;
                self.program_lane(1, address, 1, (*word >> 32) as u32)?;
            }
        }
        for (address, target) in image.map.iter().enumerate() {
            if let Some(target) = target {
                self.program_lane(2, address, 0, *target as u32)?;
            }
        }
        Ok(())
    }
    pub fn program_lane(
        &mut self,
        space: u8,
        address: usize,
        lane: usize,
        data: u32,
    ) -> Result<()> {
        ensure!(
            address <= u16::MAX as usize && lane < 8 && space < 4,
            "invalid programming address"
        );
        self.rtl.cpu_boot_space_i = space;
        self.rtl.cpu_boot_addr_i = address as u16;
        self.rtl.cpu_boot_lane_i = lane as u8;
        self.rtl.cpu_boot_data_i = data;
        self.rtl.cpu_boot_valid_i = 1;
        self.rtl.eval();
        ensure!(
            self.rtl.cpu_boot_ready_o != 0,
            "programming interface unavailable"
        );
        self.tick()?;
        self.rtl.cpu_boot_valid_i = 0;
        Ok(())
    }
    pub fn start_processor(&mut self, entry: u16) -> Result<()> {
        ensure!(self.rtl.cpu_halted_o != 0, "processor is already active");
        self.rtl.cpu_enable_i = 1;
        self.rtl.cpu_entry_i = entry;
        self.rtl.cpu_start_i = 1;
        self.tick()?;
        self.rtl.cpu_start_i = 0;
        Ok(())
    }
    pub fn processor_ports(&self) -> Ports {
        Ports {
            pager: self.rtl.cpu_pager_o,
            index: self.rtl.cpu_index_o,
            register: self.rtl.cpu_register_o,
            memory: self.rtl.cpu_memory_o,
            read: self.rtl.cpu_read_o,
            load_vr: self.rtl.cpu_load_vr_o,
            vr: self.rtl.cpu_vr_o,
            alloc_size: self.rtl.cpu_alloc_size_o,
            alloc_scan: self.rtl.cpu_alloc_scan_o,
            data: self.rtl.cpu_data_o,
            check_type: self.rtl.cpu_check_type_o,
            expected_type: self.rtl.cpu_expected_type_o,
        }
    }
    pub fn compare_processor(&mut self, p: &Processor) -> Result<()> {
        let a = &self.rtl;
        ensure!(
            (a.cpu_pc_o, a.cpu_upcor_o, a.cpu_mark_o, a.cpu_ucar_o)
                == (p.pc, p.upcor, p.mark, p.ucar),
            "sequencer mismatch: RTL pc={}, model pc={}",
            a.cpu_pc_o,
            p.pc
        );
        ensure!(
            (
                a.cpu_sp_o,
                a.cpu_esp_o,
                a.cpu_csp_o,
                a.cpu_ap_o,
                a.cpu_apc_o
            ) == (p.sp, p.esp, p.csp, p.ap, p.apc),
            "processor pointer mismatch at {}",
            p.pc
        );
        ensure!(
            (a.cpu_estkr_o, a.cpu_cstkr_o, a.cpu_symbol_o, a.cpu_object_o)
                == (p.estkr, p.cstkr, p.symbol, p.object),
            "cached word mismatch at {}",
            p.pc
        );
        ensure!(
            (
                a.cpu_q_o,
                a.cpu_product_o,
                a.cpu_flags_o,
                a.cpu_lastcc_o != 0
            ) == (p.q, p.product, p.flags, p.lastcc),
            "arithmetic mismatch at {}",
            p.pc
        );
        ensure!(
            a.cpu_namarg_o == p.namarg
                && (a.cpu_halted_o != 0) == p.halted
                && (a.cpu_service_o != 0) == p.service,
            "processor execution state mismatch"
        );
        if p.service {
            ensure!(
                a.cpu_service_code_o == p.service_code,
                "service code mismatch"
            );
        }
        for j in 0..STACK_WORDS {
            self.rtl.cpu_dbg_addr_i = j as u16;
            self.rtl.eval();
            ensure!(
                self.rtl.cpu_dbg_estk_o == p.estk[j] && self.rtl.cpu_dbg_cstk_o == p.cstk[j],
                "stack slot {j} mismatch"
            );
            if j < 16 {
                ensure!(
                    self.rtl.cpu_dbg_rf_o == p.rf[j],
                    "register {j} mismatch: {} != {}",
                    self.rtl.cpu_dbg_rf_o,
                    p.rf[j]
                );
            }
        }
        Ok(())
    }
    /// Run until halt or a service break, checking every retirement and OBJEKT transaction.
    pub fn run_processor(
        &mut self,
        image: &Image,
        model: &mut Processor,
        limit: usize,
    ) -> Result<usize> {
        self.run_processor_observed(image, model, limit, |_, _| Ok(()))
    }

    /// Observe checked retirements without handing execution to the observer.
    /// The immutable harness exposes device state and transaction history only;
    /// an observer cannot inject commands or choose the next microinstruction.
    pub fn run_processor_observed(
        &mut self,
        image: &Image,
        model: &mut Processor,
        limit: usize,
        mut observe: impl FnMut(&Self, &Processor) -> Result<()>,
    ) -> Result<usize> {
        let mut retired = 0;
        let mut candidate = None;
        let mut expected_object = None;
        let mut transaction_start = (0, 0, 0);
        let mut response = None;
        let mut wait = 0u32;
        let mut from_upper = self.oracle.allocation_limit as usize == crate::MEMORY_WORDS;
        let mut collection: Option<std::result::Result<rekursiv_model::Model, Status>> = None;
        for cycle in 0..limit {
            if self.rtl.cpu_halted_o != 0 || self.rtl.cpu_service_o != 0 {
                return Ok(retired);
            }
            if self.rtl.cpu_gc_active_o != 0 {
                // This model is only a test comparison. No planned address,
                // root list, or collector decision is sent to the executor.
                self.tick()?;
                if self.rtl.cpu_gc_active_o == 0 {
                    ensure!(
                        self.rtl.cpu_fault_o == 0,
                        "machine collector failed: status {}",
                        self.rtl.cpu_last_status_o
                    );
                    self.oracle = collection
                        .take()
                        .expect("collector entry observed")
                        .map_err(|e| eyre::eyre!("RTL collected despite model error {e:?}"))?;
                    from_upper = !from_upper;
                    self.compare_state()?;
                    self.compare_processor(model)?;
                }
                continue;
            }
            if candidate.is_none() {
                candidate = Some(model.prepare(image, self.rtl.cpu_irq_i != 0));
            }
            // Periodic command backpressure and independently delayed response consumption.
            self.rtl.cpu_command_enable_i = (cycle % 7 >= 3) as u8;
            self.rtl.cpu_response_enable_i = (wait >= self.timing.response_stall) as u8;
            self.rtl.eval();
            if self.rtl.rsp_valid_o != 0 {
                wait += 1;
            } else {
                wait = 0;
            }
            let ports = self.processor_ports();
            if self.rtl.cpu_cmd_valid_o != 0
                && self.rtl.cpu_command_enable_i != 0
                && self.rtl.cmd_ready_o != 0
            {
                let expected_command = candidate
                    .as_ref()
                    .unwrap()
                    .as_ref()
                    .map_err(|e| eyre::eyre!("RTL issued command despite predicted fault {e}"))?
                    .1;
                ensure!(
                    Some(ports) == expected_command.map(|c| c.encode()).transpose()?,
                    "processor command differs from model"
                );
                ensure!(expected_object.is_none(), "processor reissued command");
                transaction_start = (
                    self.transfers.len(),
                    self.store_requests.len(),
                    self.stats.responses,
                );
                expected_object = Some(self.oracle.execute_raw(ports, false));
            }
            let received = self.rtl.rsp_valid_o != 0
                && self.rtl.cpu_rsp_ready_o != 0
                && self.rtl.cpu_response_enable_i != 0;
            if received {
                response = Some(Response {
                    status: Status::try_from(self.rtl.rsp_status_o)?,
                    data: Word::from_bits(self.rtl.rsp_data_o)?,
                });
            }
            self.tick()?;
            if received {
                let expected = expected_object
                    .take()
                    .ok_or_else(|| eyre::eyre!("unsolicited OBJEKT response"))?;
                let rsp = response.unwrap();
                ensure!(
                    self.store_requests[transaction_start.1..] == expected.store,
                    "processor store transactions differ"
                );
                self.verify(expected, rsp, transaction_start.0, transaction_start.2)?;
                self.commands.push((ports, rsp));
            }
            if self.rtl.cpu_gc_active_o != 0 {
                let mut expected = self.oracle.clone();
                let mut roots = vec![
                    Word::from_bits(model.estkr)?,
                    Word::from_bits(model.symbol)?,
                    Word::from_bits(model.object)?,
                    Word::from_bits(ports.data)?,
                ];
                roots.extend(
                    model.estk[..=model.sp as usize]
                        .iter()
                        .map(|&w| Word::from_bits(w).unwrap()),
                );
                roots.extend(image.roots);
                for i in image.code.iter().flatten() {
                    roots.push(i.data);
                    if let Some(c) = i.object {
                        roots.extend(c.expected_type);
                    }
                }
                // Fetch learns the required size from backing metadata; no
                // backing reads are performed by the collector itself.
                if ports.pager == 5 {
                    if let Some(record) = self
                        .store
                        .records
                        .get(&Word::from_bits(ports.data)?.identity()?)
                    {
                        roots.push(record.class);
                    }
                }
                let needed = if ports.pager == 5 {
                    self.store
                        .records
                        .get(&Word::from_bits(ports.data)?.identity()?)
                        .map_or(0, |r| r.body.len() as u32)
                } else {
                    ports.alloc_size
                };
                collection = Some(
                    expected
                        .collect_ram(&roots, from_upper, needed)
                        .map(|()| expected),
                );
                response = None;
                self.stats.collections += 1;
                continue;
            }
            if self.rtl.cpu_fault_o != 0 {
                return Err(eyre::eyre!(
                    "processor fault {} at pc {}",
                    self.rtl.cpu_fault_o,
                    self.rtl.cpu_pc_o
                ));
            }
            if self.rtl.cpu_retire_o != 0 {
                let (mut next, _) = candidate
                    .take()
                    .unwrap()
                    .map_err(|e| eyre::eyre!("RTL retired instruction with model fault {e}"))?;
                if let Some(rsp) = response.take() {
                    next.object = rsp.data.bits();
                }
                *model = next;
                self.compare_processor(model)?;
                retired += 1;
                observe(self, model)?;
            } else {
                // Architectural state must remain stable during request and response stalls.
                self.compare_processor(model)?;
            }
        }
        Err(eyre::eyre!(
            "processor cycle limit exceeded at pc {}",
            self.rtl.cpu_pc_o
        ))
    }
}

/// A control-store program that allocates through collisions and reloads its first object.
pub fn assembly_image(source: &str) -> Result<(Image, u16)> {
    use rekursiv_model::processor::{CODE_WORDS, STACK_WORDS};
    let assembly = rekursiv_asm::text::assemble(
        source,
        0,
        &[
            ("CODE_WORDS", CODE_WORDS as i64),
            ("STACK_WORDS", STACK_WORDS as i64),
            ("PAGER_ENTRIES", crate::PAGER_ENTRIES as i64),
            (
                "ROOT_COUNT",
                (20 + STACK_WORDS + 2 * CODE_WORDS + 32) as i64,
            ),
        ],
    )?;
    let entry = assembly.entry.unwrap_or(0);
    let mut image = Image::from_assembly(&assembly)?;
    if image.collector_entry.is_none() {
        image = image.with_ram_collector(128, crate::PAGER_ENTRIES)?;
    }
    Ok((image, entry))
}

pub fn allocation_image() -> Result<Image> {
    Ok(assembly_image(include_str!("../../../microcode/allocation.uc"))?.0)
}

pub fn allocation_example(h: &mut Harness<'_>) -> Result<String> {
    let image = allocation_image()?;
    h.load_processor(&image)?;
    h.start_processor(0)?;
    let mut model = Processor::default();
    let retired = h.run_processor(&image, &mut model, 100000)?;
    ensure!(
        model.rf[0] == 0 && model.estkr == Word::signed(1234).bits() && h.stats.saved_objects >= 5,
        "processor allocation example failed"
    );
    Ok(format!("{retired} RTL microinstructions; 21 allocations, dirty eviction and refill, recovered field=1234"))
}

/// Repeated allocations force autonomous collections. The Rust loop only clocks
/// devices and checks results; the collector executes from the loaded image.
pub fn collection_example(h: &mut Harness<'_>) -> Result<String> {
    let (image, entry) = assembly_image(include_str!("../../../microcode/collection.uc"))?;
    h.load_processor(&image)?;
    h.start_processor(entry)?;
    let mut model = Processor::default();
    h.run_processor(&image, &mut model, 200_000)?;
    ensure!(
        model.rf[8] == 0 && h.stats.collections == 5 && h.rtl.dbg_next_identity_o == 8,
        "autonomous collection result"
    );
    Ok("7 allocations, 5 machine collections and retries; no host recovery commands".into())
}
