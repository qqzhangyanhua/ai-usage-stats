use std::path::Path;

use crate::domain::{GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus};

use super::file;

pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    let dir = home.join(".codex");
    let override_path = dir.join("AGENTS.override.md");
    let base_path = dir.join("AGENTS.md");
    let override_exists = override_path.is_file();

    let mut files = Vec::new();
    if override_exists {
        files.push(file::read_file(
            &override_path,
            "~/.codex/AGENTS.override.md",
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            None,
        ));
    }

    let (base_status, base_note) = if override_exists && base_path.is_file() {
        (
            InstructionLoadStatus::PresentUnloaded,
            Some("被 ~/.codex/AGENTS.override.md 屏蔽，原生 Codex 只读 override。".into()),
        )
    } else {
        (InstructionLoadStatus::Loaded, None)
    };
    files.push(file::read_file(
        &base_path,
        "~/.codex/AGENTS.md",
        base_status,
        InstructionEvidence::Verified,
        base_note,
    ));

    for (name, path) in file::list_files(&dir.join("rules")) {
        files.push(file::read_file(
            &path,
            &format!("~/.codex/rules/{name}"),
            InstructionLoadStatus::PresentUnloaded,
            InstructionEvidence::Verified,
            Some("原生 Codex 不读取此目录，这是第三方留下的文件。".into()),
        ));
    }

    GlobalInstructionSourceRow {
        source: "codex".into(),
        application: "Codex".into(),
        files,
    }
}
