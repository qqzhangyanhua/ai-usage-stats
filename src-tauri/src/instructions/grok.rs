use std::path::Path;

use crate::domain::{
    GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus, Source,
};

use super::file;

const CANDIDATES: &[&str] = &[
    "AGENTS.md",
    "Agents.md",
    "AGENT.md",
    "CLAUDE.md",
    "Claude.md",
    "CLAUDE.local.md",
];

/// 官方 Project Rules：先读 `~/.grok/` 下的全局指令文件，候选名为
/// AGENTS.md / Agents.md / AGENT.md / CLAUDE.md / Claude.md / CLAUDE.local.md。
/// 不把 config.toml、sessions 等非指令文件列进来。
/// 依据：https://docs.x.ai/build/features/project-rules （2026-08 查阅）。
pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    let dir = home.join(".grok");
    let mut files: Vec<_> = CANDIDATES
        .iter()
        .filter(|name| dir.join(name).is_file())
        .map(|name| {
            file::read_file(
                &dir.join(name),
                &format!("~/.grok/{name}"),
                InstructionLoadStatus::Loaded,
                InstructionEvidence::Verified,
                None,
            )
        })
        .collect();

    if files.is_empty() {
        files.push(file::read_file(
            &dir.join("AGENTS.md"),
            "~/.grok/AGENTS.md",
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            None,
        ));
    }

    GlobalInstructionSourceRow {
        source: Source::Grok.as_str().into(),
        application: Source::Grok.application_name().into(),
        files,
    }
}
