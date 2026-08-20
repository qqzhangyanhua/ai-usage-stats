use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::domain::{
    ConversationDetailDto, ConversationEvent, ConversationEventActor as EventActor,
    ConversationEventCapabilityStatus as EventStatus, ConversationEventKind as EventKind,
    ConversationMessage, ConversationPage, ConversationQuery, ConversationSessionRow, Source,
    UsageRecord,
};
use crate::ingest;

const DEFAULT_PAGE_SIZE: u32 = 20;
const MAX_PAGE_SIZE: u32 = 200;
const TITLE_MAX_CHARS: usize = 80;
const CAPABILITY_MESSAGES: &str = "messages";
const CAPABILITY_EVENTS: &str = "events";
const CAPABILITY_USAGE: &str = "usage";
const EXPERIMENTAL: &str = "experimental";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationIndexIssue {
    pub path: String,
    pub message: String,
    pub event_type: Option<String>,
    pub line: Option<u64>,
}

struct CachedConversationFingerprint {
    session_id: String,
    source_file_mtime_ms: i64,
    source_file_size: i64,
}

struct ParsedCodexConversation {
    session: ConversationSessionRow,
    messages: Vec<ConversationMessage>,
    events: Vec<ConversationEvent>,
}

struct PendingMessageDelta {
    sequence: u32,
    occurred_at: String,
    role: String,
    text: String,
}

pub fn refresh_codex(
    conn: &Connection,
    home: &Path,
) -> Result<Vec<ConversationIndexIssue>, String> {
    let roots = ingest::source_scan_dirs(home, Source::Codex);
    refresh_codex_in_roots(conn, &roots)
}

pub(crate) fn refresh_codex_in_roots(
    conn: &Connection,
    roots: &[PathBuf],
) -> Result<Vec<ConversationIndexIssue>, String> {
    let mut issues = Vec::new();
    let mut seen_session_ids = BTreeSet::new();
    for root in roots {
        for path in ingest::walk_files(root, "jsonl")? {
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    issues.push(ConversationIndexIssue {
                        path: path.to_string_lossy().to_string(),
                        message: format!("读取文件元数据失败：{error}"),
                        event_type: None,
                        line: None,
                    });
                    continue;
                }
            };
            let mtime_ms = modified_millis(&metadata);
            let size = metadata.len() as i64;
            if let Some(cached) = load_cached_fingerprint(conn, &path)? {
                if cached.source_file_mtime_ms == mtime_ms && cached.source_file_size == size {
                    seen_session_ids.insert(cached.session_id);
                    continue;
                }
            }
            match parse_codex_file(&path) {
                Ok(parsed) => {
                    seen_session_ids.insert(parsed.session.session_id.clone());
                    upsert_session(conn, &parsed.session, mtime_ms, size)?;
                }
                Err(issue) => issues.push(issue),
            }
        }
    }
    if issues.is_empty() {
        tombstone_missing_sessions(conn, &seen_session_ids)?;
    }
    Ok(issues)
}

pub fn sessions_page(
    conn: &Connection,
    query: &ConversationQuery,
) -> Result<ConversationPage, String> {
    let search = query.search.as_deref().unwrap_or("").trim();
    let pattern = format!("%{}%", escape_like(search));
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let offset = i64::from((page - 1) * page_size);

    let predicate = r#"
        (?1 = '' OR title LIKE ?2 ESCAPE '\' OR source LIKE ?2 ESCAPE '\'
         OR project LIKE ?2 ESCAPE '\' OR model LIKE ?2 ESCAPE '\'
         OR session_id LIKE ?2 ESCAPE '\' OR started_at LIKE ?2 ESCAPE '\'
         OR ended_at LIKE ?2 ESCAPE '\')
    "#;
    let total = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM conversation_sessions WHERE {predicate}"),
            params![search, pattern],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())? as u32;

    let sql = format!(
        r#"
        SELECT source, session_id, title, project, model, started_at, ended_at,
               source_file, capabilities_json, support_status, file_available
        FROM conversation_sessions
        WHERE {predicate}
        ORDER BY ended_at DESC, source ASC, session_id ASC
        LIMIT ?3 OFFSET ?4
        "#
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            params![search, pattern, i64::from(page_size), offset],
            row_from_sql,
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(ConversationPage { rows, total })
}

pub fn load_detail(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
) -> Result<ConversationDetailDto, String> {
    let source = Source::parse(source).filter(|source| *source == Source::Codex);
    let Some(source) = source else {
        return Err("当前仅支持读取 Codex 对话详情".to_string());
    };
    let session = load_session(conn, source.as_str(), session_id)?
        .ok_or_else(|| "未找到该对话记录".to_string())?;
    if !session.file_available {
        return Err("原文件已删除，详情不可读取".to_string());
    }
    let path = PathBuf::from(&session.source_file);
    if !path.exists() {
        return Err("原文件已删除，详情不可读取".to_string());
    }
    ensure_trusted_path(&path, &ingest::source_scan_dirs(home, source))?;
    let parsed = parse_codex_file(&path).map_err(|issue| issue.message)?;
    if parsed.session.session_id != session.session_id {
        return Err("原始文件中的会话 ID 与索引不一致".to_string());
    }
    let usage_records = load_usage_records(conn, source, session_id)?;
    Ok(ConversationDetailDto {
        session,
        messages: parsed.messages,
        events: parsed.events,
        usage_records,
    })
}

fn parse_codex_file(path: &Path) -> Result<ParsedCodexConversation, ConversationIndexIssue> {
    let content = fs::read_to_string(path).map_err(|error| ConversationIndexIssue {
        path: path.to_string_lossy().to_string(),
        message: format!("读取原始文件失败：{error}"),
        event_type: None,
        line: None,
    })?;
    let mut session_id = String::new();
    let mut title = String::new();
    let mut project = String::new();
    let mut model = String::new();
    let mut started_at = String::new();
    let mut ended_at = String::new();
    let mut response_messages = Vec::new();
    let mut event_messages = Vec::new();
    let mut events = Vec::new();
    let mut pending_delta = None;

    for (index, raw) in content.lines().enumerate() {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(raw).map_err(|error| ConversationIndexIssue {
            path: path.to_string_lossy().to_string(),
            message: format!("JSON 无效：{error}"),
            event_type: Some("json_line".to_string()),
            line: Some((index + 1) as u64),
        })?;
        let timestamp = text_field(&value, "timestamp");
        if !timestamp.is_empty() {
            if started_at.is_empty() {
                started_at = timestamp.clone();
            }
            ended_at = timestamp.clone();
        }
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        let payload = value.get("payload").unwrap_or(&Value::Null);
        match kind {
            "session_meta" => {
                flush_message_delta(&mut pending_delta, &mut event_messages, &mut events);
                session_id = first_text(payload, &["id", "session_id"]);
                project = first_text(payload, &["cwd"]);
                title = first_text(payload, &["title", "name"]);
                events.push(semantic_event(
                    index,
                    EventKind::SystemStatus,
                    &timestamp,
                    None,
                    Some("session_started".to_string()),
                    None,
                    payload.clone(),
                ));
            }
            "turn_context" => {
                flush_message_delta(&mut pending_delta, &mut event_messages, &mut events);
                let next_project = first_text(payload, &["cwd"]);
                if !next_project.is_empty() {
                    project = next_project;
                }
                let next_model = first_text(payload, &["model"]);
                if !next_model.is_empty() && next_model != model {
                    events.push(semantic_event(
                        index,
                        EventKind::ModelChange,
                        &timestamp,
                        None,
                        Some(next_model.clone()),
                        None,
                        payload.clone(),
                    ));
                    model = next_model;
                }
            }
            "response_item" => {
                flush_message_delta(&mut pending_delta, &mut event_messages, &mut events);
                if let Some(message) = response_message(payload, &timestamp) {
                    events.push(message_event(index, &message, payload.clone()));
                    response_messages.push(message);
                } else if let Some(event) = response_semantic_event(index, &timestamp, payload) {
                    events.push(event);
                }
            }
            "event_msg" => {
                let event_kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
                match event_kind {
                    "agent_message_delta" => append_message_delta(
                        &mut pending_delta,
                        index,
                        &timestamp,
                        "assistant",
                        payload,
                    ),
                    "token_count" | "heartbeat" => {}
                    _ => {
                        flush_message_delta(&mut pending_delta, &mut event_messages, &mut events);
                        if let Some(message) = event_message(payload, &timestamp) {
                            events.push(message_event(index, &message, payload.clone()));
                            event_messages.push(message);
                        } else {
                            events.push(event_msg_semantic_event(
                                index, &timestamp, event_kind, payload,
                            ));
                        }
                    }
                }
            }
            _ => {
                flush_message_delta(&mut pending_delta, &mut event_messages, &mut events);
                events.push(unadapted_event(index, &timestamp, kind, value.clone()));
            }
        }
    }
    flush_message_delta(&mut pending_delta, &mut event_messages, &mut events);
    deduplicate_message_channels(&mut events);
    events.sort_by(compare_event_order);

    if session_id.is_empty() {
        return Err(ConversationIndexIssue {
            path: path.to_string_lossy().to_string(),
            message: "缺少 Codex 会话 ID".to_string(),
            event_type: Some("session_meta".to_string()),
            line: None,
        });
    }
    let messages = if response_messages.is_empty() {
        event_messages
    } else {
        response_messages
    };
    if title.is_empty() {
        title = messages
            .iter()
            .find(|message| message.role == "user")
            .map(|message| truncate_title(&message.text))
            .unwrap_or_else(|| session_id.clone());
    }
    let mut capabilities = Vec::new();
    if !messages.is_empty() {
        capabilities.push(CAPABILITY_MESSAGES.to_string());
    }
    if !events.is_empty() {
        capabilities.push(CAPABILITY_EVENTS.to_string());
    }
    capabilities.push(CAPABILITY_USAGE.to_string());
    let session = ConversationSessionRow {
        source: Source::Codex.as_str().to_string(),
        session_id,
        title,
        project,
        model,
        started_at,
        ended_at,
        source_file: path.to_string_lossy().to_string(),
        capabilities,
        support_status: EXPERIMENTAL.to_string(),
        file_available: true,
    };
    Ok(ParsedCodexConversation {
        session,
        messages,
        events,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MessageChannel {
    Response,
    Event,
    Delta,
}

fn deduplicate_message_channels(events: &mut Vec<ConversationEvent>) {
    let mut current_actor = None;
    let mut seen: Vec<(String, MessageChannel)> = Vec::new();
    events.retain(|event| {
        if event.kind != EventKind::Message {
            return true;
        }
        let Some(actor) = event.actor.as_ref() else {
            return true;
        };
        let Some(text) = event.text.as_ref() else {
            return true;
        };
        if current_actor.as_ref() != Some(actor) {
            current_actor = Some(*actor);
            seen.clear();
        }
        let channel = match event.details.get("type").and_then(Value::as_str) {
            Some("message") => MessageChannel::Response,
            Some("user_message" | "agent_message") => MessageChannel::Event,
            _ => MessageChannel::Delta,
        };
        if seen
            .iter()
            .any(|(seen_text, seen_channel)| seen_text == text && *seen_channel != channel)
        {
            return false;
        }
        seen.push((text.clone(), channel));
        true
    });
}

fn compare_event_order(left: &ConversationEvent, right: &ConversationEvent) -> std::cmp::Ordering {
    match (&left.occurred_at, &right.occurred_at) {
        (Some(left_time), Some(right_time)) => compare_timestamps(left_time, right_time)
            .then_with(|| left.sequence.cmp(&right.sequence)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.sequence.cmp(&right.sequence),
    }
}

fn compare_timestamps(left: &str, right: &str) -> std::cmp::Ordering {
    match (
        chrono::DateTime::parse_from_rfc3339(left),
        chrono::DateTime::parse_from_rfc3339(right),
    ) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn append_message_delta(
    pending: &mut Option<PendingMessageDelta>,
    sequence: usize,
    occurred_at: &str,
    role: &str,
    payload: &Value,
) {
    let delta = first_text(payload, &["delta", "message", "text"]);
    if delta.is_empty() {
        return;
    }
    match pending {
        Some(current) if current.role == role => current.text.push_str(&delta),
        Some(_) => {}
        None => {
            *pending = Some(PendingMessageDelta {
                sequence: sequence as u32,
                occurred_at: occurred_at.to_string(),
                role: role.to_string(),
                text: delta,
            });
        }
    }
}

fn flush_message_delta(
    pending: &mut Option<PendingMessageDelta>,
    messages: &mut Vec<ConversationMessage>,
    events: &mut Vec<ConversationEvent>,
) {
    let Some(delta) = pending.take() else {
        return;
    };
    let Some(message) = message(&delta.role, &delta.occurred_at, &Value::String(delta.text)) else {
        return;
    };
    events.push(message_event(
        delta.sequence as usize,
        &message,
        Value::Null,
    ));
    messages.push(message);
}

fn message_event(
    sequence: usize,
    message: &ConversationMessage,
    details: Value,
) -> ConversationEvent {
    let actor = match message.role.as_str() {
        "user" => EventActor::User,
        "assistant" => EventActor::Assistant,
        _ => unreachable!("conversation messages only contain user or assistant roles"),
    };
    semantic_event(
        sequence,
        EventKind::Message,
        &message.occurred_at,
        Some(actor),
        None,
        Some(message.text.clone()),
        details,
    )
}

fn semantic_event(
    sequence: usize,
    kind: EventKind,
    occurred_at: &str,
    actor: Option<EventActor>,
    name: Option<String>,
    text: Option<String>,
    details: Value,
) -> ConversationEvent {
    ConversationEvent {
        sequence: sequence as u32,
        kind,
        occurred_at: (!occurred_at.is_empty()).then(|| occurred_at.to_string()),
        actor,
        name,
        text,
        details,
        capability_status: if occurred_at.is_empty() {
            EventStatus::MissingTimestamp
        } else {
            EventStatus::Complete
        },
    }
}

fn response_semantic_event(
    sequence: usize,
    occurred_at: &str,
    payload: &Value,
) -> Option<ConversationEvent> {
    let kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "message" => None,
        "function_call" | "custom_tool_call" | "web_search_call" | "local_shell_call" => {
            Some(semantic_event(
                sequence,
                EventKind::ToolCall,
                occurred_at,
                Some(EventActor::Assistant),
                optional_text(payload, &["name", "tool", "type"]),
                optional_text(payload, &["arguments", "input", "query", "command"]),
                payload.clone(),
            ))
        }
        "function_call_output" | "custom_tool_call_output" => Some(semantic_event(
            sequence,
            EventKind::ToolResult,
            occurred_at,
            Some(EventActor::Tool),
            optional_text(payload, &["name"]),
            optional_text(payload, &["output", "result"]),
            payload.clone(),
        )),
        "reasoning" => Some(semantic_event(
            sequence,
            EventKind::Plan,
            occurred_at,
            Some(EventActor::Assistant),
            None,
            optional_text(payload, &["summary", "text", "content"]),
            payload.clone(),
        )),
        "developer" | "system" => None,
        _ => Some(unadapted_event(
            sequence,
            occurred_at,
            kind,
            payload.clone(),
        )),
    }
}

fn event_msg_semantic_event(
    sequence: usize,
    occurred_at: &str,
    kind: &str,
    payload: &Value,
) -> ConversationEvent {
    match kind {
        "plan_update" | "agent_reasoning" => semantic_event(
            sequence,
            EventKind::Plan,
            occurred_at,
            Some(EventActor::Assistant),
            None,
            optional_text(payload, &["explanation", "message", "text"]),
            payload.clone(),
        ),
        "error" | "stream_error" => semantic_event(
            sequence,
            EventKind::Error,
            occurred_at,
            None,
            optional_text(payload, &["code", "type"]),
            optional_text(payload, &["message", "error"]),
            payload.clone(),
        ),
        "task_started" | "task_complete" | "turn_aborted" | "context_compacted" | "warning" => {
            semantic_event(
                sequence,
                EventKind::SystemStatus,
                occurred_at,
                None,
                Some(kind.to_string()),
                optional_text(payload, &["message", "reason", "text"]),
                payload.clone(),
            )
        }
        _ => unadapted_event(sequence, occurred_at, kind, payload.clone()),
    }
}

fn unadapted_event(
    sequence: usize,
    occurred_at: &str,
    raw_kind: &str,
    details: Value,
) -> ConversationEvent {
    let mut event = semantic_event(
        sequence,
        EventKind::Unadapted,
        occurred_at,
        None,
        Some(if raw_kind.is_empty() {
            "unknown".to_string()
        } else {
            raw_kind.to_string()
        }),
        None,
        details,
    );
    event.capability_status = if occurred_at.is_empty() {
        EventStatus::UnadaptedMissingTimestamp
    } else {
        EventStatus::Unadapted
    };
    event
}

fn optional_text(value: &Value, keys: &[&str]) -> Option<String> {
    let text = first_text(value, keys);
    (!text.is_empty()).then_some(text)
}

fn response_message(payload: &Value, occurred_at: &str) -> Option<ConversationMessage> {
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let role = payload.get("role").and_then(Value::as_str)?;
    if role != "user" && role != "assistant" {
        return None;
    }
    message(role, occurred_at, payload.get("content")?)
}

fn event_message(payload: &Value, occurred_at: &str) -> Option<ConversationMessage> {
    let role = match payload.get("type").and_then(Value::as_str)? {
        "user_message" => "user",
        "agent_message" => "assistant",
        _ => return None,
    };
    message(role, occurred_at, payload.get("message")?)
}

fn message(role: &str, occurred_at: &str, content: &Value) -> Option<ConversationMessage> {
    let text = content_text(content).trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(ConversationMessage {
        role: role.to_string(),
        occurred_at: occurred_at.to_string(),
        text,
    })
}

fn content_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(content_text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(object) => object
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| object.get("content").map(content_text))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn first_text(value: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn text_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn truncate_title(text: &str) -> String {
    let mut chars = text.chars();
    let title: String = chars.by_ref().take(TITLE_MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{title}…")
    } else {
        title
    }
}

fn upsert_session(
    conn: &Connection,
    session: &ConversationSessionRow,
    source_file_mtime_ms: i64,
    source_file_size: i64,
) -> Result<(), String> {
    let capabilities = serde_json::to_string(&session.capabilities).map_err(|e| e.to_string())?;
    conn.execute(
        r#"
        INSERT INTO conversation_sessions(
            source, session_id, title, project, model, started_at, ended_at,
            source_file, capabilities_json, support_status, file_available,
            source_file_mtime_ms, source_file_size
        ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
        ON CONFLICT(source, session_id) DO UPDATE SET
            title = excluded.title,
            project = excluded.project,
            model = excluded.model,
            started_at = excluded.started_at,
            ended_at = excluded.ended_at,
            source_file = excluded.source_file,
            capabilities_json = excluded.capabilities_json,
            support_status = excluded.support_status,
            file_available = excluded.file_available,
            source_file_mtime_ms = excluded.source_file_mtime_ms,
            source_file_size = excluded.source_file_size
        "#,
        params![
            session.source,
            session.session_id,
            session.title,
            session.project,
            session.model,
            session.started_at,
            session.ended_at,
            session.source_file,
            capabilities,
            session.support_status,
            session.file_available,
            source_file_mtime_ms,
            source_file_size,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn load_usage_records(
    conn: &Connection,
    source: Source,
    session_id: &str,
) -> Result<Vec<UsageRecord>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT occurred_at, model, provider, project, session_id, source_file,
                   input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                   reasoning_tokens, total_tokens, native_cost
            FROM usage_records
            WHERE source = ?1 AND session_id = ?2
            ORDER BY occurred_at ASC, rowid ASC
            "#,
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![source.as_str(), session_id], |row| {
            Ok(UsageRecord {
                occurred_at: row.get(0)?,
                source,
                model: row.get(1)?,
                provider: row.get(2)?,
                project: row.get(3)?,
                session_id: row.get(4)?,
                source_file: row.get(5)?,
                input_tokens: row.get(6)?,
                output_tokens: row.get(7)?,
                cache_read_tokens: row.get(8)?,
                cache_creation_tokens: row.get(9)?,
                reasoning_tokens: row.get(10)?,
                total_tokens: row.get(11)?,
                native_cost: row.get(12)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_session(
    conn: &Connection,
    source: &str,
    session_id: &str,
) -> Result<Option<ConversationSessionRow>, String> {
    conn.query_row(
        r#"
        SELECT source, session_id, title, project, model, started_at, ended_at,
               source_file, capabilities_json, support_status, file_available
        FROM conversation_sessions WHERE source = ?1 AND session_id = ?2
        "#,
        params![source, session_id],
        row_from_sql,
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn tombstone_missing_sessions(
    conn: &Connection,
    seen_session_ids: &BTreeSet<String>,
) -> Result<(), String> {
    let cached = conn
        .prepare("SELECT session_id FROM conversation_sessions WHERE source = ?1")
        .map_err(|e| e.to_string())?
        .query_map(params![Source::Codex.as_str()], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for session_id in cached {
        if seen_session_ids.contains(&session_id) {
            continue;
        }
        conn.execute(
            "UPDATE conversation_sessions SET file_available = 0 WHERE source = ?1 AND session_id = ?2",
            params![Source::Codex.as_str(), session_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationSessionRow> {
    let capabilities_json: String = row.get(8)?;
    Ok(ConversationSessionRow {
        source: row.get(0)?,
        session_id: row.get(1)?,
        title: row.get(2)?,
        project: row.get(3)?,
        model: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        source_file: row.get(7)?,
        capabilities: serde_json::from_str(&capabilities_json).unwrap_or_default(),
        support_status: row.get(9)?,
        file_available: row.get(10)?,
    })
}

fn load_cached_fingerprint(
    conn: &Connection,
    path: &Path,
) -> Result<Option<CachedConversationFingerprint>, String> {
    conn.query_row(
        r#"
        SELECT session_id, source_file_mtime_ms, source_file_size
        FROM conversation_sessions
        WHERE source = ?1 AND source_file = ?2 AND file_available = 1
        ORDER BY ended_at DESC, session_id ASC
        LIMIT 1
        "#,
        params![Source::Codex.as_str(), path.to_string_lossy().to_string()],
        |row| {
            Ok(CachedConversationFingerprint {
                session_id: row.get(0)?,
                source_file_mtime_ms: row.get(1)?,
                source_file_size: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

fn modified_millis(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn ensure_trusted_path(path: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let canonical_path =
        fs::canonicalize(path).map_err(|error| format!("无法验证原始文件路径：{error}"))?;
    for root in roots {
        let Ok(canonical_root) = fs::canonicalize(root) else {
            continue;
        };
        if canonical_path.starts_with(canonical_root) {
            return Ok(());
        }
    }
    Err("原始文件不在 Codex 允许的扫描目录内".to_string())
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
