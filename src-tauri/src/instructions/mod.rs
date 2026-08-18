pub mod claude;

use std::path::Path;

use crate::domain::{GlobalInstructionDto, InstructionUsageSummary};

pub fn scan(
    home: &Path,
    _project_root: Option<&Path>,
    _usage: &InstructionUsageSummary,
) -> GlobalInstructionDto {
    GlobalInstructionDto {
        sources: vec![claude::scan(home)],
    }
}
