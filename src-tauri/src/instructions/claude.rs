use std::path::Path;

use crate::domain::{GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus};

use super::file;

pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    let claude_dir = home.join(".claude");
    let mut files = vec![file::read_file(
        &claude_dir.join("CLAUDE.md"),
        "~/.claude/CLAUDE.md",
        InstructionLoadStatus::Loaded,
        InstructionEvidence::Verified,
        None,
    )];

    if let Some(dir) = file::read_directory(
        &claude_dir.join("rules"),
        "~/.claude/rules/",
        InstructionLoadStatus::Loaded,
        InstructionEvidence::Verified,
        None,
    ) {
        files.push(dir);
    }

    for (name, path) in file::list_files(&claude_dir.join("rules")) {
        files.push(file::read_file(
            &path,
            &format!("~/.claude/rules/{name}"),
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            None,
        ));
    }

    GlobalInstructionSourceRow {
        source: "claude".into(),
        application: "Claude".into(),
        files,
    }
}
