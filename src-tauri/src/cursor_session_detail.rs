use std::path::Path;

use rusqlite::Connection;

use crate::adapters::cursor_session::{
    is_subagent_transcript, load_hash_files, merge_parsed_sessions,
    parse_cursor_session_transcript, session_dir_from_transcript, ParsedCursorSession,
};
use crate::domain::{
    CursorSessionDetailDto, CursorSessionListRow, CursorSessionRecord, CursorSessionToolRow,
};
use crate::store;

pub fn load_detail(
    conn: &Connection,
    home: &Path,
    source_file: &str,
) -> Result<CursorSessionDetailDto, String> {
    let source_file = source_file.trim();
    if source_file.is_empty() {
        return Err("缺少会话文件路径".to_string());
    }
    let record = store::load_cursor_session(conn, source_file)?
        .ok_or_else(|| "未找到该 Cursor 会话".to_string())?;

    let tools = tools_from_json(&record.tool_calls_json);
    let hash_files = load_hash_files(home, &record.session_id)?;
    let (paths, transcript_missing) = collect_paths(&record);

    Ok(CursorSessionDetailDto {
        session: list_row_from_record(&record, tools.iter().map(|row| row.call_count).sum()),
        tools,
        hash_files,
        read_paths: paths.read_paths.into_iter().collect(),
        write_paths: paths.write_paths.into_iter().collect(),
        transcript_missing,
    })
}

fn collect_paths(record: &CursorSessionRecord) -> (ParsedCursorSession, bool) {
    let parent = Path::new(&record.source_file);
    if !parent.exists() {
        return (ParsedCursorSession::default(), true);
    }
    let mut parsed = match read_parsed(parent) {
        Ok(parsed) => parsed,
        Err(_) => return (ParsedCursorSession::default(), true),
    };
    let mut missing = false;
    if let Some(dir) = session_dir_from_transcript(parent) {
        let subagents = dir.join("subagents");
        if subagents.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&subagents) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.ends_with(".jsonl"))
                    {
                        continue;
                    }
                    if !is_subagent_transcript(&path) {
                        continue;
                    }
                    match read_parsed(&path) {
                        Ok(extra) => merge_parsed_sessions(&mut parsed, &extra),
                        Err(_) => missing = true,
                    }
                }
            }
        }
    }
    (parsed, missing)
}

fn read_parsed(path: &Path) -> Result<ParsedCursorSession, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_cursor_session_transcript(&content)
}

fn tools_from_json(raw: &str) -> Vec<CursorSessionToolRow> {
    let map: std::collections::BTreeMap<String, i64> =
        serde_json::from_str(raw).unwrap_or_default();
    let mut tools: Vec<CursorSessionToolRow> = map
        .into_iter()
        .map(|(name, call_count)| CursorSessionToolRow { name, call_count })
        .collect();
    tools.sort_by(|a, b| {
        b.call_count
            .cmp(&a.call_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    tools
}

fn list_row_from_record(
    record: &CursorSessionRecord,
    tool_call_count: i64,
) -> CursorSessionListRow {
    CursorSessionListRow {
        session_id: record.session_id.clone(),
        project: if record.project.is_empty() {
            "未知项目".to_string()
        } else {
            record.project.clone()
        },
        turn_count: record.turn_count,
        success_count: record.success_count,
        error_count: record.error_count,
        aborted_count: record.aborted_count,
        user_prompt_count: record.user_prompt_count,
        subagent_count: record.subagent_count,
        models: serde_json::from_str(&record.models_json).unwrap_or_default(),
        sources: serde_json::from_str(&record.sources_json).unwrap_or_default(),
        tool_call_count,
        first_seen_at: record.first_seen_at.clone(),
        last_seen_at: record.last_seen_at.clone(),
        files_touched: record.files_touched,
        source_file: record.source_file.clone(),
    }
}
