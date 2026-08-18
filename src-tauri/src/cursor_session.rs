use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::Connection;

use crate::adapters::cursor_session::{
    apply_hash_enrichment, build_cursor_session_record, load_hash_enrichments,
    parse_cursor_session_transcript,
};
use crate::domain::{
    CursorSessionDailyPoint, CursorSessionListRow, CursorSessionModelRow, CursorSessionProjectRow,
    CursorSessionRecord, CursorSessionSummaryDto, CursorSessionToolRow, IngestIssue, IngestReport,
};
use crate::store;

pub const SOURCE_LABEL: &str = "cursor-session";

pub fn ingest(conn: &Connection, home: &Path, report: &mut IngestReport) {
    let root = home.join(".cursor/projects");
    if !root.exists() {
        return;
    }

    let transcripts = match walk_transcripts(&root) {
        Ok(paths) => paths,
        Err(error) => {
            record_issue(report, &root.to_string_lossy(), &error);
            return;
        }
    };

    let enrichments = match load_hash_enrichments(home) {
        Ok(map) => Some(map),
        Err(error) => {
            record_issue(report, &root.to_string_lossy(), &error);
            None
        }
    };

    let mut seen_paths = BTreeSet::new();
    let mut any_failed = false;

    for path in transcripts {
        let path_key = path.to_string_lossy().to_string();
        report.files_seen += 1;
        seen_paths.insert(path_key.clone());

        let meta = match fs::metadata(&path) {
            Ok(meta) => meta,
            Err(error) => {
                record_issue(report, &path_key, &format!("读取文件元数据失败：{error}"));
                any_failed = true;
                continue;
            }
        };
        let mtime_ms = modified_millis(&meta);
        let size = meta.len() as i64;

        if let Ok(Some((cached_mtime, cached_size))) =
            store::cursor_session_file_fingerprint(conn, &path_key)
        {
            if cached_mtime == mtime_ms && cached_size == size {
                match store::cursor_session_has_source_file(conn, &path_key) {
                    Ok(true) => {
                        report.files_skipped += 1;
                        continue;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        record_issue(report, &path_key, &error);
                        any_failed = true;
                        continue;
                    }
                }
            }
        }

        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => {
                record_issue(report, &path_key, &format!("读取 transcript 失败：{error}"));
                any_failed = true;
                continue;
            }
        };

        let parsed = match parse_cursor_session_transcript(&content) {
            Ok(parsed) => parsed,
            Err(error) => {
                record_issue(report, &path_key, &error);
                any_failed = true;
                continue;
            }
        };

        let seen_at = millis_to_rfc3339(mtime_ms);
        let mut record = match build_cursor_session_record(&path_key, &parsed, seen_at) {
            Ok(record) => record,
            Err(error) => {
                record_issue(report, &path_key, &error);
                any_failed = true;
                continue;
            }
        };
        if let Some(enrichment) = enrichments
            .as_ref()
            .and_then(|map| map.get(&record.session_id))
        {
            if let Err(error) = apply_hash_enrichment(&mut record, enrichment) {
                record_issue(report, &path_key, &error);
                any_failed = true;
                continue;
            }
        }

        if let Err(error) = store::upsert_cursor_session(conn, &record) {
            record_issue(report, &path_key, &error);
            any_failed = true;
            continue;
        }
        if let Err(error) = store::upsert_cursor_session_file(conn, &path_key, mtime_ms, size) {
            record_issue(report, &path_key, &error);
            any_failed = true;
            continue;
        }
        report.files_parsed += 1;
    }

    if !any_failed {
        match store::reconcile_cursor_sessions(conn, &seen_paths) {
            Ok(removed) => report.records_removed += removed,
            Err(error) => record_issue(report, &root.to_string_lossy(), &error),
        }
    }

    if let Some(map) = enrichments.as_ref() {
        match refresh_hash_enrichments(conn, map) {
            Ok(()) => {
                if let Err(error) =
                    store::set_cursor_tracking_fingerprint(conn, &tracking_db_fingerprint(home))
                {
                    record_issue(report, &root.to_string_lossy(), &error);
                }
            }
            Err(error) => record_issue(report, &root.to_string_lossy(), &error),
        }
    }

    let _ = store::set_cursor_session_as_of(conn, &chrono::Utc::now().to_rfc3339());
}

fn refresh_hash_enrichments(
    conn: &Connection,
    enrichments: &BTreeMap<String, crate::adapters::cursor_session::SessionHashEnrichment>,
) -> Result<(), String> {
    let sessions = store::load_cursor_sessions(conn)?;
    for mut session in sessions {
        if let Some(enrichment) = enrichments.get(&session.session_id) {
            apply_hash_enrichment(&mut session, enrichment)?;
        } else {
            session.models_json = "[]".to_string();
            session.files_touched = 0;
        }
        store::upsert_cursor_session(conn, &session)?;
    }
    Ok(())
}

pub fn load_summary(conn: &Connection) -> Result<CursorSessionSummaryDto, String> {
    let sessions = store::load_cursor_sessions(conn)?;
    let mut summary = summarize_cursor_sessions(&sessions);
    summary.as_of = store::cursor_session_as_of(conn)?;
    Ok(summary)
}

#[derive(Default)]
struct ProjectAgg {
    session_count: i64,
    turn_count: i64,
    error_count: i64,
    files_touched: i64,
    last_seen_at: Option<String>,
}

pub fn summarize_cursor_sessions(sessions: &[CursorSessionRecord]) -> CursorSessionSummaryDto {
    if sessions.is_empty() {
        return CursorSessionSummaryDto::empty();
    }

    let session_count = sessions.len() as i64;
    let turn_count: i64 = sessions.iter().map(|session| session.turn_count).sum();
    let error_count: i64 = sessions.iter().map(|session| session.error_count).sum();
    let error_rate = if turn_count > 0 {
        Some(error_count as f64 / turn_count as f64)
    } else {
        None
    };

    let mut projects: BTreeMap<String, ProjectAgg> = BTreeMap::new();
    let mut daily: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut models: BTreeMap<String, i64> = BTreeMap::new();
    let mut tools: BTreeMap<String, i64> = BTreeMap::new();
    let mut list_rows: Vec<CursorSessionListRow> = Vec::with_capacity(sessions.len());

    for session in sessions {
        let project = display_project(&session.project);
        let entry = projects.entry(project.clone()).or_default();
        entry.session_count += 1;
        entry.turn_count += session.turn_count;
        entry.error_count += session.error_count;
        entry.files_touched += session.files_touched;
        entry.last_seen_at = later_ts(&entry.last_seen_at, &session.last_seen_at);

        if let Some(day) = session
            .last_seen_at
            .as_deref()
            .map(local_day)
            .filter(|day| !day.is_empty())
        {
            let bucket = daily.entry(day).or_insert((0, 0));
            bucket.0 += 1;
            bucket.1 += session.turn_count;
        }

        let session_models = parse_models(&session.models_json);
        for name in &session_models {
            if name.is_empty() {
                continue;
            }
            *models.entry(name.clone()).or_insert(0) += 1;
        }

        let session_tools = parse_tools(&session.tool_calls_json);
        let mut tool_call_count = 0i64;
        for (name, count) in session_tools {
            *tools.entry(name).or_insert(0) += count;
            tool_call_count += count;
        }

        list_rows.push(CursorSessionListRow {
            session_id: session.session_id.clone(),
            project,
            turn_count: session.turn_count,
            success_count: session.success_count,
            error_count: session.error_count,
            aborted_count: session.aborted_count,
            models: session_models,
            tool_call_count,
            first_seen_at: session.first_seen_at.clone(),
            last_seen_at: session.last_seen_at.clone(),
            files_touched: session.files_touched,
            source_file: session.source_file.clone(),
        });
    }

    list_rows.sort_by(|a, b| {
        b.last_seen_at
            .cmp(&a.last_seen_at)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    let active_project_count = projects.len() as i64;
    let mut by_project: Vec<CursorSessionProjectRow> = projects
        .into_iter()
        .map(|(name, agg)| CursorSessionProjectRow {
            name,
            session_count: agg.session_count,
            turn_count: agg.turn_count,
            error_count: agg.error_count,
            files_touched: agg.files_touched,
            last_seen_at: agg.last_seen_at,
        })
        .collect();
    by_project.sort_by(|a, b| {
        b.session_count
            .cmp(&a.session_count)
            .then_with(|| b.turn_count.cmp(&a.turn_count))
            .then_with(|| a.name.cmp(&b.name))
    });

    let daily = daily
        .into_iter()
        .map(
            |(bucket, (session_count, turn_count))| CursorSessionDailyPoint {
                bucket,
                session_count,
                turn_count,
            },
        )
        .collect();

    let mut by_model: Vec<CursorSessionModelRow> = models
        .into_iter()
        .map(|(name, session_count)| CursorSessionModelRow {
            name,
            session_count,
        })
        .collect();
    by_model.sort_by(|a, b| {
        b.session_count
            .cmp(&a.session_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut top_tools: Vec<CursorSessionToolRow> = tools
        .into_iter()
        .map(|(name, call_count)| CursorSessionToolRow { name, call_count })
        .collect();
    top_tools.sort_by(|a, b| {
        b.call_count
            .cmp(&a.call_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    top_tools.truncate(12);

    CursorSessionSummaryDto {
        as_of: None,
        session_count,
        turn_count,
        error_rate,
        active_project_count,
        by_project,
        by_model,
        top_tools,
        daily,
        sessions: list_rows,
    }
}

fn display_project(project: &str) -> String {
    if project.is_empty() {
        "未知项目".to_string()
    } else {
        project.to_string()
    }
}

fn parse_models(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn parse_tools(raw: &str) -> BTreeMap<String, i64> {
    serde_json::from_str::<BTreeMap<String, i64>>(raw).unwrap_or_default()
}

fn later_ts(current: &Option<String>, candidate: &Option<String>) -> Option<String> {
    match (current.as_deref(), candidate.as_deref()) {
        (None, None) => None,
        (Some(value), None) => Some(value.to_string()),
        (None, Some(value)) => Some(value.to_string()),
        (Some(left), Some(right)) => {
            let pick_right = match (
                chrono::DateTime::parse_from_rfc3339(left).ok(),
                chrono::DateTime::parse_from_rfc3339(right).ok(),
            ) {
                (Some(left_dt), Some(right_dt)) => right_dt > left_dt,
                _ => right > left,
            };
            Some(if pick_right { right } else { left }.to_string())
        }
    }
}

/// 托盘心跳用：transcript 指纹或代码量 sqlite 变化时视为 stale。
pub(crate) fn scan_is_stale(conn: &Connection, home: &Path) -> Result<bool, String> {
    let root = home.join(".cursor/projects");
    let transcripts = if root.exists() {
        walk_transcripts(&root)?
    } else {
        Vec::new()
    };
    let cached = store::cached_cursor_session_file_stats(conn)?;
    if transcripts.is_empty() && cached.is_empty() {
        return Ok(false);
    }
    let seen: BTreeSet<String> = transcripts
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    if cached.len() != seen.len() || cached.iter().any(|(path, _, _)| !seen.contains(path)) {
        return Ok(true);
    }
    for path in transcripts {
        let loc = path.to_string_lossy().to_string();
        let meta = match fs::metadata(&path) {
            Ok(meta) => meta,
            Err(_) => return Ok(true),
        };
        match store::cursor_session_file_fingerprint(conn, &loc)? {
            Some((mtime, size)) if mtime == modified_millis(&meta) && size == meta.len() as i64 => {
            }
            _ => return Ok(true),
        }
    }

    let current = tracking_db_fingerprint(home);
    let stored = store::cursor_tracking_fingerprint(conn)?;
    Ok(current != stored)
}

fn tracking_db_fingerprint(home: &Path) -> String {
    let path = home.join(".cursor/ai-tracking/ai-code-tracking.db");
    match fs::metadata(&path) {
        Ok(meta) => format!("{}|{}", modified_millis(&meta), meta.len()),
        Err(_) => String::new(),
    }
}

fn walk_transcripts(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    walk_transcripts_inner(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_transcripts_inner(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|e| format!("扫描 Cursor 会话目录 {} 失败：{e}", dir.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            walk_transcripts_inner(&path, files)?;
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !name.ends_with(".jsonl") {
            continue;
        }
        if path
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .any(|part| part == "agent-transcripts")
        {
            files.push(path);
        }
    }
    Ok(())
}

fn record_issue(report: &mut IngestReport, path: &str, message: &str) {
    report.files_failed += 1;
    report.partial_success = true;
    report.issues.push(IngestIssue {
        source: SOURCE_LABEL.to_string(),
        path: path.to_string(),
        message: message.to_string(),
    });
}

fn modified_millis(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn millis_to_rfc3339(millis: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(millis).map(|dt| dt.to_rfc3339())
}

fn local_day(occurred_at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(occurred_at)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|_| occurred_at.get(..10).unwrap_or(occurred_at).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::cursor_session::project_from_transcript_path;

    #[test]
    fn project_from_transcript_path_decodes_slug() {
        let path =
            Path::new("/home/.cursor/projects/Users-test-project/agent-transcripts/s1/s1.jsonl");
        assert_eq!(project_from_transcript_path(path), "/Users/test/project");
    }

    fn sample_session(
        session_id: &str,
        project: &str,
        last_seen_at: &str,
        turn_count: i64,
        error_count: i64,
        models_json: &str,
        tool_calls_json: &str,
    ) -> CursorSessionRecord {
        CursorSessionRecord {
            session_id: session_id.to_string(),
            project: project.to_string(),
            turn_count,
            success_count: turn_count - error_count,
            error_count,
            aborted_count: 0,
            tool_calls_json: tool_calls_json.to_string(),
            models_json: models_json.to_string(),
            first_seen_at: Some(last_seen_at.to_string()),
            last_seen_at: Some(last_seen_at.to_string()),
            files_touched: 1,
            source_file: format!("/tmp/{session_id}.jsonl"),
        }
    }

    #[test]
    fn summarize_includes_session_ids_newest_first() {
        let summary = summarize_cursor_sessions(&[
            sample_session(
                "sess-old",
                "/Users/test/alpha",
                "2026-08-16T10:00:00+00:00",
                2,
                1,
                r#"["grok-4.6"]"#,
                r#"{"Read":1}"#,
            ),
            sample_session(
                "sess-new",
                "/Users/test/beta",
                "2026-08-18T10:00:00+00:00",
                3,
                0,
                r#"["grok-4.6"]"#,
                r#"{"Read":2,"Shell":1}"#,
            ),
        ]);

        assert_eq!(summary.session_count, 2);
        assert_eq!(summary.active_project_count, 2);
        assert_eq!(summary.sessions.len(), 2);
        assert_eq!(summary.sessions[0].session_id, "sess-new");
        assert_eq!(summary.sessions[0].project, "/Users/test/beta");
        assert_eq!(summary.sessions[0].tool_call_count, 3);
        assert_eq!(summary.sessions[1].session_id, "sess-old");
        assert_eq!(summary.by_project.len(), 2);
        let beta = summary
            .by_project
            .iter()
            .find(|row| row.name == "/Users/test/beta")
            .expect("beta project");
        assert_eq!(beta.session_count, 1);
        assert_eq!(beta.turn_count, 3);
        assert_eq!(beta.error_count, 0);
        assert_eq!(beta.files_touched, 1);
        assert_eq!(
            beta.last_seen_at.as_deref(),
            Some("2026-08-18T10:00:00+00:00")
        );
    }
}
