use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::domain::{
    ConversationDetailDto, ConversationMessage, ConversationPage, ConversationQuery,
    ConversationSessionRow, Source,
};
use crate::ingest;

const DEFAULT_PAGE_SIZE: u32 = 20;
const MAX_PAGE_SIZE: u32 = 200;
const TITLE_MAX_CHARS: usize = 80;
const CAPABILITY_MESSAGES: &str = "messages";
const EXPERIMENTAL: &str = "experimental";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationIndexIssue {
    pub path: String,
    pub message: String,
}

struct ParsedCodexConversation {
    session: ConversationSessionRow,
    messages: Vec<ConversationMessage>,
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
            match parse_codex_file(&path) {
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
    let parsed = parse_codex_file(&path)?;
    if parsed.session.session_id != session.session_id {
        return Err("原始文件中的会话 ID 与索引不一致".to_string());
    }
    Ok(ConversationDetailDto {
        session,
        messages: parsed.messages,
    })
}

fn parse_codex_file(path: &Path) -> Result<ParsedCodexConversation, String> {
    let content = fs::read_to_string(path).map_err(|error| format!("读取原始文件失败：{error}"))?;
    let mut session_id = String::new();
    let mut title = String::new();
    let mut project = String::new();
    let mut model = String::new();
    let mut started_at = String::new();
    let mut ended_at = String::new();
    let mut response_messages = Vec::new();
    let mut event_messages = Vec::new();

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
                session_id = first_text(payload, &["id", "session_id"]);
                project = first_text(payload, &["cwd"]);
                title = first_text(payload, &["title", "name"]);
            }
            "turn_context" => {
                let next_project = first_text(payload, &["cwd"]);
                if !next_project.is_empty() {
                    project = next_project;
                }
                let next_model = first_text(payload, &["model"]);
                if !next_model.is_empty() {
                    model = next_model;
                }
            }
            "response_item" => {
                if let Some(message) = response_message(payload, &timestamp) {
                    response_messages.push(message);
                }
            }
            "event_msg" => {
                if let Some(message) = event_message(payload, &timestamp) {
                    event_messages.push(message);
                }
            }
            _ => {}
        }
    }

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
    let capabilities = if messages.is_empty() {
        Vec::new()
    } else {
        vec![CAPABILITY_MESSAGES.to_string()]
    };
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
    Ok(ParsedCodexConversation { session, messages })
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
