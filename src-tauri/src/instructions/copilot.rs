use std::path::Path;

use crate::domain::{
    GlobalInstructionSourceRow, InstructionEvidence, InstructionLoadStatus, Source,
};

use super::file;

/// 官方 Copilot CLI custom instructions：用户级主文件
/// `$HOME/.copilot/copilot-instructions.md`，模块文件在
/// `$HOME/.copilot/instructions/**/*.instructions.md`。
/// 依据：https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-custom-instructions （2026-08 查阅）。
pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    let root = home.join(".copilot");
    let mut files = vec![file::read_file(
        &root.join("copilot-instructions.md"),
        "~/.copilot/copilot-instructions.md",
        InstructionLoadStatus::Loaded,
        InstructionEvidence::Verified,
        None,
    )];

    for (name, path) in file::list_files(&root.join("instructions")) {
        if !name.ends_with(".instructions.md") {
            continue;
        }
        files.push(file::read_file(
            &path,
            &format!("~/.copilot/instructions/{name}"),
            InstructionLoadStatus::Loaded,
            InstructionEvidence::Verified,
            None,
        ));
    }

    GlobalInstructionSourceRow {
        source: Source::Copilot.as_str().into(),
        application: Source::Copilot.application_name().into(),
        files,
    }
}
