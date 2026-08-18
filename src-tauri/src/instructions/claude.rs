use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::domain::{GlobalInstructionFile, GlobalInstructionSourceRow, InstructionLoadStatus};

pub fn scan(home: &Path) -> GlobalInstructionSourceRow {
    let claude_dir = home.join(".claude");
    let mut files = vec![read_file(
        &claude_dir.join("CLAUDE.md"),
        "~/.claude/CLAUDE.md",
    )];

    let user_dir = claude_dir.join("rules");
    if let Ok(entries) = fs::read_dir(&user_dir) {
        let mut extras = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            extras.push((name.clone(), path, format!("~/.claude/rules/{name}")));
        }
        extras.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, path, display) in extras {
            files.push(read_file(&path, &display));
        }
    }

    GlobalInstructionSourceRow {
        source: "claude".into(),
        application: "Claude".into(),
        files,
    }
}

fn read_file(path: &Path, display_path: &str) -> GlobalInstructionFile {
    match fs::metadata(path) {
        Ok(meta) => from_meta(path, display_path, &meta),
        Err(_) => GlobalInstructionFile {
            display_path: display_path.to_string(),
            abs_path: path.to_string_lossy().into_owned(),
            byte_size: 0,
            modified_at: None,
            load_status: InstructionLoadStatus::NotCreated,
            content: String::new(),
            error: None,
        },
    }
}

fn from_meta(path: &Path, display_path: &str, meta: &fs::Metadata) -> GlobalInstructionFile {
    let content = fs::read_to_string(path).ok();
    let error = if content.is_none() {
        Some(format!("读取 {display_path} 失败"))
    } else {
        None
    };
    GlobalInstructionFile {
        display_path: display_path.to_string(),
        abs_path: path.to_string_lossy().into_owned(),
        byte_size: meta.len(),
        modified_at: mtime_rfc3339(meta),
        load_status: InstructionLoadStatus::Loaded,
        content: content.unwrap_or_default(),
        error,
    }
}

fn mtime_rfc3339(meta: &fs::Metadata) -> Option<String> {
    let modified = meta.modified().ok()?;
    Some(DateTime::<Utc>::from(modified).to_rfc3339())
}
