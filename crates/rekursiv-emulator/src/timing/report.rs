use super::Estimate;
use crate::Statistics;
use std::fmt::Write;

impl Estimate {
    /// Format a hardware estimate at `clock_mhz`, separately from native MIPS.
    /// The frequency changes time conversion only, not channel cycle assumptions.
    /// Returns an error for a non-finite or non-positive clock frequency.
    pub fn report(&self, stats: &Statistics, clock_mhz: f64) -> eyre::Result<String> {
        eyre::ensure!(
            clock_mhz.is_finite() && clock_mhz > 0.0,
            "clock MHz must be finite and positive"
        );
        let mut out = String::new();
        let c = self.cycles;
        let w = self.work;
        let total = c.total();
        let retired = stats.retired + stats.collector_retired;
        writeln!(
            out,
            "Hardware cycle estimate (passive model; excludes bootstrap):"
        )
        .unwrap();
        for (name, channel) in [
            ("RAM", self.config.ram),
            ("backing", self.config.backing),
            ("device", self.config.device),
        ] {
            writeln!(
                out,
                "  {name}: {} request-wait + 1 acceptance + {} response cycles/request",
                channel.request_wait, channel.response_cycles
            )
            .unwrap();
        }
        writeln!(
            out,
            "  elapsed: {total} cycles; {:.3} cycles/retired instruction",
            if retired == 0 {
                0.0
            } else {
                total as f64 / retired as f64
            }
        )
        .unwrap();
        writeln!(
            out,
            "  at {clock_mhz:.3} MHz: {:.6} s, {:.3} M microinstructions/s ({:.3} M mutator/s)",
            total as f64 / (clock_mhz * 1e6),
            if total == 0 {
                0.0
            } else {
                clock_mhz * retired as f64 / total as f64
            },
            if total == 0 {
                0.0
            } else {
                clock_mhz * stats.retired as f64 / total as f64
            }
        )
        .unwrap();
        writeln!(out, "  additive cycles: execute {}, object issue {}, blocking object wait {}, async join/wait {}, numeric wait {}, device wait {}, GC maintenance wait {}", c.execute, c.object_issue, c.object_wait, c.async_wait, c.numeric_wait, c.device_wait, c.maintenance_wait).unwrap();
        writeln!(
            out,
            "  collector: {} cycles ({:.2}% of elapsed, already included)",
            w.collector_cycles,
            if total == 0 {
                0.0
            } else {
                100.0 * w.collector_cycles as f64 / total as f64
            }
        )
        .unwrap();
        writeln!(out, "  service work (overlaps execution): RAM {} requests / {} cycles; backing {} requests / {} cycles", w.ram_requests, w.ram_cycles, w.backing_requests, w.backing_cycles).unwrap();
        writeln!(out, "  async: {} service cycles; {} hidden cycles across {} joins; pending={}, {} remaining service cycles", w.async_service, w.async_hidden, w.async_joins, self.pending(), self.pending_cycles()).unwrap();
        writeln!(out, "  approximations: {} divide/sqrt operations (32/60 cycles for binary32/64); {} uncalibrated object paths", w.approximate_floats, w.approximate_objects).unwrap();
        writeln!(out, "  No DRAM row/bank model, refresh, shared-bus contention, disk latency, or display DMA. Guest clocks and input are unchanged.").unwrap();
        Ok(out)
    }
}
