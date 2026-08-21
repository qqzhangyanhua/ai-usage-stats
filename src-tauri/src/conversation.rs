use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use base64::prelude::*;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::domain::{
    ConversationAgentCapabilityStatus as AgentCapabilityStatus, ConversationAgentLink,
    ConversationAgentLinkStatus as AgentLinkStatus, ConversationAgentRelations,
    ConversationAttachment, ConversationAttachmentContentDto,
    ConversationAttachmentKind as AttachmentKind, ConversationAttachmentStatus as AttachmentStatus,
    ConversationDetailDto, ConversationDetailStateDto, ConversationEvent,
    ConversationEventActor as EventActor, ConversationEventCapabilityStatus as EventStatus,
    ConversationEventContentDto, ConversationEventContentStatus as ContentStatus,
    ConversationEventKind as EventKind, ConversationExportDto, ConversationExportFormat,
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
const DETAIL_READ_ATTEMPTS: usize = 3;
const LARGE_CONTENT_THRESHOLD: usize = 4_096;
const CONTENT_PREVIEW_CHARS: usize = 2_000;
const THUMBNAIL_MAX_WIDTH: u32 = 320;
const THUMBNAIL_MAX_HEIGHT: u32 = 240;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationIndexIssue {
    pub path: String,
    pub message: String,
    pub event_type: Option<String>,
    pub line: Option<u64>,
}

struct CachedConversationFingerprint {
    session_id: String,
    source_file_mtime_ns: i64,
    source_file_size: i64,
}

struct ParsedCodexConversation {
    session: ConversationSessionRow,
    messages: Vec<ConversationMessage>,
    events: Vec<ConversationEvent>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct IndexedAgentMetadata {
    parent_session_ids: Vec<String>,
    spawn_attempts: Vec<IndexedSpawnAttempt>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct IndexedSpawnAttempt {
    launch_event_id: String,
    child_session_id: Option<String>,
}

struct PendingMessageDelta {
    sequence: u32,
    occurred_at: String,
    role: String,
    text: String,
}

pub(crate) struct PreparedConversationDetail {
    source: Source,
    session: ConversationSessionRow,
    usage_records: Vec<UsageRecord>,
    agent_relations: ConversationAgentRelations,
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
    let mut grouped: BTreeMap<String, Vec<ParsedCodexConversation>> = BTreeMap::new();
    let mut unchanged_paths: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
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
            let mtime_ns = modified_nanos(&metadata);
            let size = metadata.len() as i64;
            let cached = load_cached_fingerprints(conn, &path)?;
            if let [cached] = cached.as_slice() {
                if cached.source_file_mtime_ns == mtime_ns && cached.source_file_size == size {
                    unchanged_paths
                        .entry(cached.session_id.clone())
                        .or_default()
                        .push(path);
                    continue;
                }
            }
            match parse_codex_file(&path) {
                Ok(parsed) => grouped
                    .entry(parsed.session.session_id.clone())
                    .or_default()
                    .push(parsed),
                Err(issue) => issues.push(issue),
            }
        }
    }

    let failed_paths_by_session = failed_session_paths(conn, &issues)?;
    let blocked_session_ids = failed_paths_by_session
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut scanned_paths_by_session: BTreeMap<String, BTreeSet<PathBuf>> = unchanged_paths
        .iter()
        .map(|(session_id, paths)| (session_id.clone(), paths.iter().cloned().collect()))
        .collect();
    for (session_id, parsed_files) in &grouped {
        scanned_paths_by_session
            .entry(session_id.clone())
            .or_default()
            .extend(
                parsed_files
                    .iter()
                    .map(|parsed| PathBuf::from(&parsed.session.source_file)),
            );
    }
    for (session_id, failed_paths) in &failed_paths_by_session {
        scanned_paths_by_session
            .entry(session_id.clone())
            .or_default()
            .extend(failed_paths.iter().cloned());
    }
    let mut incomplete_session_ids = BTreeSet::new();
    for (session_id, scanned_paths) in &scanned_paths_by_session {
        let indexed_paths = load_session_files(conn, Source::Codex.as_str(), session_id)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        if !indexed_paths.is_empty() && !indexed_paths.is_subset(scanned_paths) {
            let scanned_paths = scanned_paths.iter().cloned().collect::<Vec<_>>();
            update_session_files(conn, session_id, &scanned_paths, false)?;
            mark_session_unavailable(conn, session_id)?;
            incomplete_session_ids.insert(session_id.clone());
        }
    }
    for session_id in &incomplete_session_ids {
        grouped.remove(session_id);
        unchanged_paths.remove(session_id);
    }

    for (session_id, paths) in std::mem::take(&mut unchanged_paths) {
        let indexed_paths = load_session_files(conn, Source::Codex.as_str(), &session_id)?;
        let scanned = paths.iter().cloned().collect::<BTreeSet<_>>();
        let indexed = indexed_paths.into_iter().collect::<BTreeSet<_>>();
        if grouped.contains_key(&session_id) || scanned != indexed {
            for path in paths {
                match parse_codex_file(&path) {
                    Ok(parsed) => grouped.entry(session_id.clone()).or_default().push(parsed),
                    Err(issue) => issues.push(issue),
                }
            }
        } else {
            unchanged_paths.insert(session_id, scanned.into_iter().collect());
        }
    }

    let seen_session_ids = unchanged_paths
        .keys()
        .chain(grouped.keys())
        .chain(incomplete_session_ids.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    for (session_id, parsed_files) in grouped {
        if blocked_session_ids.contains(&session_id) {
            continue;
        }
        let source_files = parsed_files
            .iter()
            .map(|parsed| PathBuf::from(&parsed.session.source_file))
            .collect::<Vec<_>>();
        let merged = merge_parsed_conversations(parsed_files);
        let agent_metadata = extract_agent_metadata(&merged.events);
        let representative_metadata = fs::metadata(&merged.session.source_file)
            .map_err(|error| format!("读取文件元数据失败：{error}"))?;
        upsert_session(
            conn,
            &merged.session,
            &agent_metadata,
            modified_nanos(&representative_metadata),
            representative_metadata.len() as i64,
        )?;
        update_session_files(conn, &session_id, &source_files, issues.is_empty())?;
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
    let mut rows = stmt
        .query_map(
            params![search, pattern, i64::from(page_size), offset],
            row_from_sql,
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for row in &mut rows {
        let paths = load_session_files(conn, &row.source, &row.session_id)?;
        if !paths.is_empty() {
            row.source_files = paths
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();
        }
    }

    Ok(ConversationPage { rows, total })
}

pub fn load_detail(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
) -> Result<ConversationDetailDto, String> {
    let prepared = prepare_detail(conn, source, session_id)?;
    load_prepared_detail(home, prepared)
}

pub(crate) fn prepare_detail(
    conn: &Connection,
    source: &str,
    session_id: &str,
) -> Result<PreparedConversationDetail, String> {
    let source = Source::parse(source).filter(|source| *source == Source::Codex);
    let Some(source) = source else {
        return Err("当前仅支持读取 Codex 对话详情".to_string());
    };
    let session = load_session(conn, source.as_str(), session_id)?
        .ok_or_else(|| "未找到该对话记录".to_string())?;
    let usage_records = load_usage_records(conn, source, session_id)?;
    let agent_relations = load_agent_relations(conn, session_id, &[])?;
    Ok(PreparedConversationDetail {
        source,
        session,
        usage_records,
        agent_relations,
    })
}

pub(crate) fn load_prepared_detail(
    home: &Path,
    prepared: PreparedConversationDetail,
) -> Result<ConversationDetailDto, String> {
    let PreparedConversationDetail {
        source,
        mut session,
        usage_records,
        agent_relations,
    } = prepared;
    let paths = trusted_paths_for_session(home, source, &session)?;
    let (parsed, revision) = parse_codex_files_with_revision(&paths)?;
    ensure_matching_session(&parsed, &session)?;
    session.file_available = true;
    session.source_files = parsed.session.source_files.clone();
    Ok(ConversationDetailDto {
        revision,
        session,
        messages: parsed.messages,
        events: parsed.events,
        usage_records,
        agent_relations,
    })
}

pub fn detail_state(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
    known_revision: &str,
) -> Result<ConversationDetailStateDto, String> {
    let source = Source::parse(source).filter(|source| *source == Source::Codex);
    let Some(source) = source else {
        return Err("当前仅支持读取 Codex 对话详情".to_string());
    };
    let session = load_session(conn, source.as_str(), session_id)?
        .ok_or_else(|| "未找到该对话记录".to_string())?;
    let roots = ingest::source_scan_dirs(home, source);
    let representative = PathBuf::from(&session.source_file);
    let Some(_) = detail_file_revision(&representative, &roots)? else {
        return Ok(ConversationDetailStateDto {
            revision: known_revision.to_string(),
            changed: false,
            file_available: false,
        });
    };
    let paths = session_source_paths(&session)?;
    let Some(revision) = detail_files_revision(&paths, &roots)? else {
        return Ok(ConversationDetailStateDto {
            revision: known_revision.to_string(),
            changed: false,
            file_available: false,
        });
    };
    Ok(ConversationDetailStateDto {
        changed: revision != known_revision,
        revision,
        file_available: true,
    })
}

fn parse_codex_files_with_revision(
    paths: &[PathBuf],
) -> Result<(ParsedCodexConversation, String), String> {
    read_consistent_snapshot(
        || files_revision(paths),
        || parse_codex_files_mode(paths, true, false).map_err(|issue| issue.message),
    )
}

pub(crate) fn read_consistent_snapshot<T>(
    mut revision: impl FnMut() -> Result<String, String>,
    mut read: impl FnMut() -> Result<T, String>,
) -> Result<(T, String), String> {
    for _ in 0..DETAIL_READ_ATTEMPTS {
        let before_revision = revision()?;
        let snapshot = read();
        let after_revision = revision()?;
        if after_revision != before_revision {
            continue;
        }
        return snapshot.map(|snapshot| (snapshot, after_revision));
    }
    Err("原始文件在读取期间持续变化，请重试".to_string())
}

pub fn load_event_content(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
    event_id: &str,
) -> Result<ConversationEventContentDto, String> {
    let (_, session, paths) = load_trusted_session_files(conn, home, source, session_id)?;
    let parsed = parse_codex_files(&paths, true)?;
    ensure_matching_session(&parsed, &session)?;
    let event = parsed
        .events
        .into_iter()
        .find(|event| event.event_id == event_id)
        .ok_or_else(|| "原始文件中未找到该事件".to_string())?;
    Ok(ConversationEventContentDto {
        event_id: event.event_id,
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
    let (_, session, paths) = load_trusted_session_files(conn, home, source, session_id)?;
    let parsed = parse_codex_files(&paths, true)?;
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
    let attachment_index = event
        .attachments
        .iter()
        .position(|attachment| attachment.id == attachment_id)
        .ok_or_else(|| "原始文件中未找到该附件".to_string())?;
    let attachment = event.attachments[attachment_index].clone();
    if attachment.kind != AttachmentKind::Image {
        return Err("该附件不是可预览的图片".to_string());
    }
    let source_path = PathBuf::from(&event.source_file);
    let source_fragment = parse_codex_file_for_detail(&source_path, true)?;
    let payload = read_source_payload(&source_path, event.source_sequence)?;
    let mut candidate = attachment_candidates(
        event.source_sequence,
        &payload,
        &source_fragment.session.project,
    )
    .into_iter()
    .nth(attachment_index)
    .ok_or_else(|| "原始文件中未找到该附件".to_string())?;
    candidate.attachment = attachment;
    ensure_attachment_path_allowed(&candidate, &source_fragment.session.project)?;
    Ok(candidate)
}

pub fn build_export(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
    format: ConversationExportFormat,
) -> Result<ConversationExportDto, String> {
    let (_, session, paths) = load_trusted_session_files(conn, home, source, session_id)?;
    let parsed = parse_codex_files_mode(&paths, false, true).map_err(|issue| issue.message)?;
    ensure_matching_session(&parsed, &session)?;
    let base_name = safe_export_name(&parsed.session.title, &session.session_id);
    match format {
        ConversationExportFormat::Json if paths.len() > 1 => {
            Err("该会话包含多个原始文件，无法导出为单一原始 JSONL".to_string())
        }
        ConversationExportFormat::Json => Ok(ConversationExportDto {
            default_name: format!("{base_name}.jsonl"),
            content: fs::read(&paths[0]).map_err(|error| format!("读取原始文件失败：{error}"))?,
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

fn parse_codex_file(path: &Path) -> Result<ParsedCodexConversation, ConversationIndexIssue> {
    parse_codex_file_mode(path, false, false)
}

fn parse_codex_file_for_detail(
    path: &Path,
    include_deferred_content: bool,
) -> Result<ParsedCodexConversation, String> {
    parse_codex_file_mode(path, true, include_deferred_content).map_err(|issue| issue.message)
}

fn parse_codex_file_mode(
    path: &Path,
    tolerate_incomplete_tail: bool,
    include_deferred_content: bool,
) -> Result<ParsedCodexConversation, ConversationIndexIssue> {
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
    let last_line_index = content.lines().count().saturating_sub(1);
    let has_unterminated_tail = !content.ends_with('\n');

    for (index, raw) in content.lines().enumerate() {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let value: Value = match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(error)
                if tolerate_incomplete_tail
                    && has_unterminated_tail
                    && index == last_line_index
                    && error.classify() == serde_json::error::Category::Eof =>
            {
                break;
            }
            Err(error) => {
                return Err(ConversationIndexIssue {
                    path: path.to_string_lossy().to_string(),
                    message: format!("JSON 无效：{error}"),
                    event_type: Some("json_line".to_string()),
                    line: Some((index + 1) as u64),
                });
            }
        };
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
    let source_file = path.to_string_lossy().to_string();
    for event in &mut events {
        event.source_file = source_file.clone();
        event.event_id = event_id_for(&source_file, event.source_sequence);
        for (index, attachment) in event.attachments.iter_mut().enumerate() {
            attachment.id = format!("{}:{index}", event.event_id);
        }
    }
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
        source_files: vec![path.to_string_lossy().to_string()],
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

fn parse_codex_files(
    paths: &[PathBuf],
    include_deferred_content: bool,
) -> Result<ParsedCodexConversation, String> {
    parse_codex_files_mode(paths, true, include_deferred_content).map_err(|issue| issue.message)
}

fn parse_codex_files_mode(
    paths: &[PathBuf],
    tolerate_incomplete_tail: bool,
    include_deferred_content: bool,
) -> Result<ParsedCodexConversation, ConversationIndexIssue> {
    let parsed = paths
        .iter()
        .map(|path| parse_codex_file_mode(path, tolerate_incomplete_tail, include_deferred_content))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(merge_parsed_conversations(parsed))
}

fn merge_parsed_conversations(
    mut parsed_files: Vec<ParsedCodexConversation>,
) -> ParsedCodexConversation {
    parsed_files.sort_by(|left, right| left.session.source_file.cmp(&right.session.source_file));
    let mut session = parsed_files[0].session.clone();
    session.started_at = parsed_files
        .iter()
        .map(|parsed| parsed.session.started_at.as_str())
        .filter(|value| !value.is_empty())
        .min_by(|left, right| compare_timestamps(left, right))
        .unwrap_or("")
        .to_string();
    session.ended_at = parsed_files
        .iter()
        .map(|parsed| parsed.session.ended_at.as_str())
        .filter(|value| !value.is_empty())
        .max_by(|left, right| compare_timestamps(left, right))
        .unwrap_or("")
        .to_string();
    if let Some(latest_model) = parsed_files
        .iter()
        .filter(|parsed| !parsed.session.model.is_empty())
        .max_by(|left, right| compare_timestamps(&left.session.ended_at, &right.session.ended_at))
    {
        session.model = latest_model.session.model.clone();
    }
    session.source_files = parsed_files
        .iter()
        .map(|parsed| parsed.session.source_file.clone())
        .collect();
    let capability_set = parsed_files
        .iter()
        .flat_map(|parsed| parsed.session.capabilities.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut capabilities = [CAPABILITY_MESSAGES, CAPABILITY_EVENTS, CAPABILITY_USAGE]
        .into_iter()
        .filter(|capability| capability_set.contains(*capability))
        .map(str::to_string)
        .collect::<Vec<_>>();
    capabilities.extend(capability_set.into_iter().filter(|capability| {
        !matches!(
            capability.as_str(),
            CAPABILITY_MESSAGES | CAPABILITY_EVENTS | CAPABILITY_USAGE
        )
    }));
    session.capabilities = capabilities;

    let mut messages = parsed_files
        .iter()
        .flat_map(|parsed| parsed.messages.iter().cloned())
        .collect::<Vec<_>>();
    messages.sort_by(|left, right| {
        compare_timestamps(&left.occurred_at, &right.occurred_at)
            .then_with(|| left.role.cmp(&right.role))
            .then_with(|| left.text.cmp(&right.text))
    });
    let mut seen_messages = BTreeSet::new();
    messages.retain(|message| {
        seen_messages.insert((
            message.occurred_at.clone(),
            message.role.clone(),
            message.text.clone(),
        ))
    });

    let mut sourced_events = Vec::new();
    for parsed in parsed_files {
        let source_file = parsed.session.source_file;
        let mut occurrences = BTreeMap::<String, u32>::new();
        for event in parsed.events {
            let identity = event_identity(&event);
            let occurrence = occurrences.entry(identity.clone()).or_default();
            let dedupe_key = format!("{identity}\u{1f}{}", *occurrence);
            *occurrence += 1;
            sourced_events.push((source_file.clone(), dedupe_key, event));
        }
    }
    let mut seen_events = BTreeSet::new();
    sourced_events.retain(|(_, dedupe_key, _)| seen_events.insert(dedupe_key.clone()));
    sourced_events.sort_by(|(left_path, _, left), (right_path, _, right)| {
        compare_event_timestamps(left, right)
            .then_with(|| left_path.cmp(right_path))
            .then_with(|| left.source_sequence.cmp(&right.source_sequence))
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
    let events = sourced_events
        .into_iter()
        .enumerate()
        .map(|(sequence, (_, _, mut event))| {
            event.sequence = sequence as u32;
            event
        })
        .collect();

    ParsedCodexConversation {
        session,
        messages,
        events,
    }
}

fn extract_agent_metadata(events: &[ConversationEvent]) -> IndexedAgentMetadata {
    let mut parent_session_ids = BTreeSet::new();
    let mut spawn_calls = BTreeMap::new();
    let mut spawn_results = BTreeMap::new();

    for event in events {
        if event.kind == EventKind::SystemStatus && event.name.as_deref() == Some("session_started")
        {
            for candidate in [
                event.details.get("parent_id"),
                event.details.get("parent_session_id"),
                event.details.pointer("/source/subagent/parent_id"),
                event.details.pointer("/source/subagent/parent_session_id"),
            ]
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|value| !value.is_empty())
            {
                parent_session_ids.insert(candidate.to_string());
            }
        }
        if let Some(call_id) = event.details.get("call_id").and_then(Value::as_str) {
            if event.kind == EventKind::ToolCall && event.name.as_deref() == Some("spawn_agent") {
                spawn_calls.insert(call_id.to_string(), event.event_id.clone());
            } else if event.kind == EventKind::ToolResult {
                spawn_results.insert(call_id.to_string(), &event.details);
            }
        }
    }

    let spawn_attempts = spawn_calls
        .into_iter()
        .map(|(call_id, launch_event_id)| IndexedSpawnAttempt {
            launch_event_id,
            child_session_id: spawn_results
                .get(&call_id)
                .and_then(|details| structured_agent_id(details)),
        })
        .collect();
    IndexedAgentMetadata {
        parent_session_ids: parent_session_ids.into_iter().collect(),
        spawn_attempts,
    }
}

fn structured_agent_id(value: &Value) -> Option<String> {
    if let Some(agent_id) = value
        .as_object()
        .and_then(|object| object.get("agent_id"))
        .and_then(Value::as_str)
        .filter(|agent_id| !agent_id.is_empty())
    {
        return Some(agent_id.to_string());
    }
    for key in ["output", "result"] {
        let Some(candidate) = value.get(key) else {
            continue;
        };
        if let Some(agent_id) = structured_agent_id(candidate) {
            return Some(agent_id);
        }
        if let Some(text) = candidate.as_str() {
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                if let Some(agent_id) = structured_agent_id(&parsed) {
                    return Some(agent_id);
                }
            }
        }
    }
    None
}

fn event_id_for(source_file: &str, source_sequence: u32) -> String {
    format!(
        "{}:{source_sequence}",
        BASE64_URL_SAFE_NO_PAD.encode(source_file.as_bytes())
    )
}

fn event_identity(event: &ConversationEvent) -> String {
    let mut normalized = event.clone();
    normalized.event_id.clear();
    normalized.sequence = 0;
    normalized.source_file.clear();
    normalized.source_sequence = 0;
    for (index, attachment) in normalized.attachments.iter_mut().enumerate() {
        attachment.id = index.to_string();
    }
    serde_json::to_string(&normalized).unwrap_or_default()
}

fn compare_event_timestamps(
    left: &ConversationEvent,
    right: &ConversationEvent,
) -> std::cmp::Ordering {
    match (&left.occurred_at, &right.occurred_at) {
        (Some(left_time), Some(right_time)) => compare_timestamps(left_time, right_time),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
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
        event_id: String::new(),
        sequence: sequence as u32,
        source_file: String::new(),
        source_sequence: sequence as u32,
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

fn upsert_session(
    conn: &Connection,
    session: &ConversationSessionRow,
    agent_metadata: &IndexedAgentMetadata,
    source_file_mtime_ns: i64,
    source_file_size: i64,
) -> Result<(), String> {
    let capabilities = serde_json::to_string(&session.capabilities).map_err(|e| e.to_string())?;
    let agent_metadata = serde_json::to_string(agent_metadata).map_err(|e| e.to_string())?;
    conn.execute(
        r#"
        INSERT INTO conversation_sessions(
            source, session_id, title, project, model, started_at, ended_at,
            source_file, capabilities_json, support_status, file_available,
            source_file_mtime_ns, source_file_size, agent_metadata_json
        ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
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
            source_file_mtime_ns = excluded.source_file_mtime_ns,
            source_file_size = excluded.source_file_size,
            agent_metadata_json = excluded.agent_metadata_json
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
            source_file_mtime_ns,
            source_file_size,
            agent_metadata,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn failed_session_paths(
    conn: &Connection,
    issues: &[ConversationIndexIssue],
) -> Result<BTreeMap<String, BTreeSet<PathBuf>>, String> {
    let mut paths_by_session = BTreeMap::new();
    let mut statement = conn
        .prepare(
            r#"
            SELECT session_id FROM conversation_session_files
            WHERE source = ?1 AND source_file = ?2
            "#,
        )
        .map_err(|error| error.to_string())?;
    for issue in issues {
        if let Some(session_id) = statement
            .query_row(params![Source::Codex.as_str(), issue.path], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|error| error.to_string())?
        {
            paths_by_session
                .entry(session_id)
                .or_insert_with(BTreeSet::new)
                .insert(PathBuf::from(&issue.path));
        }
    }
    Ok(paths_by_session)
}

fn update_session_files(
    conn: &Connection,
    session_id: &str,
    paths: &[PathBuf],
    replace: bool,
) -> Result<(), String> {
    if replace {
        conn.execute(
            "DELETE FROM conversation_session_files WHERE source = ?1 AND session_id = ?2",
            params![Source::Codex.as_str(), session_id],
        )
        .map_err(|error| error.to_string())?;
    }
    for path in paths {
        let metadata =
            fs::metadata(path).map_err(|error| format!("读取文件元数据失败：{error}"))?;
        conn.execute(
            r#"
            INSERT INTO conversation_session_files(
                source, session_id, source_file, source_file_mtime_ns, source_file_size
            ) VALUES(?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(source, source_file) DO UPDATE SET
                session_id = excluded.session_id,
                source_file_mtime_ns = excluded.source_file_mtime_ns,
                source_file_size = excluded.source_file_size
            "#,
            params![
                Source::Codex.as_str(),
                session_id,
                path.to_string_lossy().to_string(),
                modified_nanos(&metadata),
                metadata.len() as i64,
            ],
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn load_agent_relations(
    conn: &Connection,
    current_session_id: &str,
    current_events: &[ConversationEvent],
) -> Result<ConversationAgentRelations, String> {
    let mut catalog = load_agent_catalog(conn)?;
    if !current_events.is_empty() {
        catalog
            .entry(current_session_id.to_string())
            .and_modify(|(_, metadata)| *metadata = extract_agent_metadata(current_events));
    }

    let mut parent_claims = BTreeMap::<String, BTreeSet<String>>::new();
    for (session_id, (_, metadata)) in &catalog {
        for parent_id in &metadata.parent_session_ids {
            parent_claims
                .entry(session_id.clone())
                .or_default()
                .insert(parent_id.clone());
        }
        for attempt in &metadata.spawn_attempts {
            if let Some(child_id) = &attempt.child_session_id {
                parent_claims
                    .entry(child_id.clone())
                    .or_default()
                    .insert(session_id.clone());
            }
        }
    }

    let current_metadata = &catalog
        .get(current_session_id)
        .ok_or_else(|| "未找到该对话记录".to_string())?
        .1;
    let mut child_launches = BTreeMap::<String, Option<String>>::new();
    for attempt in &current_metadata.spawn_attempts {
        if let Some(child_id) = &attempt.child_session_id {
            child_launches
                .entry(child_id.clone())
                .or_insert_with(|| Some(attempt.launch_event_id.clone()));
        }
    }
    for (child_id, parents) in &parent_claims {
        if parents.contains(current_session_id) {
            child_launches.entry(child_id.clone()).or_insert(None);
        }
    }

    let mut children = child_launches
        .into_iter()
        .map(|(child_id, launch_event_id)| {
            let status = agent_link_status(current_session_id, &child_id, &catalog, &parent_claims);
            let session = (status == AgentLinkStatus::Linked)
                .then(|| catalog.get(&child_id).map(|(session, _)| session.clone()))
                .flatten();
            ConversationAgentLink {
                relationship_id: launch_event_id
                    .clone()
                    .unwrap_or_else(|| format!("metadata:{current_session_id}:{child_id}")),
                session_id: Some(child_id),
                launch_event_id,
                status,
                session,
            }
        })
        .collect::<Vec<_>>();
    children.extend(
        current_metadata
            .spawn_attempts
            .iter()
            .filter(|attempt| attempt.child_session_id.is_none())
            .map(|attempt| ConversationAgentLink {
                relationship_id: attempt.launch_event_id.clone(),
                session_id: None,
                launch_event_id: Some(attempt.launch_event_id.clone()),
                status: AgentLinkStatus::Unresolved,
                session: None,
            }),
    );
    children.sort_by(|left, right| left.relationship_id.cmp(&right.relationship_id));

    let parent = build_parent_link(current_session_id, &catalog, &parent_claims);
    let statuses = parent
        .iter()
        .map(|link| link.status)
        .chain(children.iter().map(|link| link.status))
        .collect::<Vec<_>>();
    let has_linked = statuses.contains(&AgentLinkStatus::Linked);
    let has_unavailable = statuses
        .iter()
        .any(|status| *status != AgentLinkStatus::Linked);
    let capability_status = match (has_linked, has_unavailable) {
        (true, true) => AgentCapabilityStatus::Partial,
        (false, true) => AgentCapabilityStatus::Unavailable,
        _ => AgentCapabilityStatus::Complete,
    };

    Ok(ConversationAgentRelations {
        capability_status,
        parent,
        children,
    })
}

fn load_agent_catalog(
    conn: &Connection,
) -> Result<BTreeMap<String, (ConversationSessionRow, IndexedAgentMetadata)>, String> {
    let indexed = {
        let mut statement = conn
            .prepare(
                "SELECT session_id, agent_metadata_json FROM conversation_sessions WHERE source = ?1",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![Source::Codex.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    let mut catalog = BTreeMap::new();
    for (session_id, metadata_json) in indexed {
        let Some(session) = load_session(conn, Source::Codex.as_str(), &session_id)? else {
            continue;
        };
        let metadata = serde_json::from_str(&metadata_json).unwrap_or_default();
        catalog.insert(session_id, (session, metadata));
    }
    Ok(catalog)
}

fn build_parent_link(
    child_id: &str,
    catalog: &BTreeMap<String, (ConversationSessionRow, IndexedAgentMetadata)>,
    parent_claims: &BTreeMap<String, BTreeSet<String>>,
) -> Option<ConversationAgentLink> {
    let claims = parent_claims.get(child_id)?;
    if claims.len() != 1 {
        return Some(ConversationAgentLink {
            relationship_id: format!("conflict:{child_id}"),
            session_id: None,
            launch_event_id: None,
            status: AgentLinkStatus::Conflict,
            session: None,
        });
    }
    let parent_id = claims.iter().next()?.clone();
    let launch_event_id = catalog.get(&parent_id).and_then(|(_, metadata)| {
        metadata
            .spawn_attempts
            .iter()
            .find(|attempt| attempt.child_session_id.as_deref() == Some(child_id))
            .map(|attempt| attempt.launch_event_id.clone())
    });
    let status = agent_link_status(&parent_id, child_id, catalog, parent_claims);
    let session = (status == AgentLinkStatus::Linked)
        .then(|| catalog.get(&parent_id).map(|(session, _)| session.clone()))
        .flatten();
    Some(ConversationAgentLink {
        relationship_id: launch_event_id
            .clone()
            .unwrap_or_else(|| format!("metadata:{parent_id}:{child_id}")),
        session_id: Some(parent_id),
        launch_event_id,
        status,
        session,
    })
}

fn agent_link_status(
    parent_id: &str,
    child_id: &str,
    catalog: &BTreeMap<String, (ConversationSessionRow, IndexedAgentMetadata)>,
    parent_claims: &BTreeMap<String, BTreeSet<String>>,
) -> AgentLinkStatus {
    if !catalog.contains_key(parent_id) || !catalog.contains_key(child_id) {
        return AgentLinkStatus::MissingSource;
    }
    if parent_claims
        .get(child_id)
        .is_some_and(|claims| claims.len() > 1)
    {
        return AgentLinkStatus::Conflict;
    }
    if parent_id == child_id || agent_path_exists(child_id, parent_id, parent_claims) {
        return AgentLinkStatus::Cycle;
    }
    AgentLinkStatus::Linked
}

fn agent_path_exists(
    from: &str,
    target: &str,
    parent_claims: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    let mut pending = vec![from.to_string()];
    let mut visited = BTreeSet::new();
    while let Some(parent) = pending.pop() {
        if !visited.insert(parent.clone()) {
            continue;
        }
        for (child, parents) in parent_claims {
            if parents.contains(&parent) {
                if child == target {
                    return true;
                }
                pending.push(child.clone());
            }
        }
    }
    false
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
            ORDER BY occurred_at ASC, source_file ASC, rowid ASC
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
    let mut records = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut seen = BTreeSet::new();
    records.retain(|record| seen.insert(usage_record_identity(record)));
    Ok(records)
}

fn usage_record_identity(record: &UsageRecord) -> String {
    serde_json::json!({
        "occurred_at": record.occurred_at,
        "source": record.source,
        "model": record.model,
        "provider": record.provider,
        "project": record.project,
        "session_id": record.session_id,
        "input_tokens": record.input_tokens,
        "output_tokens": record.output_tokens,
        "cache_read_tokens": record.cache_read_tokens,
        "cache_creation_tokens": record.cache_creation_tokens,
        "reasoning_tokens": record.reasoning_tokens,
        "total_tokens": record.total_tokens,
        "native_cost_bits": record.native_cost.map(f64::to_bits),
    })
    .to_string()
}

fn load_session(
    conn: &Connection,
    source: &str,
    session_id: &str,
) -> Result<Option<ConversationSessionRow>, String> {
    let mut session = conn
        .query_row(
            r#"
            SELECT source, session_id, title, project, model, started_at, ended_at,
                   source_file, capabilities_json, support_status, file_available
            FROM conversation_sessions WHERE source = ?1 AND session_id = ?2
            "#,
            params![source, session_id],
            row_from_sql,
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(session) = &mut session {
        let paths = load_session_files(conn, source, session_id)?;
        if !paths.is_empty() {
            session.source_files = paths
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();
        }
    }
    Ok(session)
}

fn load_session_files(
    conn: &Connection,
    source: &str,
    session_id: &str,
) -> Result<Vec<PathBuf>, String> {
    let mut statement = conn
        .prepare(
            r#"
            SELECT source_file FROM conversation_session_files
            WHERE source = ?1 AND session_id = ?2
            ORDER BY source_file ASC
            "#,
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![source, session_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .map(|result| result.map(PathBuf::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn load_trusted_session_files(
    conn: &Connection,
    home: &Path,
    source: &str,
    session_id: &str,
) -> Result<(Source, ConversationSessionRow, Vec<PathBuf>), String> {
    let source = Source::parse(source).filter(|source| *source == Source::Codex);
    let Some(source) = source else {
        return Err("当前仅支持读取 Codex 对话详情".to_string());
    };
    let session = load_session(conn, source.as_str(), session_id)?
        .ok_or_else(|| "未找到该对话记录".to_string())?;
    let roots = ingest::source_scan_dirs(home, source);
    let representative = PathBuf::from(&session.source_file);
    if !representative.exists() {
        return Err("原始文件已不存在，无法读取对话详情".to_string());
    }
    ensure_trusted_path(&representative, &roots)?;
    let mut paths = load_session_files(conn, source.as_str(), session_id)?;
    if !paths.is_empty() && !paths.iter().any(|path| path == &representative) {
        return Err("会话索引的代表文件与来源清单不一致".to_string());
    }
    if paths.is_empty() {
        paths.push(representative);
    }
    for path in &paths {
        if !path.exists() {
            return Err("原始文件已不存在，无法读取对话详情".to_string());
        }
        ensure_trusted_path(path, &roots)?;
    }
    Ok((source, session, paths))
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
        mark_session_unavailable(conn, &session_id)?;
    }
    Ok(())
}

fn mark_session_unavailable(conn: &Connection, session_id: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE conversation_sessions SET file_available = 0 WHERE source = ?1 AND session_id = ?2",
        params![Source::Codex.as_str(), session_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationSessionRow> {
    let capabilities_json: String = row.get(8)?;
    let source_file: String = row.get(7)?;
    Ok(ConversationSessionRow {
        source: row.get(0)?,
        session_id: row.get(1)?,
        title: row.get(2)?,
        project: row.get(3)?,
        model: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        source_file: source_file.clone(),
        source_files: vec![source_file],
        capabilities: serde_json::from_str(&capabilities_json).unwrap_or_default(),
        support_status: row.get(9)?,
        file_available: row.get(10)?,
    })
}

fn load_cached_fingerprints(
    conn: &Connection,
    path: &Path,
) -> Result<Vec<CachedConversationFingerprint>, String> {
    let mut stmt = conn
        .prepare(
            r#"
        SELECT DISTINCT session_id, source_file_mtime_ns, source_file_size
        FROM (
            SELECT files.session_id, files.source_file_mtime_ns, files.source_file_size
            FROM conversation_session_files AS files
            JOIN conversation_sessions AS sessions
              ON sessions.source = files.source AND sessions.session_id = files.session_id
            WHERE files.source = ?1 AND files.source_file = ?2 AND sessions.file_available = 1
            UNION ALL
            SELECT session_id, source_file_mtime_ns, source_file_size
            FROM conversation_sessions
            WHERE source = ?1 AND source_file = ?2 AND file_available = 1
        )
        LIMIT 2
        "#,
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(
            params![Source::Codex.as_str(), path.to_string_lossy().to_string()],
            |row| {
                Ok(CachedConversationFingerprint {
                    session_id: row.get(0)?,
                    source_file_mtime_ns: row.get(1)?,
                    source_file_size: row.get(2)?,
                })
            },
        )
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn modified_nanos(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0)
}

fn metadata_revision(metadata: &fs::Metadata) -> String {
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{modified_ns}:{}", metadata.len())
}

fn ensure_trusted_path(path: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let canonical_path =
        fs::canonicalize(path).map_err(|error| format!("无法验证原始文件路径：{error}"))?;
    ensure_canonical_path_in_roots(&canonical_path, roots)
}

fn ensure_canonical_path_in_roots(canonical_path: &Path, roots: &[PathBuf]) -> Result<(), String> {
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

fn session_source_paths(session: &ConversationSessionRow) -> Result<Vec<PathBuf>, String> {
    let representative = PathBuf::from(&session.source_file);
    let paths = if session.source_files.is_empty() {
        vec![representative.clone()]
    } else {
        session.source_files.iter().map(PathBuf::from).collect()
    };
    if !paths.iter().any(|path| path == &representative) {
        return Err("会话索引的代表文件与来源清单不一致".to_string());
    }
    Ok(paths)
}

fn trusted_paths_for_session(
    home: &Path,
    source: Source,
    session: &ConversationSessionRow,
) -> Result<Vec<PathBuf>, String> {
    let roots = ingest::source_scan_dirs(home, source);
    let representative = PathBuf::from(&session.source_file);
    if !representative.exists() {
        return Err("原文件已删除，详情不可读取".to_string());
    }
    ensure_trusted_path(&representative, &roots)?;
    let paths = session_source_paths(session)?;
    for path in &paths {
        if !path.exists() {
            return Err("原文件已删除，详情不可读取".to_string());
        }
        ensure_trusted_path(path, &roots)?;
    }
    Ok(paths)
}

fn files_revision(paths: &[PathBuf]) -> Result<String, String> {
    let revisions = paths
        .iter()
        .map(|path| {
            fs::metadata(path)
                .map(|metadata| {
                    (
                        path.to_string_lossy().to_string(),
                        metadata_revision(&metadata),
                    )
                })
                .map_err(|error| format!("读取原始文件元数据失败：{error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let [(_, revision)] = revisions.as_slice() {
        return Ok(revision.clone());
    }
    serde_json::to_string(&revisions).map_err(|error| error.to_string())
}

fn detail_files_revision(paths: &[PathBuf], roots: &[PathBuf]) -> Result<Option<String>, String> {
    let mut revisions = Vec::with_capacity(paths.len());
    for path in paths {
        let Some(revision) = detail_file_revision(path, roots)? else {
            return Ok(None);
        };
        revisions.push((path.to_string_lossy().to_string(), revision));
    }
    if let [(_, revision)] = revisions.as_slice() {
        return Ok(Some(revision.clone()));
    }
    serde_json::to_string(&revisions)
        .map(Some)
        .map_err(|error| error.to_string())
}

fn detail_file_revision(path: &Path, roots: &[PathBuf]) -> Result<Option<String>, String> {
    checked_detail_file_revision(
        roots,
        || fs::canonicalize(path),
        |canonical_path| fs::metadata(canonical_path).map(|metadata| metadata_revision(&metadata)),
    )
}

pub(crate) fn checked_detail_file_revision(
    roots: &[PathBuf],
    canonicalize_file: impl FnOnce() -> std::io::Result<PathBuf>,
    read_revision: impl FnOnce(&Path) -> std::io::Result<String>,
) -> Result<Option<String>, String> {
    let canonical_path = match canonicalize_file() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法验证原始文件路径：{error}")),
    };
    ensure_canonical_path_in_roots(&canonical_path, roots)?;
    match read_revision(&canonical_path) {
        Ok(revision) => Ok(Some(revision)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("读取原始文件元数据失败：{error}")),
    }
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}
