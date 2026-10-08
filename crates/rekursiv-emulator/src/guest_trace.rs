//! Optional observation of language microcode boundaries. Never supplies results.
use eyre::{ensure, Result};
use rekursiv_emulator::Machine;
use std::{
    collections::BTreeMap,
    io::{BufWriter, Write},
    path::Path,
};

pub struct GuestTrace {
    output: BufWriter<std::fs::File>,
    decoded: u16,
    primitive: u16,
}
impl GuestTrace {
    pub fn create(path: &Path, symbols: &BTreeMap<String, i64>) -> Result<Self> {
        ensure!(
            symbols.contains_key("decoded") && symbols.contains_key("primitive_dispatch"),
            "--guest-trace requires a language interpreter profile"
        );
        Ok(Self {
            output: BufWriter::new(std::fs::File::create(path)?),
            decoded: symbols["decoded"] as u16,
            primitive: symbols["primitive_dispatch"] as u16,
        })
    }
    pub fn observe(&mut self, machine: &Machine) -> Result<()> {
        if machine.recovering() {
            return Ok(());
        }
        let cpu = &machine.cpu;
        let vr = &machine.objekt.state.vr;
        if cpu.pc == self.decoded {
            writeln!(
                self.output,
                "bytecode={} context={:010x} method={:010x} ip={} sp={}",
                cpu.rf[0],
                vr[0].bits(),
                vr[1].bits(),
                cpu.rf[8].saturating_sub(1),
                cpu.rf[9]
            )?;
        } else if cpu.pc == self.primitive {
            writeln!(
                self.output,
                "primitive={} method={:010x} receiver={:010x} arguments={}",
                cpu.rf[0],
                vr[7].bits(),
                vr[6].bits(),
                cpu.rf[1]
            )?;
        }
        Ok(())
    }
    pub fn flush(&mut self) -> Result<()> {
        Ok(self.output.flush()?)
    }
}
