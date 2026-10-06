//! Assemble the standalone collector for a concrete hardware configuration.
use crate::{processor::Instruction, text, Status};
pub const SOURCE: &str = include_str!("../../../microcode/ram-collector.uc");
pub fn program(
    origin: u16,
    pager_entries: usize,
    root_count: usize,
) -> Result<Vec<Instruction>, Status> {
    if !pager_entries.is_power_of_two()
        || !(2..=65536).contains(&pager_entries)
        || root_count == 0
        || root_count > 65535
    {
        return Err(Status::BadCommand);
    }
    let program = text::assemble(
        SOURCE,
        origin,
        &[
            ("PAGER_ENTRIES", pager_entries as i64),
            ("ROOT_COUNT", root_count as i64),
        ],
    )
    .map_err(|_| Status::BadCommand)?;
    Ok(program.code.into_values().collect())
}
