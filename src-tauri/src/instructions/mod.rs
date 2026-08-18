pub mod claude;
pub mod codex;
pub mod cursor;
mod file;
pub mod gemini;

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::domain::{GlobalInstructionDto, InstructionUsageSummary};

pub fn scan(
    home: &Path,
    _project_root: Option<&Path>,
    _usage: &InstructionUsageSummary,
) -> GlobalInstructionDto {
    GlobalInstructionDto {
        sources: vec![
            claude::scan(home),
            codex::scan(home),
            gemini::scan(home),
            cursor::scan(),
        ],
    }
}

/// 解析「在外部打开」的目标：已存在的文件或目录原样打开；文件尚未创建则打开父目录。
pub fn resolve_open_path(abs_path: &str) -> Result<PathBuf, String> {
    if abs_path.trim().is_empty() {
        return Err("没有可打开的路径".into());
    }
    let path = PathBuf::from(abs_path);
    if path.exists() {
        return Ok(path);
    }
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => Ok(parent.to_path_buf()),
        _ => Err("没有可打开的路径".into()),
    }
}

pub fn open_in_external_editor(abs_path: &str) -> Result<(), String> {
    let target = resolve_open_path(abs_path)?;
    let status = open_command(&target)
        .status()
        .map_err(|e| format!("无法在外部打开：{e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("无法在外部打开该全局指令".into())
    }
}

fn open_command(target: &Path) -> Command {
    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        cmd.arg(target);
        cmd
    }
    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(target);
        cmd
    }
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", &target.display().to_string()]);
        cmd
    }
}
