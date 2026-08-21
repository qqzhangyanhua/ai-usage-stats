use std::collections::BTreeSet;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::domain::{
    ConversationAttachment, ConversationAttachmentContentDto,
    ConversationAttachmentKind as AttachmentKind, ConversationAttachmentStatus as AttachmentStatus,
    ConversationDetailDto, ConversationEvent, ConversationEventActor as EventActor,
    ConversationEventCapabilityStatus as EventStatus, ConversationEventContentDto,
    ConversationEventContentStatus as ContentStatus, ConversationEventKind as EventKind,
    ConversationExportDto, ConversationExportFormat, ConversationMessage, ConversationPage,
    ConversationQuery, ConversationSessionRow, Source, UsageRecord,
};
use crate::ingest;

const DEFAULT_PAGE_SIZE: u32 = 20;
const MAX_PAGE_SIZE: u32 = 200;
const TITLE_MAX_CHARS: usize = 80;
const CAPABILITY_MESSAGES: &str = "messages";
const CAPABILITY_EVENTS: &str = "events";
const CAPABILITY_USAGE: &str = "usage";
const EXPERIMENTAL: &str = "experimental";
const LARGE_CONTENT_THRESHOLD: usize = 4_096;
const CONTENT_PREVIEW_CHARS: usize = 2_000;
const THUMBNAIL_MAX_WIDTH: u32 = 320;
const THUMBNAIL_MAX_HEIGHT: u32 = 240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationIndexIssue {
    pub path: String,
    pub message: String,
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
            match parse_codex_file(&path, false) {
                Ok(parsed) => {
                    seen_session_ids.insert(parsed.session.session_id.clone());
                    upsert_session(conn, &parsed.session)?;
                }
                Err(message) => issues.push(ConversationIndexIssue {
                    path: path.to_string_lossy().to_string(),
                    message,
                }),
            }
        }
    }
    if issues.is_empty() {
        reconcile_sessions(conn, &seen_session_ids)?;
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
               source_file, capabilities_json, support_status
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
    let (source, session, path) = load_trusted_session(conn, home, source, session_id)?;
    let parsed = parse_codex_file(&path, false)?;
    ensure_matching_session(&parsed, &session)?;
    let usage_records = load_usage_records(conn, source, session_id)?;
    Ok(ConversationDetailDto {
        session,
        messages: parsed.messages,
        events: parsed.events,
        usage_records,
    })
}

pub fn load_event_content(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
    sequence: u32,
) -> Result<ConversationEventContentDto, String> {
    let (_, session, path) = load_trusted_session(conn, home, source, session_id)?;
    let parsed = parse_codex_file(&path, true)?;
    ensure_matching_session(&parsed, &session)?;
    let event = parsed
        .events
        .into_iter()
        .find(|event| event.sequence == sequence)
        .ok_or_else(|| "原始文件中未找到该事件".to_string())?;
    Ok(ConversationEventContentDto {
        sequence,
        text: event.text,
        details: event.details,
    })
}

pub fn load_attachment(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
    attachment_id: &str,
) -> Result<ConversationAttachmentContentDto, String> {
    let candidate = resolve_attachment(conn, home, source, session_id, attachment_id)?;
    let data_url = attachment_data_url(&candidate)?;
    Ok(ConversationAttachmentContentDto {
        attachment: candidate.attachment,
        data_url,
    })
}

pub fn load_attachment_thumbnail(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
    attachment_id: &str,
) -> Result<ConversationAttachmentContentDto, String> {
    let candidate = resolve_attachment(conn, home, source, session_id, attachment_id)?;
    let data_url = attachment_thumbnail_data_url(&candidate)?;
    Ok(ConversationAttachmentContentDto {
        attachment: candidate.attachment,
        data_url,
    })
}

fn resolve_attachment(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
    attachment_id: &str,
) -> Result<AttachmentCandidate, String> {
    let (_, session, path) = load_trusted_session(conn, home, source, session_id)?;
    let parsed = parse_codex_file(&path, true)?;
    ensure_matching_session(&parsed, &session)?;
    let event = parsed
        .events
        .iter()
        .find(|event| {
            event
                .attachments
                .iter()
                .any(|attachment| attachment.id == attachment_id)
        })
        .ok_or_else(|| "原始文件中未找到该附件".to_string())?;
    let attachment = event
        .attachments
        .iter()
        .find(|attachment| attachment.id == attachment_id)
        .cloned()
        .ok_or_else(|| "原始文件中未找到该附件".to_string())?;
    if attachment.kind != AttachmentKind::Image {
        return Err("该附件不是可预览的图片".to_string());
    }
    let payload = read_source_payload(&path, event.sequence)?;
    let mut candidate = attachment_candidates(event.sequence, &payload, &parsed.session.project)
        .into_iter()
        .find(|candidate| candidate.attachment.id == attachment_id)
        .ok_or_else(|| "原始文件中未找到该附件".to_string())?;
    candidate.attachment = attachment;
    ensure_attachment_path_allowed(&candidate, &parsed.session.project)?;
    Ok(candidate)
}

pub fn build_export(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
    format: ConversationExportFormat,
) -> Result<ConversationExportDto, String> {
    let (_, session, path) = load_trusted_session(conn, home, source, session_id)?;
    let raw = fs::read(&path).map_err(|error| format!("读取原始文件失败：{error}"))?;
    let parsed = parse_codex_file(&path, true)?;
    ensure_matching_session(&parsed, &session)?;
    let base_name = safe_export_name(&parsed.session.title, &session.session_id);
    match format {
        ConversationExportFormat::Json => Ok(ConversationExportDto {
            default_name: format!("{base_name}.jsonl"),
            content: raw,
        }),
        ConversationExportFormat::Markdown => Ok(ConversationExportDto {
            default_name: format!("{base_name}.md"),
            content: render_markdown_export(&parsed).into_bytes(),
        }),
    }
}

fn safe_export_name(title: &str, session_id: &str) -> String {
    let source = if title.trim().is_empty() {
        session_id
    } else {
        title.trim()
    };
    let name: String = source
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => character,
        })
        .take(100)
        .collect();
    if name.is_empty() {
        "conversation".to_string()
    } else {
        name
    }
}

fn render_markdown_export(parsed: &ParsedCodexConversation) -> String {
    let session = &parsed.session;
    let mut markdown = format!(
        "# {}\n\n- 来源：{}\n- 会话 ID：`{}`\n- 项目：{}\n- 模型：{}\n- 开始：{}\n- 结束：{}\n\n",
        session.title,
        session.source,
        session.session_id,
        explicit_value(&session.project),
        explicit_value(&session.model),
        explicit_value(&session.started_at),
        explicit_value(&session.ended_at),
    );
    for event in &parsed.events {
        markdown.push_str(&format!(
            "---\n\n## {} · {}\n\n- 时间：{}\n",
            event.sequence,
            event.kind.as_str(),
            event.occurred_at.as_deref().unwrap_or("时间缺失")
        ));
        if let Some(actor) = event.actor {
            markdown.push_str(&format!("- 角色：{}\n", actor.as_str()));
        }
        if let Some(name) = &event.name {
            markdown.push_str(&format!("- 名称：`{name}`\n"));
        }
        if let Some(text) = &event.text {
            markdown.push('\n');
            markdown.push_str(text);
            markdown.push('\n');
        }
        if !event.attachments.is_empty() {
            markdown.push_str("\n### 附件\n\n");
            for attachment in &event.attachments {
                let status = match attachment.status {
                    AttachmentStatus::Available => "可用",
                    AttachmentStatus::Missing => "附件缺失",
                    AttachmentStatus::Embedded => "内嵌",
                    AttachmentStatus::Unsupported => "不支持应用内加载",
                };
                let size = attachment
                    .size_bytes
                    .map(|size| format!("{size} bytes"))
                    .unwrap_or_else(|| "大小未知".to_string());
                markdown.push_str(&format!(
                    "- **{}** · `{}` · {} · {} · {}\n",
                    attachment.name, attachment.original_path, attachment.media_type, size, status
                ));
            }
        }
        if let Some(details) = export_details(&event.details) {
            markdown.push_str("\n<details><summary>原始事件数据</summary>\n\n```json\n");
            markdown.push_str(&details);
            markdown.push_str("\n```\n\n</details>\n");
        }
    }
    markdown
}

fn explicit_value(value: &str) -> &str {
    if value.is_empty() {
        "缺失"
    } else {
        value
    }
}

fn export_details(details: &Value) -> Option<String> {
    let mut details = details.clone();
    if let Value::Object(object) = &mut details {
        object.remove("content");
        object.remove("message");
        object.remove("output");
        object.remove("result");
        if object.is_empty() {
            return None;
        }
    } else if details.is_null() {
        return None;
    }
    serde_json::to_string_pretty(&details).ok()
}

fn parse_codex_file(
    path: &Path,
    include_deferred_content: bool,
) -> Result<ParsedCodexConversation, String> {
    let content = fs::read_to_string(path).map_err(|error| format!("读取原始文件失败：{error}"))?;
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
        let value: Value = serde_json::from_str(raw)
            .map_err(|error| format!("第 {} 行 JSON 无效：{error}", index + 1))?;
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
                } else if let Some(event) =
                    response_semantic_event(index, &timestamp, payload, include_deferred_content)
                {
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
    populate_attachments(&mut events, &project);
    strip_message_bodies_from_details(&mut events);
    deduplicate_message_channels(&mut events);
    events.sort_by(compare_event_order);

    if session_id.is_empty() {
        return Err("缺少 Codex 会话 ID".to_string());
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
        attachments: Vec::new(),
        capability_status: if occurred_at.is_empty() {
            EventStatus::MissingTimestamp
        } else {
            EventStatus::Complete
        },
        content_status: ContentStatus::Complete,
    }
}

fn response_semantic_event(
    sequence: usize,
    occurred_at: &str,
    payload: &Value,
    include_deferred_content: bool,
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
        "function_call_output" | "custom_tool_call_output" => Some(tool_result_event(
            sequence,
            occurred_at,
            payload,
            include_deferred_content,
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

fn tool_result_event(
    sequence: usize,
    occurred_at: &str,
    payload: &Value,
    include_deferred_content: bool,
) -> ConversationEvent {
    let text = optional_text(payload, &["output", "result"]);
    let should_defer = !include_deferred_content
        && text
            .as_ref()
            .is_some_and(|text| text.len() > LARGE_CONTENT_THRESHOLD);
    let mut details = payload.clone();
    let rendered_text = if should_defer {
        if let Value::Object(object) = &mut details {
            object.remove("output");
            object.remove("result");
        }
        text.map(|text| text.chars().take(CONTENT_PREVIEW_CHARS).collect())
    } else {
        text
    };
    let mut event = semantic_event(
        sequence,
        EventKind::ToolResult,
        occurred_at,
        Some(EventActor::Tool),
        optional_text(payload, &["name"]),
        rendered_text,
        details,
    );
    if should_defer {
        event.content_status = ContentStatus::Deferred;
    }
    event
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

struct AttachmentCandidate {
    attachment: ConversationAttachment,
    source: String,
    resolved_path: Option<PathBuf>,
}

fn populate_attachments(events: &mut [ConversationEvent], project: &str) {
    for event in events {
        event.attachments = attachment_candidates(event.sequence, &event.details, project)
            .into_iter()
            .map(|candidate| candidate.attachment)
            .collect();
    }
}

fn strip_message_bodies_from_details(events: &mut [ConversationEvent]) {
    for event in events {
        if event.kind != EventKind::Message {
            continue;
        }
        if let Value::Object(object) = &mut event.details {
            object.remove("content");
            object.remove("message");
            object.remove("attachments");
        }
    }
}

fn read_source_payload(path: &Path, sequence: u32) -> Result<Value, String> {
    let content = fs::read_to_string(path).map_err(|error| format!("读取原始文件失败：{error}"))?;
    let raw = content
        .lines()
        .nth(sequence as usize)
        .ok_or_else(|| "原始文件中未找到附件所在事件".to_string())?;
    let value: Value =
        serde_json::from_str(raw).map_err(|error| format!("附件所在事件 JSON 无效：{error}"))?;
    Ok(value.get("payload").cloned().unwrap_or(value))
}

fn attachment_candidates(
    sequence: u32,
    payload: &Value,
    project: &str,
) -> Vec<AttachmentCandidate> {
    let mut values = Vec::new();
    for key in ["content", "attachments"] {
        match payload.get(key) {
            Some(Value::Array(items)) => values.extend(items),
            Some(value @ Value::Object(_)) => values.push(value),
            _ => {}
        }
    }
    values
        .into_iter()
        .filter_map(|value| attachment_candidate(value, project))
        .enumerate()
        .map(|(index, mut candidate)| {
            candidate.attachment.id = format!("{sequence}:{index}");
            candidate
        })
        .collect()
}

fn attachment_candidate(value: &Value, project: &str) -> Option<AttachmentCandidate> {
    let object = value.as_object()?;
    let raw_type = object.get("type").and_then(Value::as_str).unwrap_or("");
    let kind = if raw_type.contains("image") {
        AttachmentKind::Image
    } else if raw_type.contains("file") || raw_type.contains("attachment") {
        AttachmentKind::File
    } else {
        return None;
    };
    let source = ["file_path", "path", "url", "image_url"]
        .iter()
        .find_map(|key| object.get(*key).and_then(attachment_source_value))?;
    let embedded = source.starts_with("data:");
    let remote = source.starts_with("http://") || source.starts_with("https://");
    let resolved_path = if embedded || remote {
        None
    } else {
        let path = PathBuf::from(&source);
        Some(if path.is_absolute() || project.is_empty() {
            path
        } else {
            PathBuf::from(project).join(path)
        })
    };
    let metadata = resolved_path
        .as_ref()
        .and_then(|path| fs::metadata(path).ok());
    let status = if embedded {
        AttachmentStatus::Embedded
    } else if remote {
        AttachmentStatus::Unsupported
    } else if metadata.is_some() {
        AttachmentStatus::Available
    } else {
        AttachmentStatus::Missing
    };
    let original_path = if embedded {
        "内嵌图片数据".to_string()
    } else {
        source.clone()
    };
    let name = first_text(value, &["name", "file_name"]);
    let name = if name.is_empty() {
        Path::new(&original_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(if kind == AttachmentKind::Image {
                "image"
            } else {
                "attachment"
            })
            .to_string()
    } else {
        name
    };
    let media_type = optional_text(value, &["mime_type", "media_type"])
        .unwrap_or_else(|| infer_media_type(&name, kind));
    Some(AttachmentCandidate {
        attachment: ConversationAttachment {
            id: String::new(),
            kind,
            name,
            original_path,
            media_type,
            size_bytes: metadata.map(|metadata| metadata.len()),
            status,
        },
        source,
        resolved_path,
    })
}

fn attachment_source_value(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.get("url").and_then(Value::as_str).map(str::to_string))
}

fn infer_media_type(name: &str, kind: AttachmentKind) -> String {
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "md" => "text/markdown",
        "txt" => "text/plain",
        _ if kind == AttachmentKind::Image => "image/*",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn ensure_attachment_path_allowed(
    candidate: &AttachmentCandidate,
    project: &str,
) -> Result<(), String> {
    if candidate.attachment.status != AttachmentStatus::Available {
        return Ok(());
    }
    let path = candidate
        .resolved_path
        .as_ref()
        .ok_or_else(|| "附件路径不可用".to_string())?;
    let canonical_path =
        fs::canonicalize(path).map_err(|_| "原附件已不存在，无法加载图片".to_string())?;
    let project_path = Path::new(project);
    if !project_path.is_absolute() {
        return Err("附件路径不在会话项目允许的目录内".to_string());
    }
    let project_root =
        fs::canonicalize(project_path).map_err(|_| "会话项目目录不可用".to_string())?;
    if project_root.parent().is_some() && canonical_path.starts_with(project_root) {
        Ok(())
    } else {
        Err("附件路径不在会话项目允许的目录内".to_string())
    }
}

fn attachment_data_url(candidate: &AttachmentCandidate) -> Result<String, String> {
    if candidate.attachment.status == AttachmentStatus::Embedded {
        if candidate.source.starts_with("data:image/") {
            return Ok(candidate.source.clone());
        }
        return Err("内嵌附件不是可预览的图片".to_string());
    }
    let bytes = attachment_bytes(candidate)?;
    Ok(format!(
        "data:{};base64,{}",
        candidate.attachment.media_type,
        BASE64_STANDARD.encode(bytes)
    ))
}

fn attachment_thumbnail_data_url(candidate: &AttachmentCandidate) -> Result<String, String> {
    let bytes = attachment_bytes(candidate)?;
    let image =
        image::load_from_memory(&bytes).map_err(|error| format!("图片格式无效：{error}"))?;
    let thumbnail = image.thumbnail(
        image.width().min(THUMBNAIL_MAX_WIDTH),
        image.height().min(THUMBNAIL_MAX_HEIGHT),
    );
    let mut encoded = Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| format!("生成图片缩略图失败：{error}"))?;
    Ok(format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(encoded.into_inner())
    ))
}

fn attachment_bytes(candidate: &AttachmentCandidate) -> Result<Vec<u8>, String> {
    match candidate.attachment.status {
        AttachmentStatus::Embedded => {
            let (metadata, encoded) = candidate
                .source
                .split_once(',')
                .ok_or_else(|| "内嵌图片数据无效".to_string())?;
            if !metadata.starts_with("data:image/") || !metadata.ends_with(";base64") {
                return Err("内嵌附件不是可预览的图片".to_string());
            }
            BASE64_STANDARD
                .decode(encoded)
                .map_err(|error| format!("内嵌图片数据无效：{error}"))
        }
        AttachmentStatus::Missing => Err("原附件已不存在，无法加载图片".to_string()),
        AttachmentStatus::Unsupported => Err("远程附件不在应用内加载".to_string()),
        AttachmentStatus::Available => {
            let path = candidate
                .resolved_path
                .as_ref()
                .ok_or_else(|| "附件路径不可用".to_string())?;
            fs::read(path).map_err(|error| format!("读取原附件失败：{error}"))
        }
    }
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

fn upsert_session(conn: &Connection, session: &ConversationSessionRow) -> Result<(), String> {
    let capabilities = serde_json::to_string(&session.capabilities).map_err(|e| e.to_string())?;
    conn.execute(
        r#"
        INSERT INTO conversation_sessions(
            source, session_id, title, project, model, started_at, ended_at,
            source_file, capabilities_json, support_status
        ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
        ON CONFLICT(source, session_id) DO UPDATE SET
            title = excluded.title,
            project = excluded.project,
            model = excluded.model,
            started_at = excluded.started_at,
            ended_at = excluded.ended_at,
            source_file = excluded.source_file,
            capabilities_json = excluded.capabilities_json,
            support_status = excluded.support_status
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
               source_file, capabilities_json, support_status
        FROM conversation_sessions WHERE source = ?1 AND session_id = ?2
        "#,
        params![source, session_id],
        row_from_sql,
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn load_trusted_session(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
) -> Result<(Source, ConversationSessionRow, PathBuf), String> {
    let source = Source::parse(source).filter(|source| *source == Source::Codex);
    let Some(source) = source else {
        return Err("当前仅支持读取 Codex 对话详情".to_string());
    };
    let session = load_session(conn, source.as_str(), session_id)?
        .ok_or_else(|| "未找到该对话记录".to_string())?;
    let path = PathBuf::from(&session.source_file);
    if !path.exists() {
        return Err("原始文件已不存在，无法读取对话详情".to_string());
    }
    ensure_trusted_path(&path, &ingest::source_scan_dirs(home, source))?;
    Ok((source, session, path))
}

fn ensure_matching_session(
    parsed: &ParsedCodexConversation,
    session: &ConversationSessionRow,
) -> Result<(), String> {
    if parsed.session.session_id == session.session_id {
        Ok(())
    } else {
        Err("原始文件中的会话 ID 与索引不一致".to_string())
    }
}

fn reconcile_sessions(
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
            "DELETE FROM conversation_sessions WHERE source = ?1 AND session_id = ?2",
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
    })
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
