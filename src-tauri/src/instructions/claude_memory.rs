use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::adapters::project::decode_dashed_dir;
use crate::domain::{
    ClaudeAutoMemoryFile, ClaudeAutoMemoryRepo, InstructionCheckupFinding, InstructionCheckupKind,
    InstructionCheckupSeverity,
};

pub fn collect(home: &Path) -> Vec<ClaudeAutoMemoryRepo> {
    let mut by_repo: BTreeMap<String, ClaudeAutoMemoryRepo> = BTreeMap::new();
    for root in project_roots(home) {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let Some(repo) = read_repo(home, &entry.path()) else {
                continue;
            };
            match by_repo.get(&repo.repo) {
                Some(existing) if existing.modified_at >= repo.modified_at => {}
                _ => {
                    by_repo.insert(repo.repo.clone(), repo);
                }
            }
        }
    }
    let mut repos: Vec<ClaudeAutoMemoryRepo> = by_repo.into_values().collect();
    repos.sort_by(|a, b| {
        b.modified_at
            .cmp(&a.modified_at)
            .then_with(|| a.repo.cmp(&b.repo))
    });
    repos
}

pub fn finding(repos: &[ClaudeAutoMemoryRepo]) -> Option<InstructionCheckupFinding> {
    if repos.is_empty() {
        return None;
    }
    Some(InstructionCheckupFinding {
        kind: InstructionCheckupKind::AutoMemory,
        severity: InstructionCheckupSeverity::Medium,
        source: "claude".into(),
        application: "Claude".into(),
        display_path: "自动记忆".into(),
        problem: format!("检测到 Claude 在 {} 个仓库写了自动记忆。", repos.len()),
        consequence: "每次会话开始会把 MEMORY.md 开头注入上下文，这些笔记正在影响 Claude 的行为。"
            .into(),
    })
}

fn project_roots(home: &Path) -> [PathBuf; 2] {
    [
        home.join(".claude/projects"),
        home.join(".config/claude/projects"),
    ]
}

fn read_repo(home: &Path, project_dir: &Path) -> Option<ClaudeAutoMemoryRepo> {
    let memory_dir = project_dir.join("memory");
    if !memory_dir.is_dir() {
        return None;
    }
    let slug = project_dir.file_name()?.to_str()?;
    let mut files: Vec<ClaudeAutoMemoryFile> = fs::read_dir(&memory_dir)
        .ok()?
        .flatten()
        .filter_map(|entry| read_file(&entry.path()))
        .collect();
    if files.is_empty() {
        return None;
    }
    files.sort_by(|a, b| memory_name_rank(&a.name).cmp(&memory_name_rank(&b.name)));
    let byte_size = files.iter().map(|file| file.byte_size).sum();
    let modified_at = files
        .iter()
        .filter_map(|file| file.modified_at.as_ref())
        .max()
        .cloned();
    Some(ClaudeAutoMemoryRepo {
        repo: decode_dashed_dir(slug),
        display_path: display_path(home, &memory_dir, slug),
        abs_path: memory_dir.to_string_lossy().into_owned(),
        byte_size,
        modified_at,
        files,
    })
}

fn read_file(path: &Path) -> Option<ClaudeAutoMemoryFile> {
    if !path.is_file() {
        return None;
    }
    let name = path.file_name()?.to_str()?.to_string();
    if name.starts_with('.') {
        return None;
    }
    let meta = fs::metadata(path).ok()?;
    if meta.len() == 0 {
        return None;
    }
    let content = fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    Some(ClaudeAutoMemoryFile {
        name,
        abs_path: path.to_string_lossy().into_owned(),
        byte_size: meta.len(),
        modified_at: mtime_rfc3339(&meta),
        content,
    })
}

fn display_path(home: &Path, memory_dir: &Path, slug: &str) -> String {
    if memory_dir.starts_with(home.join(".config/claude/projects")) {
        format!("~/.config/claude/projects/{slug}/memory/")
    } else {
        format!("~/.claude/projects/{slug}/memory/")
    }
}

fn memory_name_rank(name: &str) -> (u8, &str) {
    if name == "MEMORY.md" {
        (0, name)
    } else {
        (1, name)
    }
}

fn mtime_rfc3339(meta: &fs::Metadata) -> Option<String> {
    let modified = meta.modified().ok()?;
    Some(DateTime::<Utc>::from(modified).to_rfc3339())
}
