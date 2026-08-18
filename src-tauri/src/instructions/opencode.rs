use std::path::Path;

use crate::domain::{
    GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus, Source,
};

use super::file;

/// 官方 Rules：用户级文件为 `~/.config/opencode/AGENTS.md`，跨会话加载。
/// 未创建时官方会回退 `~/.claude/CLAUDE.md`（已在 Claude 下列出，此处不重复展示）。
/// 依据：https://opencode.ai/docs/rules/ （2026-08 查阅）。
pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    let path = home.join(".config/opencode/AGENTS.md");
    let note = if path.is_file() {
        None
    } else {
        Some("未创建时官方会回退 ~/.claude/CLAUDE.md。".into())
    };
    GlobalInstructionSourceRow {
        source: Source::Opencode.as_str().into(),
        application: Source::Opencode.application_name().into(),
        files: vec![file::read_file(
            &path,
            "~/.config/opencode/AGENTS.md",
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            note,
        )],
    }
}
