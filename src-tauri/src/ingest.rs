use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::Connection;

use crate::adapters::cursor::{parse_cursor_commits, summarize_code_volume, CursorCommitRow};
use crate::adapters::opencode::{parse_opencode_messages, OpencodeMessage};
use crate::adapters::{claude, codex, dsh, factory, gemini, grok, kimi, pi, qwen};
use crate::domain::{
    CodeVolumeSummary, IngestIssue, IngestReport, Source, SourceDiagnostic, SourceIngestReport,
    UsageRecord,
};
use crate::store;

pub fn default_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub fn ingest_all(conn: &Connection, home: &Path) -> Result<IngestReport, String> {
    let transaction = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let removed_unknown = store::remove_unknown_sources(&transaction)?;
    let mut report = ingest_all_inner(&transaction, home)?;
    report.records_removed += removed_unknown;
    report.partial_success = report.files_failed > 0;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(report)
}

pub fn source_diagnostics(conn: &Connection, home: &Path) -> Result<Vec<SourceDiagnostic>, String> {
    Source::ALL
        .iter()
        .map(|source| {
            let root = source_root(home, *source);
            let (cached_files, record_count, total_tokens) =
                store::source_cache_stats(conn, *source)?;
            Ok(SourceDiagnostic {
                source: source.as_str().to_string(),
                application: source.application_name().to_string(),
                detected: root.exists(),
                root_path: root.to_string_lossy().to_string(),
                cached_files,
                record_count,
                total_tokens,
                coverage: source_coverage(*source).to_string(),
            })
        })
        .collect()
}

pub fn rebuild_cache(
    conn: &Connection,
    home: &Path,
    source: Option<Source>,
) -> Result<IngestReport, String> {
    let transaction = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut removed_unknown = 0;
    match source {
        Some(source) => store::invalidate_source(&transaction, source)?,
        None => {
            removed_unknown = store::remove_unknown_sources(&transaction)?;
            for source in Source::ALL {
                store::invalidate_source(&transaction, source)?;
            }
        }
    }

    let mut report = IngestReport {
        records_removed: removed_unknown,
        ..IngestReport::default()
    };
    match source {
        Some(source) => ingest_source(&transaction, home, source, &mut report)?,
        None => ingest_all_sources(&transaction, home, &mut report)?,
    }
    report.partial_success = report.files_failed > 0;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(report)
}

fn ingest_all_inner(conn: &Connection, home: &Path) -> Result<IngestReport, String> {
    let mut report = IngestReport::default();
    ingest_all_sources(conn, home, &mut report)?;
    Ok(report)
}

fn ingest_all_sources(
    conn: &Connection,
    home: &Path,
    report: &mut IngestReport,
) -> Result<(), String> {
    for source in Source::ALL {
        if let Err(error) = ingest_source(conn, home, source, report) {
            if error.starts_with("扫描目录") {
                let root = source_root(home, source);
                record_failure(report, source, &root.to_string_lossy(), &error);
                continue;
            }
            return Err(error);
        }
    }
    Ok(())
}

fn source_root(home: &Path, source: Source) -> PathBuf {
    match source {
        Source::Codex => home.join(".codex/sessions"),
        Source::Claude => home.join(".claude/projects"),
        Source::Pi => home.join(".pi/agent/sessions"),
        Source::Opencode => home.join(".local/share/opencode/opencode.db"),
        Source::Kimi => home.join(".kimi/sessions"),
        Source::Dsh => home.join(".dsh/sessions"),
        Source::Gemini => home.join(".gemini/tmp"),
        Source::Grok => home.join(".grok/sessions"),
        Source::Qwen => home.join(".qwen/tmp"),
        Source::Factory => home.join(".factory/sessions"),
    }
}

fn source_coverage(source: Source) -> &'static str {
    match source {
        Source::Qwen => "本地无 Token",
        Source::Grok => "仅上下文总量",
        Source::Factory => "会话累计 Token",
        _ => "轮级 Token",
    }
}

fn ingest_source(
    conn: &Connection,
    home: &Path,
    source: Source,
    report: &mut IngestReport,
) -> Result<(), String> {
    match source {
        Source::Codex => ingest_jsonl_tree(
            conn,
            source,
            &home.join(".codex/sessions"),
            "jsonl",
            report,
            |content, path| Ok(codex::parse_codex_jsonl(content, path)),
        ),
        Source::Claude => ingest_jsonl_tree(
            conn,
            source,
            &home.join(".claude/projects"),
            "jsonl",
            report,
            |content, path| Ok(claude::parse_claude_jsonl(content, path)),
        ),
        Source::Pi => ingest_jsonl_tree(
            conn,
            source,
            &home.join(".pi/agent/sessions"),
            "jsonl",
            report,
            |content, path| Ok(pi::parse_pi_jsonl(content, path)),
        ),
        Source::Kimi => ingest_kimi(conn, &home.join(".kimi"), report),
        Source::Dsh => ingest_dsh(conn, &home.join(".dsh/sessions"), report),
        Source::Gemini => ingest_gemini(conn, &home.join(".gemini/tmp"), report),
        Source::Grok => ingest_grok(conn, &home.join(".grok/sessions"), report),
        Source::Qwen => ingest_qwen(conn, &home.join(".qwen/tmp"), report),
        Source::Factory => ingest_factory(conn, &home.join(".factory/sessions"), report),
        Source::Opencode => ingest_opencode(
            conn,
            &home.join(".local/share/opencode/opencode.db"),
            report,
        ),
    }
}

fn ingest_jsonl_tree(
    conn: &Connection,
    source: Source,
    root: &Path,
    ext: &str,
    report: &mut IngestReport,
    parse: impl Fn(&str, &str) -> Result<Vec<UsageRecord>, String>,
) -> Result<(), String> {
    set_detected(report, source, root.exists());
    let mut seen = BTreeSet::new();
    for path in walk_files(root, ext)? {
        seen.insert(path.to_string_lossy().to_string());
        ingest_one(conn, source, &path, "", report, |bytes, loc| {
            let content = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
            validate_jsonl(&content)?;
            parse(&content, loc)
        })?;
    }
    reconcile_source(conn, source, &seen, report)
}

fn ingest_one(
    conn: &Connection,
    source: Source,
    path: &Path,
    fingerprint: &str,
    report: &mut IngestReport,
    parse: impl Fn(&[u8], &str) -> Result<Vec<UsageRecord>, String>,
) -> Result<(), String> {
    increment(report, source, |source_report| {
        source_report.files_seen += 1
    });
    report.files_seen += 1;
    let loc = path.to_string_lossy().to_string();
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(error) => {
            record_failure(report, source, &loc, &error.to_string());
            return Ok(());
        }
    };
    let size = meta.len() as i64;
    let mtime_ms = modified_millis(&meta);
    let cache_fingerprint = format!("{}|{fingerprint}", metadata_fingerprint(path));
    if store::file_unchanged(conn, &loc, mtime_ms, size, source, &cache_fingerprint)? {
        increment(report, source, |source_report| {
            source_report.files_skipped += 1
        });
        report.files_skipped += 1;
        return Ok(());
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            record_failure(report, source, &loc, &error.to_string());
            return Ok(());
        }
    };
    let records = match parse(&bytes, &loc) {
        Ok(records) => records,
        Err(error) => {
            record_failure(report, source, &loc, &error);
            return Ok(());
        }
    };
    let previous_count = store::record_count_for_file(conn, &loc)?;
    if previous_count > 0 && records.len() < previous_count as usize && is_append_log_source(source)
    {
        record_failure(
            report,
            source,
            &loc,
            &format!(
                "解析记录从 {previous_count} 条降为 {} 条，已保留上次正确缓存",
                records.len()
            ),
        );
        return Ok(());
    }

    store::delete_records_for_file(conn, &loc)?;
    let written = store::insert_records(conn, &records)?;
    store::mark_file(conn, &loc, mtime_ms, size, source, &cache_fingerprint)?;
    report.records_written += written;
    report.files_parsed += 1;
    increment(report, source, |source_report| {
        source_report.records_written += written;
        source_report.files_parsed += 1;
    });
    Ok(())
}

fn is_append_log_source(source: Source) -> bool {
    matches!(
        source,
        Source::Codex | Source::Claude | Source::Pi | Source::Kimi | Source::Dsh | Source::Grok
    )
}

fn validate_jsonl(content: &str) -> Result<(), String> {
    for (index, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        serde_json::from_str::<serde_json::Value>(line)
            .map_err(|error| format!("第 {} 行 JSON 无效：{error}", index + 1))?;
    }
    Ok(())
}

fn ingest_kimi(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    let source = Source::Kimi;
    let sessions = root.join("sessions");
    set_detected(report, source, sessions.exists());
    let sidecar = root.join("kimi.json");
    let fingerprint = content_fingerprint(&sidecar);
    let projects = match kimi_projects(root) {
        Ok(projects) => projects,
        Err(error) => {
            record_failure(
                report,
                source,
                &sidecar.to_string_lossy(),
                &format!("Kimi 项目映射无效：{error}"),
            );
            return Ok(());
        }
    };
    let mut seen = BTreeSet::new();
    for path in walk_files(&sessions, "jsonl")? {
        if path.file_name().and_then(|name| name.to_str()) != Some("wire.jsonl") {
            continue;
        }
        seen.insert(path.to_string_lossy().to_string());
        let session_id = path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string();
        let project = projects
            .iter()
            .find(|(id, _)| id == &session_id)
            .map(|(_, project)| project.clone())
            .unwrap_or_else(|| {
                path.parent()
                    .and_then(|parent| parent.parent())
                    .and_then(|parent| parent.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
                    .to_string()
            });
        ingest_one(conn, source, &path, &fingerprint, report, |bytes, loc| {
            let content = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
            validate_jsonl(&content)?;
            Ok(kimi::parse_kimi_wire(&content, loc, &project))
        })?;
    }
    reconcile_source(conn, source, &seen, report)
}

fn kimi_projects(root: &Path) -> Result<Vec<(String, String)>, String> {
    let path = root.join("kimi.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let value =
        serde_json::from_str::<serde_json::Value>(&text).map_err(|error| error.to_string())?;
    Ok(value
        .get("work_dirs")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some((
                        item.get("last_session_id")?.as_str()?.to_string(),
                        item.get("path")?.as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default())
}

fn ingest_dsh(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    let source = Source::Dsh;
    set_detected(report, source, root.exists());
    let mut seen = BTreeSet::new();
    for path in walk_suffix(root, "session.jsonl.zstd")? {
        seen.insert(path.to_string_lossy().to_string());
        ingest_one(conn, source, &path, "", report, dsh::parse_dsh_zstd)?;
    }
    reconcile_source(conn, source, &seen, report)
}

fn ingest_gemini(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    let source = Source::Gemini;
    set_detected(report, source, root.exists());
    let mut seen = BTreeSet::new();
    for path in walk_files(root, "json")? {
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .starts_with("session-")
        {
            continue;
        }
        seen.insert(path.to_string_lossy().to_string());
        ingest_one(conn, source, &path, "", report, |bytes, loc| {
            let content = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
            serde_json::from_str::<serde_json::Value>(&content).map_err(|e| e.to_string())?;
            Ok(gemini::parse_gemini_session(&content, loc))
        })?;
    }
    reconcile_source(conn, source, &seen, report)
}

fn ingest_grok(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    let source = Source::Grok;
    set_detected(report, source, root.exists());
    let mut seen = BTreeSet::new();
    for path in walk_files(root, "jsonl")? {
        if path.file_name().and_then(|name| name.to_str()) != Some("updates.jsonl") {
            continue;
        }
        seen.insert(path.to_string_lossy().to_string());
        let summary_path = path
            .parent()
            .map(|parent| parent.join("summary.json"))
            .unwrap_or_default();
        let fingerprint = content_fingerprint(&summary_path);
        let summary = if summary_path.exists() {
            match fs::read_to_string(&summary_path)
                .map_err(|error| error.to_string())
                .and_then(|text| {
                    serde_json::from_str::<serde_json::Value>(&text)
                        .map_err(|error| error.to_string())
                }) {
                Ok(summary) => Some(summary),
                Err(error) => {
                    record_failure(
                        report,
                        source,
                        &summary_path.to_string_lossy(),
                        &format!("Grok 模型摘要无效：{error}"),
                    );
                    continue;
                }
            }
        } else {
            None
        };
        let model = summary
            .as_ref()
            .and_then(|value| value.get("current_model_id"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        ingest_one(conn, source, &path, &fingerprint, report, |bytes, loc| {
            let content = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
            validate_jsonl(&content)?;
            Ok(grok::parse_grok_updates(&content, loc, &model))
        })?;
    }
    reconcile_source(conn, source, &seen, report)
}

fn ingest_qwen(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    let source = Source::Qwen;
    set_detected(report, source, root.exists());
    let mut seen = BTreeSet::new();
    for path in walk_files(root, "json")? {
        if path.file_name().and_then(|name| name.to_str()) != Some("logs.json") {
            continue;
        }
        seen.insert(path.to_string_lossy().to_string());
        ingest_one(conn, source, &path, "", report, |bytes, loc| {
            let content = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
            serde_json::from_str::<serde_json::Value>(&content).map_err(|e| e.to_string())?;
            Ok(qwen::parse_qwen_session(&content, loc))
        })?;
    }
    reconcile_source(conn, source, &seen, report)
}

fn ingest_factory(conn: &Connection, root: &Path, report: &mut IngestReport) -> Result<(), String> {
    let source = Source::Factory;
    set_detected(report, source, root.exists());
    let mut seen = BTreeSet::new();
    for path in walk_suffix(root, ".settings.json")? {
        seen.insert(path.to_string_lossy().to_string());
        ingest_one(conn, source, &path, "", report, |bytes, loc| {
            let content = String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?;
            serde_json::from_str::<serde_json::Value>(&content).map_err(|e| e.to_string())?;
            Ok(factory::parse_factory_settings(&content, loc))
        })?;
    }
    reconcile_source(conn, source, &seen, report)
}

fn ingest_opencode(
    conn: &Connection,
    db_path: &Path,
    report: &mut IngestReport,
) -> Result<(), String> {
    let source = Source::Opencode;
    set_detected(report, source, db_path.exists());
    let mut seen = BTreeSet::new();
    if db_path.exists() {
        seen.insert(db_path.to_string_lossy().to_string());
        let wal_path = PathBuf::from(format!("{}-wal", db_path.to_string_lossy()));
        let fingerprint = metadata_fingerprint(&wal_path);
        ingest_one(conn, source, db_path, &fingerprint, report, |_, loc| {
            let source_db = open_readonly(db_path)?;
            let mut stmt = source_db
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
                let data = serde_json::from_str(&data)
                    .map_err(|error| format!("OpenCode message JSON 无效：{error}"))?;
                messages.push(OpencodeMessage {
                    session_id,
                    source_file: loc.to_string(),
                    data,
                });
            }
            Ok(parse_opencode_messages(&messages))
        })?;
    }
    reconcile_source(conn, source, &seen, report)
}

fn reconcile_source(
    conn: &Connection,
    source: Source,
    seen: &BTreeSet<String>,
    report: &mut IngestReport,
) -> Result<(), String> {
    if source_report_mut(report, source).files_failed > 0 {
        return Ok(());
    }
    let removed = store::reconcile_source(conn, source, seen)?;
    report.records_removed += removed;
    increment(report, source, |source_report| {
        source_report.records_removed += removed
    });
    Ok(())
}

fn source_report_mut(report: &mut IngestReport, source: Source) -> &mut SourceIngestReport {
    report
        .sources
        .iter_mut()
        .find(|entry| entry.source == source.as_str())
        .expect("all sources are initialized")
}

fn increment(
    report: &mut IngestReport,
    source: Source,
    update: impl FnOnce(&mut SourceIngestReport),
) {
    update(source_report_mut(report, source));
}

fn set_detected(report: &mut IngestReport, source: Source, detected: bool) {
    source_report_mut(report, source).detected = detected;
}

fn record_failure(report: &mut IngestReport, source: Source, path: &str, message: &str) {
    report.files_failed += 1;
    report.partial_success = true;
    increment(report, source, |source_report| {
        source_report.files_failed += 1
    });
    report.issues.push(IngestIssue {
        source: source.as_str().to_string(),
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

fn content_fingerprint(path: &Path) -> String {
    let Ok(bytes) = fs::read(path) else {
        return "missing".to_string();
    };
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in &bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{}:{hash:016x}", bytes.len())
}

fn metadata_fingerprint(path: &Path) -> String {
    let Ok(meta) = fs::metadata(path) else {
        return "missing".to_string();
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}:{modified}:{}:{}:{}",
            meta.len(),
            meta.ino(),
            meta.ctime(),
            meta.ctime_nsec()
        )
    }
    #[cfg(not(unix))]
    format!("{}:{modified}", meta.len())
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
    let source_db = open_readonly(&db_path)?;
    let mut stmt = source_db
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
            let percentage: Option<String> = row.get(6)?;
            Ok(CursorCommitRow {
                commit_hash: row.get(0)?,
                branch: row.get(1)?,
                scored_at_ms: row.get(2)?,
                lines_added: row.get(3)?,
                composer_lines_added: row.get(4)?,
                human_lines_added: row.get(5)?,
                ai_percentage: percentage.and_then(|value| value.parse().ok()),
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

fn walk_files(root: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
    walk_matching(root, |path| {
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(extension))
            .unwrap_or(false)
    })
}

fn walk_suffix(root: &Path, suffix: &str) -> Result<Vec<PathBuf>, String> {
    walk_matching(root, |path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(suffix))
            .unwrap_or(false)
    })
}

fn walk_matching(root: &Path, matches: impl Fn(&Path) -> bool) -> Result<Vec<PathBuf>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let entries =
        fs::read_dir(root).map_err(|error| format!("扫描目录 {} 失败：{error}", root.display()))?;
    let mut output = Vec::new();
    let mut stack = entries
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("扫描目录 {} 失败：{error}", root.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    while let Some(path) = stack.pop() {
        let file_type = fs::symlink_metadata(&path)
            .map_err(|error| format!("扫描目录 {} 失败：{error}", path.display()))?
            .file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let entries = fs::read_dir(&path)
                .map_err(|error| format!("扫描目录 {} 失败：{error}", path.display()))?;
            stack.extend(
                entries
                    .map(|entry| {
                        entry
                            .map(|entry| entry.path())
                            .map_err(|error| format!("扫描目录 {} 失败：{error}", path.display()))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
        } else if file_type.is_file() && matches(&path) {
            output.push(path);
        }
    }
    Ok(output)
}
