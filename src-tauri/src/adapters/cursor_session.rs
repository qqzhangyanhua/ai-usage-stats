use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::adapters::project;
use crate::domain::CursorSessionRecord;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHashEnrichment {
    pub models: BTreeSet<String>,
    pub files: BTreeSet<String>,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
}

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

/// 只读加载 ai_code_hashes，按 conversationId 聚合 enrich 字段。
pub fn load_hash_enrichments(home: &Path) -> Result<BTreeMap<String, SessionHashEnrichment>, String> {
    let db_path = home.join(".cursor/ai-tracking/ai-code-tracking.db");
    if !db_path.exists() {
        return Ok(BTreeMap::new());
    }
    let conn = open_readonly(&db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT conversationId, model, timestamp, fileName
            FROM ai_code_hashes
            WHERE conversationId IS NOT NULL AND conversationId != ''
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;

    let mut enrichments: BTreeMap<String, SessionHashEnrichment> = BTreeMap::new();
    for row in rows {
        let (conversation_id, model, timestamp, file_name) = row.map_err(|e| e.to_string())?;
        let entry = enrichments.entry(conversation_id).or_default();
        if let Some(model) = model.filter(|value| !value.is_empty()) {
            entry.models.insert(model);
        }
        if let Some(file_name) = file_name.filter(|value| !value.is_empty()) {
            entry.files.insert(file_name);
        }
        if let Some(timestamp) = timestamp {
            entry.first_ms = Some(match entry.first_ms {
                Some(current) => current.min(timestamp),
                None => timestamp,
            });
            entry.last_ms = Some(match entry.last_ms {
                Some(current) => current.max(timestamp),
                None => timestamp,
            });
        }
    }

    Ok(enrichments)
}

impl Default for SessionHashEnrichment {
    fn default() -> Self {
        Self {
            models: BTreeSet::new(),
            files: BTreeSet::new(),
            first_ms: None,
            last_ms: None,
        }
    }
}

pub fn apply_hash_enrichment(
    record: &mut CursorSessionRecord,
    enrichment: &SessionHashEnrichment,
) -> Result<(), String> {
    record.models_json =
        serde_json::to_string(&enrichment.models.iter().collect::<Vec<_>>()).map_err(|e| e.to_string())?;
    record.files_touched = enrichment.files.len() as i64;
    if let Some(ms) = enrichment.first_ms {
        record.first_seen_at = millis_to_rfc3339(ms);
    }
    if let Some(ms) = enrichment.last_ms {
        record.last_seen_at = millis_to_rfc3339(ms);
    }
    Ok(())
}

fn open_readonly(path: &PathBuf) -> Result<rusqlite::Connection, String> {
    let uri = format!("file:{}?mode=ro", path.to_string_lossy());
    rusqlite::Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| e.to_string())
}

fn millis_to_rfc3339(millis: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(millis).map(|dt| dt.to_rfc3339())
}
