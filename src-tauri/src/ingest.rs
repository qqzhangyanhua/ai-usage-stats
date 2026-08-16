use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::Connection;

use crate::adapters::cursor::{parse_cursor_commits, summarize_code_volume, CursorCommitRow};
use crate::adapters::opencode::{parse_opencode_messages, OpencodeMessage};
use crate::adapters::{
    claude, codex, dsh, factory, gemini, grok, kimi, pi, qwen,
};
use crate::domain::{CodeVolumeSummary, IngestReport, UsageRecord};
use crate::store;

pub fn default_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn ingest_all(conn: &Connection, home: &Path) -> Result<IngestReport, String> {
    let mut report = IngestReport {
        files_seen: 0,
        files_parsed: 0,
        files_skipped: 0,
        records_written: 0,
    };
    ingest_jsonl_tree(
        conn,
        &home.join(".codex/sessions"),
        "jsonl",
        &mut report,
        |content, path| Ok(codex::parse_codex_jsonl(content, path)),
    )?;
    ingest_jsonl_tree(
        conn,
        &home.join(".claude/projects"),
        "jsonl",
        &mut report,
        |content, path| Ok(claude::parse_claude_jsonl(content, path)),
    )?;
    ingest_jsonl_tree(
        conn,
        &home.join(".pi/agent/sessions"),
        "jsonl",
        &mut report,
        |content, path| Ok(pi::parse_pi_jsonl(content, path)),
    )?;
    ingest_kimi(conn, &home.join(".kimi"), &mut report)?;
    ingest_dsh(conn, &home.join(".dsh/sessions"), &mut report)?;
    ingest_gemini(conn, &home.join(".gemini/tmp"), &mut report)?;
    ingest_grok(conn, &home.join(".grok/sessions"), &mut report)?;
    ingest_qwen(conn, &home.join(".qwen/tmp"), &mut report)?;
    ingest_factory(conn, &home.join(".factory/sessions"), &mut report)?;
    ingest_opencode(conn, &home.join(".local/share/opencode/opencode.db"), &mut report)?;
    Ok(report)
}

fn ingest_jsonl_tree(
    conn: &Connection,
    root: &Path,
    ext: &str,
    report: &mut IngestReport,
    parse: impl Fn(&str, &str) -> Result<Vec<UsageRecord>, String>,
) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for path in walk_files(root, ext) {
        ingest_one(conn, &path, report, |bytes, loc| {
            let content = String::from_utf8_lossy(bytes).into_owned();
            parse(&content, loc)
        })?;
    }
    Ok(())
}

fn ingest_one(
    conn: &Connection,
    path: &Path,
    report: &mut IngestReport,
    parse: impl Fn(&[u8], &str) -> Result<Vec<UsageRecord>, String>,
) -> Result<(), String> {
    report.files_seen += 1;
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let size = meta.len() as i64;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let loc = path.to_string_lossy().to_string();
    if store::file_unchanged(conn, &loc, mtime_ms, size)? {
        report.files_skipped += 1;
        return Ok(());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let records = parse(&bytes, &loc)?;
    store::delete_records_for_file(conn, &loc)?;
    report.records_written += store::insert_records(conn, &records)?;
    store::mark_file(conn, &loc, mtime_ms, size)?;
    report.files_parsed += 1;
    Ok(())
}

fn ingest_kimi(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    let projects = kimi_projects(root);
    let sessions = root.join("sessions");
    if !sessions.exists() {
        return Ok(());
    }
    for path in walk_files(&sessions, "jsonl") {
        if path.file_name().and_then(|n| n.to_str()) != Some("wire.jsonl") {
            continue;
        }
        let session_id = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let project = projects
            .iter()
            .find(|(sid, _)| sid == &session_id)
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| {
                path.parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string()
            });
        ingest_one(conn, &path, report, |bytes, loc| {
            let content = String::from_utf8_lossy(bytes).into_owned();
            Ok(kimi::parse_kimi_wire(&content, loc, &project))
        })?;
    }
    Ok(())
}

fn kimi_projects(root: &Path) -> Vec<(String, String)> {
    let path = root.join("kimi.json");
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    value
        .get("work_dirs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some((
                        item.get("last_session_id")?.as_str()?.to_string(),
                        item.get("path")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ingest_dsh(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for path in walk_suffix(root, "session.jsonl.zstd") {
        ingest_one(conn, &path, report, |bytes, loc| dsh::parse_dsh_zstd(bytes, loc))?;
    }
    Ok(())
}

fn ingest_gemini(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for path in walk_files(root, "json") {
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .starts_with("session-")
        {
            continue;
        }
        ingest_one(conn, &path, report, |bytes, loc| {
            let content = String::from_utf8_lossy(bytes).into_owned();
            Ok(gemini::parse_gemini_session(&content, loc))
        })?;
    }
    Ok(())
}

fn ingest_grok(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for path in walk_files(root, "jsonl") {
        if path.file_name().and_then(|n| n.to_str()) != Some("updates.jsonl") {
            continue;
        }
        let summary = path
            .parent()
            .map(|p| p.join("summary.json"))
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        let model = summary
            .as_ref()
            .and_then(|v| v.get("current_model_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        ingest_one(conn, &path, report, |bytes, loc| {
            let content = String::from_utf8_lossy(bytes).into_owned();
            Ok(grok::parse_grok_updates(&content, loc, &model))
        })?;
    }
    Ok(())
}

fn ingest_qwen(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for path in walk_files(root, "json") {
        if path.file_name().and_then(|n| n.to_str()) != Some("logs.json") {
            continue;
        }
        ingest_one(conn, &path, report, |bytes, loc| {
            let content = String::from_utf8_lossy(bytes).into_owned();
            Ok(qwen::parse_qwen_session(&content, loc))
        })?;
    }
    Ok(())
}

fn ingest_factory(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for path in walk_suffix(root, ".settings.json") {
        ingest_one(conn, &path, report, |bytes, loc| {
            let content = String::from_utf8_lossy(bytes).into_owned();
            Ok(factory::parse_factory_settings(&content, loc))
        })?;
    }
    Ok(())
}

fn ingest_opencode(conn: &Connection, db_path: &Path, report: &mut IngestReport) -> Result<(), String> {
    if !db_path.exists() {
        return Ok(());
    }
    ingest_one(conn, db_path, report, |_, loc| {
        let src = open_readonly(db_path)?;
        let mut stmt = src
            .prepare("SELECT session_id, data FROM message")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| e.to_string())?;
        let mut messages = Vec::new();
        for row in rows {
            let (session_id, data) = row.map_err(|e| e.to_string())?;
            let data: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
            messages.push(OpencodeMessage {
                session_id,
                source_file: loc.to_string(),
                data,
            });
        }
        Ok(parse_opencode_messages(&messages))
    })
}

pub fn load_code_volume(home: &Path) -> Result<CodeVolumeSummary, String> {
    let db_path = home.join(".cursor/ai-tracking/ai-code-tracking.db");
    if !db_path.exists() {
        return Ok(CodeVolumeSummary {
            commit_count: 0,
            lines_added: 0,
            composer_lines_added: 0,
            human_lines_added: 0,
            ai_percentage: None,
        });
    }
    let src = open_readonly(&db_path)?;
    let mut stmt = src
        .prepare(
            r#"
            SELECT commitHash, branchName, scoredAt,
                   COALESCE(linesAdded, 0),
                   COALESCE(composerLinesAdded, 0),
                   COALESCE(humanLinesAdded, 0),
                   v2AiPercentage
            FROM scored_commits
            WHERE linesAdded IS NOT NULL OR v2AiPercentage IS NOT NULL
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let pct: Option<String> = row.get(6)?;
            Ok(CursorCommitRow {
                commit_hash: row.get(0)?,
                branch: row.get(1)?,
                scored_at_ms: row.get(2)?,
                lines_added: row.get(3)?,
                composer_lines_added: row.get(4)?,
                human_lines_added: row.get(5)?,
                ai_percentage: pct.and_then(|s| s.parse().ok()),
            })
        })
        .map_err(|e| e.to_string())?;
    let commits: Result<Vec<_>, _> = rows.collect();
    let parsed = parse_cursor_commits(&commits.map_err(|e| e.to_string())?);
    Ok(summarize_code_volume(&parsed))
}

fn open_readonly(path: &Path) -> Result<rusqlite::Connection, String> {
    let uri = format!("file:{}?mode=ro", path.to_string_lossy());
    rusqlite::Connection::open_with_flags(
        uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| e.to_string())
}

fn walk_files(root: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    let mut stack: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            if let Ok(entries) = fs::read_dir(&path) {
                stack.extend(entries.filter_map(|e| e.ok().map(|e| e.path())));
            }
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case(ext))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    out
}

fn walk_suffix(root: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return out;
    };
    let mut stack: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            if let Ok(entries) = fs::read_dir(&path) {
                stack.extend(entries.filter_map(|e| e.ok().map(|e| e.path())));
            }
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.ends_with(suffix))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
    out
}
