use std::path::Path;

use crate::domain::{
    GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus, Source,
};

use super::file;

/// 官方 agent-instructions：用户级固定为 `$DSH_HOME/AGENTS.md`，默认 `~/.dsh/AGENTS.md`，
/// 不受项目候选文件名列表影响，也没有 local overlay。
/// 依据：deepseek-harness `packages/context/agent-instructions/README.md`（2026-08 查阅）。
pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    GlobalInstructionSourceRow {
        source: Source::Dsh.as_str().into(),
        application: Source::Dsh.application_name().into(),
        files: vec![file::read_file(
            &home.join(".dsh/AGENTS.md"),
            "~/.dsh/AGENTS.md",
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            None,
        )],
    }
}
