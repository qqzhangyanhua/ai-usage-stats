use crate::domain::{GlobalInstructionSourceRow, Source};

use super::file;

/// kimi-cli 只从项目根到 cwd 分层加载 `AGENTS.md` / `.kimi/AGENTS.md`，
/// 没有用户级全局指令文件。`~/.kimi/AGENTS.md` 仍是未落地的功能请求。
/// 依据：MoonshotAI/kimi-cli `load_agents_md` 与 issue #2152 / #439（2026-08 查阅）。
/// 注意：kimi-code 的 `~/.kimi-code/AGENTS.md` 是另一套产品，本 Source 扫的是 `~/.kimi`。
pub fn scan() -> GlobalInstructionSourceRow {
    GlobalInstructionSourceRow {
        source: Source::Kimi.as_str().into(),
        application: Source::Kimi.application_name().into(),
        files: vec![file::no_mechanism(
            "kimi-cli 只加载项目树内的 AGENTS.md，官方尚未支持 ~/.kimi/AGENTS.md。",
        )],
    }
}
