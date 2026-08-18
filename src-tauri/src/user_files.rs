use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};

use crate::domain::WriteUserFileResult;

const BACKUP_KEEP: usize = 5;
const BACKUP_DIR: &str = "instruction-backups";

pub fn write(
    home: &Path,
    data_dir: &Path,
    path: &Path,
    content: &str,
    expected_mtime: Option<&str>,
) -> Result<WriteUserFileResult, String> {
    if !is_allowed(home, path) {
        return Err("该路径不在可写名单中".into());
    }

    let exists = path.is_file();
    let current_mtime = exists
        .then(|| {
            fs::metadata(path)
                .ok()
                .and_then(|meta| mtime_rfc3339(&meta))
        })
        .flatten();
    if current_mtime.as_deref() != expected_mtime {
        return Err("该文件在外部被修改过".into());
    }

    if exists {
        backup_original(data_dir, home, path)?;
    }

    atomic_write(path, content)?;

    let meta = fs::metadata(path).map_err(|e| format!("写入后读取失败：{e}"))?;
    Ok(WriteUserFileResult {
        modified_at: mtime_rfc3339(&meta),
        byte_size: meta.len(),
    })
}

pub fn is_allowed(home: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(home) else {
        return false;
    };
    let parts: Vec<_> = rel.iter().filter_map(|p| p.to_str()).collect();
    match parts.as_slice() {
        [".claude", "CLAUDE.md"] => true,
        [".claude", "rules", name] if is_plain_name(name) => true,
        [".codex", "AGENTS.md"] => true,
        [".codex", "AGENTS.override.md"] => true,
        [".gemini", "GEMINI.md"] => true,
        _ => false,
    }
}

fn is_plain_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

fn backup_original(data_dir: &Path, home: &Path, path: &Path) -> Result<(), String> {
    let original = fs::read(path).map_err(|e| format!("备份前读取失败：{e}"))?;
    let dir = backup_dir_for(data_dir, home, path);
    fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败：{e}"))?;
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    let dest = dir.join(format!("{stamp}.bak"));
    fs::write(&dest, original).map_err(|e| format!("写入备份失败：{e}"))?;
    prune_backups(&dir)?;
    Ok(())
}

fn backup_dir_for(data_dir: &Path, home: &Path, path: &Path) -> PathBuf {
    let rel = path
        .strip_prefix(home)
        .map(|p| p.to_string_lossy().replace(['/', '\\'], "__"))
        .unwrap_or_else(|_| "unknown".into());
    data_dir.join(BACKUP_DIR).join(rel)
}

fn prune_backups(dir: &Path) -> Result<(), String> {
    let mut files = fs::read_dir(dir)
        .map_err(|e| format!("读取备份目录失败：{e}"))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    files.sort();
    if files.len() <= BACKUP_KEEP {
        return Ok(());
    }
    for stale in files.iter().take(files.len() - BACKUP_KEEP) {
        let _ = fs::remove_file(stale);
    }
    Ok(())
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = path.with_file_name(format!(
        ".{}.tmp-{nonce}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file")
    ));
    let write_result = fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败：{e}"));
    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
        return write_result;
    }
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&tmp);
            Err(format!("替换目标文件失败：{error}"))
        }
    }
}

fn mtime_rfc3339(meta: &fs::Metadata) -> Option<String> {
    let modified = meta.modified().ok()?;
    Some(DateTime::<Utc>::from(modified).to_rfc3339())
}
