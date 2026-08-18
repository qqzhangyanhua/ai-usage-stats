pub mod claude;
pub mod codex;
pub mod cursor;
mod file;
pub mod gemini;

use std::path::Path;

use crate::domain::{GlobalInstructionDto, InstructionUsageSummary};

pub fn scan(
    home: &Path,
    _project_root: Option<&Path>,
    _usage: &InstructionUsageSummary,
) -> GlobalInstructionDto {
    GlobalInstructionDto {
        sources: vec![
            claude::scan(home),
            codex::scan(home),
            gemini::scan(home),
            cursor::scan(),
        ],
    }
}
