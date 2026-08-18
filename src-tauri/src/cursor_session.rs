use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::Connection;

use crate::adapters::cursor_session::{build_cursor_session_record, parse_cursor_session_transcript};
use crate::domain::{
    CursorSessionDailyPoint, CursorSessionProjectRow, CursorSessionRecord, CursorSessionSummaryDto,
    IngestIssue, IngestReport,
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
                report.files_skipped += 1;
                continue;
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
        let record = match build_cursor_session_record(&path_key, &parsed, seen_at) {
            Ok(record) => record,
            Err(error) => {
                record_issue(report, &path_key, &error);
                any_failed = true;
                continue;
            }
        };

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

    let _ = store::set_cursor_session_as_of(conn, &chrono::Utc::now().to_rfc3339());
}

pub fn load_summary(conn: &Connection) -> Result<CursorSessionSummaryDto, String> {
    let sessions = store::load_cursor_sessions(conn)?;
    let mut summary = summarize_cursor_sessions(&sessions);
    summary.as_of = store::cursor_session_as_of(conn)?;
    Ok(summary)
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

    let mut projects: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for session in sessions {
        let project = if session.project.is_empty() {
            "未知项目".to_string()
        } else {
            session.project.clone()
        };
        let entry = projects.entry(project).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += session.turn_count;
    }
    let active_project_count = projects.len() as i64;
    let mut by_project: Vec<CursorSessionProjectRow> = projects
        .into_iter()
        .map(|(name, (session_count, turn_count))| CursorSessionProjectRow {
            name,
            session_count,
            turn_count,
        })
        .collect();
    by_project.sort_by(|a, b| {
        b.session_count
            .cmp(&a.session_count)
            .then_with(|| b.turn_count.cmp(&a.turn_count))
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut daily: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for session in sessions {
        let Some(day) = session
            .last_seen_at
            .as_deref()
            .map(local_day)
            .filter(|day| !day.is_empty())
        else {
            continue;
        };
        let entry = daily.entry(day).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += session.turn_count;
    }
    let daily = daily
        .into_iter()
        .map(|(bucket, (session_count, turn_count))| CursorSessionDailyPoint {
            bucket,
            session_count,
            turn_count,
        })
        .collect();

    CursorSessionSummaryDto {
        as_of: None,
        session_count,
        turn_count,
        error_rate,
        active_project_count,
        by_project,
        daily,
    }
}

fn walk_transcripts(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    walk_transcripts_inner(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn walk_transcripts_inner(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("扫描 Cursor 会话目录 {} 失败：{e}", dir.display()))? {
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
        let path = Path::new(
            "/home/.cursor/projects/Users-test-project/agent-transcripts/s1/s1.jsonl",
        );
        assert_eq!(
            project_from_transcript_path(path),
            "/Users/test/project"
        );
    }
}
