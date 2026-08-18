use std::path::Path;

use crate::domain::{GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus};

use super::file;

pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    GlobalInstructionSourceRow {
        source: "gemini".into(),
        application: "Gemini".into(),
        files: vec![file::read_file(
            &home.join(".gemini/GEMINI.md"),
            "~/.gemini/GEMINI.md",
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            None,
        )],
    }
}
