use std::collections::BTreeMap;
use std::path::Path;

use crate::adapters::project;
use crate::domain::CursorSessionRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCursorSession {
    pub turn_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub aborted_count: i64,
    pub tool_calls: BTreeMap<String, i64>,
}

/// 从 agent-transcripts jsonl 解析单会话聚合；不读取 user/assistant 正文。
pub fn parse_cursor_session_transcript(content: &str) -> Result<ParsedCursorSession, String> {
    let mut values = Vec::new();
    let mut parse_errors = 0usize;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(value) => values.push(value),
            Err(_) => parse_errors += 1,
        }
    }
    if parse_errors > 0 {
        return Err(format!(
            "Cursor 会话 transcript JSON 解析失败：{parse_errors} 行无效"
        ));
    }

    let mut parsed = ParsedCursorSession {
        turn_count: 0,
        success_count: 0,
        error_count: 0,
        aborted_count: 0,
        tool_calls: BTreeMap::new(),
    };

    for value in &values {
        if value.get("type").and_then(|v| v.as_str()) == Some("turn_ended") {
            parsed.turn_count += 1;
            match value.get("status").and_then(|v| v.as_str()) {
                Some("success") => parsed.success_count += 1,
                Some("error") => parsed.error_count += 1,
                Some("aborted") => parsed.aborted_count += 1,
                _ => {}
            }
            continue;
        }
        if value.get("role").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let Some(blocks) = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(|content| content.as_array())
        else {
            continue;
        };
        for block in blocks {
            if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                continue;
            }
            let name = block
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            *parsed.tool_calls.entry(name).or_insert(0) += 1;
        }
    }

    Ok(parsed)
}

pub fn project_from_transcript_path(path: &Path) -> String {
    let mut saw_projects = false;
    for component in path.components() {
        let part = component.as_os_str().to_string_lossy();
        if saw_projects {
            return project::decode_dashed_dir(&part);
        }
        if part == "projects" {
            saw_projects = true;
        }
    }
    String::new()
}

pub fn build_cursor_session_record(
    source_file: &str,
    parsed: &ParsedCursorSession,
    seen_at: Option<String>,
) -> Result<CursorSessionRecord, String> {
    let tool_calls_json =
        serde_json::to_string(&parsed.tool_calls).map_err(|e| e.to_string())?;
    Ok(CursorSessionRecord {
        session_id: project::session_id_from_source_file(source_file),
        project: project_from_transcript_path(Path::new(source_file)),
        turn_count: parsed.turn_count,
        success_count: parsed.success_count,
        error_count: parsed.error_count,
        aborted_count: parsed.aborted_count,
        tool_calls_json,
        models_json: "[]".to_string(),
        first_seen_at: seen_at.clone(),
        last_seen_at: seen_at,
        files_touched: 0,
        source_file: source_file.to_string(),
    })
}
