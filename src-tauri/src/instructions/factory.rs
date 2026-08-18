use std::path::Path;

use crate::domain::{
    GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus, Source,
};

use super::file;

/// 官方 AGENTS.md：个人目录 `~/.factory/` 存放跨项目偏好，推荐文件名 AGENTS.md。
/// `~/.agents/`、`~/.agent/` 是兼容目录，不属于本 Source 的主约定，不在此列出。
/// 依据：https://docs.factory.ai/cli/configuration/agents-md （2026-08 查阅）。
pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    GlobalInstructionSourceRow {
        source: Source::Factory.as_str().into(),
        application: Source::Factory.application_name().into(),
        files: vec![file::read_file(
            &home.join(".factory/AGENTS.md"),
            "~/.factory/AGENTS.md",
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            None,
        )],
    }
}
