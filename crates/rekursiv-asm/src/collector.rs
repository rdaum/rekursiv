//! Assemble the standalone collector for a concrete hardware configuration.
use crate::{processor::Instruction, text, Status};
pub const SOURCE: &str = include_str!("../../../microcode/ram-collector.uc");
pub fn program(origin: u16, root_count: usize) -> Result<Vec<Instruction>, Status> {
    if root_count == 0 || root_count > 65535 {
        return Err(Status::BadCommand);
    }
    let program = text::assemble(SOURCE, origin, &[("ROOT_COUNT", root_count as i64)])
        .map_err(|_| Status::BadCommand)?;
    Ok(program.code.into_values().collect())
}
