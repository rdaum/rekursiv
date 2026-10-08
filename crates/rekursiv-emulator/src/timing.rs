//! Passive estimates of RTL execution cycles, independent of host execution speed.
//!
//! This is an event cost model, not a second hardware simulator. Local execution
//! advances a virtual clock; an asynchronous OBJEKT reply has a completion time
//! on that clock. Only its unhidden tail delays the next barrier. The estimator
//! never advances guest devices or changes instruction ordering.
#![deny(missing_docs)]

mod object;
mod report;

pub(crate) use object::ObjectPath;
use rekursiv_asm::processor::{Alu, FloatOp, Instruction, Precision, Recovery};

/// Timing of one external request, in processor clocks.
///
/// Request acceptance and response consumption always use separate edges.
/// Channels currently permit one outstanding request, with no burst overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel {
    /// Clocks spent waiting for request acceptance, before the acceptance edge.
    pub request_wait: u32,
    /// Clocks from acceptance to response consumption. Must be at least one.
    pub response_cycles: u32,
}
impl Channel {
    pub(crate) fn cycles(self) -> u64 {
        1 + u64::from(self.request_wait) + u64::from(self.response_cycles)
    }
}

/// Explicit latency assumptions. Profiles are examples, not board specifications.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Resident object RAM timing, per 40-bit word request.
    pub ram: Channel,
    /// Backing adapter timing, per request including metadata and save commits.
    pub backing: Channel,
    /// Peripheral register timing, per request. Disk completion is not modeled.
    pub device: Channel,
}
impl Config {
    /// Example SRAM system: one-cycle RAM response, eight-cycle backing response.
    pub const SRAM: Self = Self {
        ram: Channel {
            request_wait: 0,
            response_cycles: 1,
        },
        backing: Channel {
            request_wait: 0,
            response_cycles: 8,
        },
        device: Channel {
            request_wait: 0,
            response_cycles: 2,
        },
    };
    /// Example DRAM system: twelve-cycle RAM response, forty-cycle backing response.
    /// This applies a fixed latency per word; it does not assume burst support.
    pub const DRAM: Self = Self {
        ram: Channel {
            request_wait: 0,
            response_cycles: 12,
        },
        backing: Channel {
            request_wait: 0,
            response_cycles: 40,
        },
        device: Channel {
            request_wait: 0,
            response_cycles: 2,
        },
    };
    pub(crate) fn validate(self) -> eyre::Result<()> {
        eyre::ensure!(
            [self.ram, self.backing, self.device]
                .iter()
                .all(|c| c.response_cycles > 0),
            "timing responses require at least one cycle"
        );
        Ok(())
    }
}

/// Additive cycle categories. Their sum is the estimated elapsed processor time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cycles {
    /// One EXECUTE cycle per instruction attempt, including HOLD and GC transitions.
    pub execute: u64,
    /// OBJEKT command acceptance cycles, for blocking and asynchronous commands.
    pub object_issue: u64,
    /// Blocking OBJEKT service and reply cycles, excluding issue.
    pub object_wait: u64,
    /// Exposed asynchronous wait, including the mandatory barrier join edge.
    pub async_wait: u64,
    /// NUMERIK floating-point wait cycles beyond the EXECUTE cycle.
    pub numeric_wait: u64,
    /// Device handshake and completion cycles beyond the EXECUTE cycle.
    pub device_wait: u64,
    /// Collector maintenance issue and response cycles beyond EXECUTE.
    pub maintenance_wait: u64,
}
impl Cycles {
    /// Sum of the mutually exclusive categories.
    pub fn total(self) -> u64 {
        self.execute
            + self.object_issue
            + self.object_wait
            + self.async_wait
            + self.numeric_wait
            + self.device_wait
            + self.maintenance_wait
    }
}

/// Work counts and overlap observations. These are not additive elapsed cycles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Work {
    /// Resident RAM requests, including collector reads and writes.
    pub ram_requests: u64,
    /// Cycles occupied by RAM transactions, including acceptance and response.
    pub ram_cycles: u64,
    /// Backing adapter requests, including metadata and save commits.
    pub backing_requests: u64,
    /// Cycles occupied by backing transactions.
    pub backing_cycles: u64,
    /// Service cycles of all asynchronous commands, excluding issue.
    pub async_service: u64,
    /// Asynchronous service hidden by other instructions, for joined commands.
    pub async_hidden: u64,
    /// Number of commands joined at a dependency or external boundary.
    pub async_joins: u64,
    /// Collector cycles: a subset of the elapsed total, not an additional cost.
    pub collector_cycles: u64,
    /// Divide/square-root operations charged a conservative fixed latency.
    pub approximate_floats: u64,
    /// Error paths whose control cost is not calibrated (excludes allocation pressure).
    pub approximate_objects: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pending {
    start: u64,
    service: u64,
}

/// Running estimate for one execution, excluding bootstrap and host compilation.
/// Obtain it through [`crate::Machine::cycle_estimate`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Estimate {
    config: Config,
    cycles: Cycles,
    work: Work,
    pending: Option<Pending>,
}
impl Estimate {
    pub(crate) fn new(config: Config) -> Self {
        Self {
            config,
            cycles: Cycles::default(),
            work: Work::default(),
            pending: None,
        }
    }
    /// The latency assumptions used for this entire sample.
    pub fn config(&self) -> Config {
        self.config
    }
    /// Additive elapsed-cycle breakdown.
    pub fn cycles(&self) -> Cycles {
        self.cycles
    }
    /// Service work and overlap counters; do not add these to elapsed cycles.
    pub fn work(&self) -> Work {
        self.work
    }
    /// Remaining service cycles for an unjoined command at the sample boundary.
    /// Reporting does not drain the command or charge a future join.
    pub fn pending_cycles(&self) -> u64 {
        self.pending.map_or(0, |p| {
            (p.start + p.service).saturating_sub(self.cycles.total())
        })
    }
    /// Whether a command still needs a join, even if its reply is already ready.
    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn local(&mut self, count: u64, collector: bool) {
        self.cycles.execute += count;
        if collector {
            self.work.collector_cycles += count;
        }
    }
    pub(crate) fn instruction(&mut self, i: Instruction, collector: bool) {
        if i.alu == Alu::Float {
            let extra = if matches!(i.float, FloatOp::Divide | FloatOp::Sqrt) {
                // HardFloat's iterative path depends on operands (including
                // exceptional early exits). Use a conservative normal-case
                // allowance, and count every approximation in the report.
                self.work.approximate_floats += 1;
                if i.precision == Precision::Binary64 {
                    59
                } else {
                    31
                }
            } else {
                2
            };
            self.cycles.numeric_wait += extra;
        }
        if i.device != rekursiv_asm::processor::Device::None {
            // logik_io REQUEST/RESPONSE, followed by COMPLETE retirement.
            self.cycles.device_wait += self.config.device.cycles() + 1;
        }
        if collector && !matches!(i.recovery, Recovery::None | Recovery::Return) {
            let ram = matches!(i.recovery, Recovery::ReadBody | Recovery::WriteBody);
            let extra = 2 + if ram { self.config.ram.cycles() } else { 0 };
            self.cycles.maintenance_wait += extra;
            self.work.collector_cycles += extra;
            if ram {
                self.work.ram_requests += 1;
                self.work.ram_cycles += self.config.ram.cycles();
            }
        }
    }
    pub(crate) fn join(&mut self) {
        if let Some(p) = self.pending.take() {
            let hidden = (self.cycles.total() - p.start).min(p.service);
            self.work.async_hidden += hidden;
            self.work.async_joins += 1;
            // LOGIK joins on the reply edge when still waiting, or on a
            // separate EXECUTE edge if the reply arrived during local work.
            self.cycles.async_wait += (p.service - hidden).max(1);
        }
    }
}
