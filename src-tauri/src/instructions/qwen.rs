use std::path::Path;

use crate::domain::{
    GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus, Source,
};

use super::file;

/// 官方 Memory：用户级文件为 `~/.qwen/QWEN.md`，跨项目加载。
/// 虽由 Gemini CLI fork，当前文档已标准化为 QWEN.md，不再把 GEMINI.md 标成已验证。
/// 依据：https://qwenlm.github.io/qwen-code-docs/en/users/features/memory/ （2026-08 查阅）。
pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    GlobalInstructionSourceRow {
        source: Source::Qwen.as_str().into(),
        application: Source::Qwen.application_name().into(),
        files: vec![file::read_file(
            &home.join(".qwen/QWEN.md"),
            "~/.qwen/QWEN.md",
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            None,
        )],
    }
}
