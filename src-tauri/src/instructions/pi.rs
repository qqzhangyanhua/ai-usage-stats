use std::path::Path;

use crate::domain::{
    GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus, Source,
};

use super::file;

/// 官方 Context Files：全局文件为 `~/.pi/agent/AGENTS.md`；同目录存在
/// `AGENTS.override.md` 时只读 override。
/// 依据：pi `packages/coding-agent/README.md`（2026-08 查阅）。
pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    let dir = home.join(".pi/agent");
    let override_path = dir.join("AGENTS.override.md");
    let base_path = dir.join("AGENTS.md");
    let override_exists = override_path.is_file();

    let mut files = Vec::new();
    if override_exists {
        files.push(file::read_file(
            &override_path,
            "~/.pi/agent/AGENTS.override.md",
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            None,
        ));
    }

    let (base_status, base_note) = if override_exists && base_path.is_file() {
        (
            InstructionLoadStatus::PresentUnloaded,
            Some("被 ~/.pi/agent/AGENTS.override.md 屏蔽。".into()),
        )
    } else {
        (InstructionLoadStatus::Loaded, None)
    };
    files.push(file::read_file(
        &base_path,
        "~/.pi/agent/AGENTS.md",
        base_status,
        InstructionEvidence::Verified,
        base_note,
    ));

    GlobalInstructionSourceRow {
        source: Source::Pi.as_str().into(),
        application: Source::Pi.application_name().into(),
        files,
    }
}
