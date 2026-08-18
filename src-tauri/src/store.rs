use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{CursorSessionRecord, CursorUsageEvent, Source, UsageRecord};

pub const ADAPTER_VERSION: i64 = 7;

pub fn open_db(path: &str) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    configure_connection(&conn)?;
    init_schema(&conn)?;
    Ok(conn)
}

pub fn open_memory() -> Result<Connection, String> {
    let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    init_schema(&conn)?;
    Ok(conn)
}

/// 只对真实文件落盘的连接生效：`:memory:` 数据库本来就没有并发读写者，WAL/NORMAL 这两个
/// pragma 在内存模式下会被 SQLite 静默忽略甚至报错，所以不对 `open_memory` 调用。
///
/// - `journal_mode=WAL`：托盘后台线程每隔几分钟跑一次完整 ingest，会长时间持有写事务；
///   WAL 让前端查询（读者）不必等这次写事务提交就能读到旧版本页，避免 UI 卡顿。
/// - `synchronous=NORMAL`：WAL 模式下官方推荐搭配 NORMAL，牺牲的持久性仅在系统级崩溃
///   （断电/内核崩溃，而非应用崩溃）时才可能丢最后几条已提交事务，可接受，换来显著更少的 fsync。
fn configure_connection(conn: &Connection) -> Result<(), String> {
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS usage_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            occurred_at TEXT NOT NULL,
            source TEXT NOT NULL,
            model TEXT NOT NULL,
            provider TEXT NOT NULL,
            project TEXT NOT NULL,
            session_id TEXT NOT NULL,
            source_file TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cache_read_tokens INTEGER NOT NULL,
            cache_creation_tokens INTEGER NOT NULL,
            reasoning_tokens INTEGER NOT NULL,
            total_tokens INTEGER NOT NULL,
            native_cost REAL
        );
        CREATE INDEX IF NOT EXISTS idx_usage_occurred ON usage_records(occurred_at);
        CREATE INDEX IF NOT EXISTS idx_usage_source ON usage_records(source);
        CREATE INDEX IF NOT EXISTS idx_usage_model ON usage_records(model);
        CREATE INDEX IF NOT EXISTS idx_usage_project ON usage_records(project);
        CREATE INDEX IF NOT EXISTS idx_usage_session ON usage_records(session_id);
        -- Ingestion replaces records by file; without this index every changed file scans the full cache.
        CREATE INDEX IF NOT EXISTS idx_usage_source_file ON usage_records(source_file);
        -- Almost every aggregate query in query.rs filters by source and/or occurred_at together
        -- (overview/trend/billing_windows); a composite index lets those use one index instead of
        -- a full scan + occurred_at index-only scan.
        CREATE INDEX IF NOT EXISTS idx_usage_source_occurred ON usage_records(source, occurred_at);

        CREATE TABLE IF NOT EXISTS ingested_files (
            path TEXT PRIMARY KEY,
            mtime_ms INTEGER NOT NULL,
            size INTEGER NOT NULL,
            source TEXT NOT NULL DEFAULT '',
            fingerprint TEXT NOT NULL DEFAULT '',
            adapter_version INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS cursor_account_usage (
            fingerprint TEXT PRIMARY KEY,
            occurred_at TEXT NOT NULL,
            model TEXT NOT NULL,
            input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            cache_read_tokens INTEGER NOT NULL,
            cache_creation_tokens INTEGER NOT NULL,
            is_headless INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_cursor_account_occurred
            ON cursor_account_usage(occurred_at);

        CREATE TABLE IF NOT EXISTS cursor_account_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cursor_sessions (
            source_file TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            project TEXT NOT NULL,
            turn_count INTEGER NOT NULL,
            success_count INTEGER NOT NULL,
            error_count INTEGER NOT NULL,
            aborted_count INTEGER NOT NULL,
            tool_calls_json TEXT NOT NULL,
            models_json TEXT NOT NULL DEFAULT '[]',
            first_seen_at TEXT,
            last_seen_at TEXT,
            files_touched INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_cursor_sessions_project ON cursor_sessions(project);
        CREATE INDEX IF NOT EXISTS idx_cursor_sessions_last_seen ON cursor_sessions(last_seen_at);

        CREATE TABLE IF NOT EXISTS cursor_session_files (
            path TEXT PRIMARY KEY,
            mtime_ms INTEGER NOT NULL,
            size INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cursor_session_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .map_err(|e| e.to_string())?;
    ensure_column(conn, "ingested_files", "source", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(
        conn,
        "ingested_files",
        "fingerprint",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        conn,
        "ingested_files",
        "adapter_version",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    // 源文件被工具自身清理后不再物理删除历史记录，只打时间戳归档（ADR 0004）。
    ensure_column(conn, "usage_records", "archived_at", "TEXT")?;
    // 必须放在上面的 ensure_column 之后：老版本缓存库的 ingested_files 表可能还没有
    // source 列，若把这条建索引语句挪进最上面的初始 CREATE TABLE batch，会在旧库上先于
    // ALTER TABLE 执行而报错。
    // reconcile_source 每个来源每轮 ingest 都要 "SELECT path FROM ingested_files WHERE
    // source = ?"，没有这个索引就是全表扫描。
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_ingested_files_source ON ingested_files(source);",
    )
    .map_err(|e| e.to_string())?;
    conn.execute_batch(
        r#"
        UPDATE ingested_files
        SET source = COALESCE(
            (SELECT source FROM usage_records WHERE source_file = ingested_files.path LIMIT 1),
            ''
        )
        WHERE source = '';
        "#,
    )
    .map_err(|e| e.to_string())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| e.to_string())?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if !columns.iter().any(|name| name == column) {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn insert_records(conn: &Connection, records: &[UsageRecord]) -> Result<u64, String> {
    let mut stmt = conn
        .prepare(
            r#"
            INSERT INTO usage_records (
                occurred_at, source, model, provider, project, session_id, source_file,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                reasoning_tokens, total_tokens, native_cost
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
            "#,
        )
        .map_err(|e| e.to_string())?;
    let mut written = 0u64;
    for record in records {
        stmt.execute(params![
            record.occurred_at,
            record.source.as_str(),
            record.model,
            record.provider,
            record.project,
            record.session_id,
            record.source_file,
            record.input_tokens,
            record.output_tokens,
            record.cache_read_tokens,
            record.cache_creation_tokens,
            record.reasoning_tokens,
            record.total_tokens,
            record.native_cost,
        ])
        .map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(written)
}

pub fn record_count_for_file(conn: &Connection, source_file: &str) -> Result<u64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM usage_records WHERE source_file = ?1",
        params![source_file],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as u64)
    .map_err(|e| e.to_string())
}

pub fn delete_records_for_file(conn: &Connection, source_file: &str) -> Result<u64, String> {
    conn.execute(
        "DELETE FROM usage_records WHERE source_file = ?1",
        params![source_file],
    )
    .map(|count| count as u64)
    .map_err(|e| e.to_string())
}

pub fn file_unchanged(
    conn: &Connection,
    path: &str,
    mtime_ms: i64,
    size: i64,
    source: Source,
    fingerprint: &str,
) -> Result<bool, String> {
    let row: Option<(i64, i64, String, String, i64)> = conn
        .query_row(
            "SELECT mtime_ms, size, source, fingerprint, adapter_version FROM ingested_files WHERE path = ?1",
            params![path],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(matches!(
        row,
        Some((m, s, cached_source, cached_fingerprint, version))
            if m == mtime_ms
                && s == size
                && cached_source == source.as_str()
                && cached_fingerprint == fingerprint
                && version == ADAPTER_VERSION
    ))
}

/// 托盘心跳用的轻量对账：只取路径、mtime、大小，不读源文件内容。
pub fn cached_file_stats(conn: &Connection) -> Result<Vec<(String, i64, i64)>, String> {
    let mut stmt = conn
        .prepare("SELECT path, mtime_ms, size FROM ingested_files")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn mark_file(
    conn: &Connection,
    path: &str,
    mtime_ms: i64,
    size: i64,
    source: Source,
    fingerprint: &str,
) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO ingested_files(path, mtime_ms, size, source, fingerprint, adapter_version)
        VALUES(?1,?2,?3,?4,?5,?6)
        ON CONFLICT(path) DO UPDATE SET
            mtime_ms = excluded.mtime_ms,
            size = excluded.size,
            source = excluded.source,
            fingerprint = excluded.fingerprint,
            adapter_version = excluded.adapter_version
        "#,
        params![
            path,
            mtime_ms,
            size,
            source.as_str(),
            fingerprint,
            ADAPTER_VERSION
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 本轮扫描已看不到的文件不再物理删除其历史记录：工具自身的日志清理/轮转不应抹掉
/// 本地已经统计过的用量。改为给对应记录打归档时间戳，记录仍计入所有统计查询；
/// 只清理 `ingested_files` 的缓存指纹（文件既已消失，也没有 mtime/大小可再对比）。
/// 见 `docs/adr/0004-archive-missing-source-files.md`。
pub fn reconcile_source(
    conn: &Connection,
    source: Source,
    seen_paths: &BTreeSet<String>,
) -> Result<u64, String> {
    let mut stmt = conn
        .prepare("SELECT path FROM ingested_files WHERE source = ?1")
        .map_err(|e| e.to_string())?;
    let cached = stmt
        .query_map(params![source.as_str()], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);

    let mut archived = 0;
    for path in cached {
        if !seen_paths.contains(&path) {
            archived += archive_records_for_file(conn, &path)?;
            conn.execute("DELETE FROM ingested_files WHERE path = ?1", params![path])
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(archived)
}

/// 把某源文件名下尚未归档的记录标记为已归档（幂等：重复调用不会改写已有的归档时间）。
/// 返回本次新归档的记录数。
pub fn archive_records_for_file(conn: &Connection, source_file: &str) -> Result<u64, String> {
    conn.execute(
        "UPDATE usage_records SET archived_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE source_file = ?1 AND archived_at IS NULL",
        params![source_file],
    )
    .map(|count| count as u64)
    .map_err(|e| e.to_string())
}

/// 永久删除某个来源（或全部来源）已归档的记录。用户在设置页显式触发，不参与常规摄取流程。
pub fn purge_archived(conn: &Connection, source: Option<Source>) -> Result<u64, String> {
    let removed = match source {
        Some(source) => conn.execute(
            "DELETE FROM usage_records WHERE archived_at IS NOT NULL AND source = ?1",
            params![source.as_str()],
        ),
        None => conn.execute(
            "DELETE FROM usage_records WHERE archived_at IS NOT NULL",
            [],
        ),
    }
    .map_err(|e| e.to_string())?;
    Ok(removed as u64)
}

pub fn invalidate_source(conn: &Connection, source: Source) -> Result<(), String> {
    conn.execute(
        "UPDATE ingested_files SET adapter_version = 0 WHERE source = ?1",
        params![source.as_str()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn remove_unknown_sources(conn: &Connection) -> Result<u64, String> {
    let known = Source::ALL
        .iter()
        .map(|source| format!("'{}'", source.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    let removed = conn
        .execute(
            &format!("DELETE FROM usage_records WHERE source NOT IN ({known})"),
            [],
        )
        .map_err(|e| e.to_string())? as u64;
    conn.execute(
        &format!("DELETE FROM ingested_files WHERE source NOT IN ({known})"),
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(removed)
}

/// 返回 (缓存文件数, 记录总数（含已归档）, Token 总数（含已归档）, 已归档记录数)。
pub fn source_cache_stats(
    conn: &Connection,
    source: Source,
) -> Result<(u64, u64, i64, u64), String> {
    let cached_files = conn
        .query_row(
            "SELECT COUNT(*) FROM ingested_files WHERE source = ?1",
            params![source.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())? as u64;
    let (record_count, total_tokens, archived_record_count) = conn
        .query_row(
            r#"
            SELECT COUNT(*), COALESCE(SUM(total_tokens), 0),
                   COUNT(*) FILTER (WHERE archived_at IS NOT NULL)
            FROM usage_records WHERE source = ?1
            "#,
            params![source.as_str()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;
    Ok((
        cached_files,
        record_count as u64,
        total_tokens,
        archived_record_count as u64,
    ))
}

pub fn load_all(conn: &Connection) -> Result<Vec<UsageRecord>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT occurred_at, source, model, provider, project, session_id, source_file,
                   input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                   reasoning_tokens, total_tokens, native_cost
            FROM usage_records
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            let source_value: String = row.get(1)?;
            let source = Source::parse(&source_value).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    format!("未知来源：{source_value}").into(),
                )
            })?;
            Ok(UsageRecord {
                occurred_at: row.get(0)?,
                source,
                model: row.get(2)?,
                provider: row.get(3)?,
                project: row.get(4)?,
                session_id: row.get(5)?,
                source_file: row.get(6)?,
                input_tokens: row.get(7)?,
                output_tokens: row.get(8)?,
                cache_read_tokens: row.get(9)?,
                cache_creation_tokens: row.get(10)?,
                reasoning_tokens: row.get(11)?,
                total_tokens: row.get(12)?,
                native_cost: row.get(13)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// 按指纹去重写入 Cursor 账号用量事件，返回新插入的行数。
pub fn upsert_cursor_account_events(
    conn: &Connection,
    events: &[CursorUsageEvent],
) -> Result<u64, String> {
    let mut stmt = conn
        .prepare(
            r#"
            INSERT OR IGNORE INTO cursor_account_usage (
                fingerprint, occurred_at, model,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                is_headless
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
            "#,
        )
        .map_err(|e| e.to_string())?;
    let mut written = 0u64;
    for event in events {
        let changed = stmt
            .execute(params![
                event.fingerprint(),
                event.occurred_at,
                event.model,
                event.input_tokens,
                event.output_tokens,
                event.cache_read_tokens,
                event.cache_creation_tokens,
                i64::from(event.is_headless),
            ])
            .map_err(|e| e.to_string())?;
        written += changed as u64;
    }
    Ok(written)
}

pub fn load_cursor_account_events(conn: &Connection) -> Result<Vec<CursorUsageEvent>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT occurred_at, model, input_tokens, output_tokens,
                   cache_read_tokens, cache_creation_tokens, is_headless
            FROM cursor_account_usage
            ORDER BY occurred_at ASC
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CursorUsageEvent {
                occurred_at: row.get(0)?,
                model: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                cache_read_tokens: row.get(4)?,
                cache_creation_tokens: row.get(5)?,
                is_headless: row.get::<_, i64>(6)? != 0,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn set_cursor_account_as_of(conn: &Connection, as_of: &str) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO cursor_account_meta(key, value) VALUES('as_of', ?1)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
        params![as_of],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn cursor_account_as_of(conn: &Connection) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM cursor_account_meta WHERE key = 'as_of'",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn max_cursor_account_occurred_ms(conn: &Connection) -> Result<Option<i64>, String> {
    let occurred_at: Option<String> = conn
        .query_row(
            "SELECT MAX(occurred_at) FROM cursor_account_usage",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let Some(occurred_at) = occurred_at else {
        return Ok(None);
    };
    let millis = chrono::DateTime::parse_from_rfc3339(&occurred_at)
        .map_err(|e| format!("Cursor 账号用量时间戳无法解析：{e}"))?
        .timestamp_millis();
    Ok(Some(millis))
}

pub fn clear_cursor_account_usage(conn: &Connection) -> Result<(), String> {
    conn.execute("DELETE FROM cursor_account_usage", [])
        .map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM cursor_account_meta", [])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn cursor_session_has_source_file(conn: &Connection, path: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT 1 FROM cursor_sessions WHERE source_file = ?1",
        params![path],
        |_| Ok(()),
    )
    .optional()
    .map(|row| row.is_some())
    .map_err(|e| e.to_string())
}

pub fn cached_cursor_session_file_stats(
    conn: &Connection,
) -> Result<Vec<(String, i64, i64)>, String> {
    let mut stmt = conn
        .prepare("SELECT path, mtime_ms, size FROM cursor_session_files")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn cursor_session_file_fingerprint(
    conn: &Connection,
    path: &str,
) -> Result<Option<(i64, i64)>, String> {
    conn.query_row(
        "SELECT mtime_ms, size FROM cursor_session_files WHERE path = ?1",
        params![path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn upsert_cursor_session(
    conn: &Connection,
    record: &CursorSessionRecord,
) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO cursor_sessions (
            source_file, session_id, project, turn_count, success_count, error_count, aborted_count,
            tool_calls_json, models_json, first_seen_at, last_seen_at, files_touched
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
        ON CONFLICT(source_file) DO UPDATE SET
            session_id = excluded.session_id,
            project = excluded.project,
            turn_count = excluded.turn_count,
            success_count = excluded.success_count,
            error_count = excluded.error_count,
            aborted_count = excluded.aborted_count,
            tool_calls_json = excluded.tool_calls_json,
            models_json = excluded.models_json,
            first_seen_at = COALESCE(cursor_sessions.first_seen_at, excluded.first_seen_at),
            last_seen_at = excluded.last_seen_at,
            files_touched = excluded.files_touched
        "#,
        params![
            record.source_file,
            record.session_id,
            record.project,
            record.turn_count,
            record.success_count,
            record.error_count,
            record.aborted_count,
            record.tool_calls_json,
            record.models_json,
            record.first_seen_at,
            record.last_seen_at,
            record.files_touched,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn upsert_cursor_session_file(
    conn: &Connection,
    path: &str,
    mtime_ms: i64,
    size: i64,
) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO cursor_session_files(path, mtime_ms, size) VALUES(?1,?2,?3)
        ON CONFLICT(path) DO UPDATE SET mtime_ms = excluded.mtime_ms, size = excluded.size
        "#,
        params![path, mtime_ms, size],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_cursor_sessions(conn: &Connection) -> Result<Vec<CursorSessionRecord>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT source_file, session_id, project, turn_count, success_count, error_count, aborted_count,
                   tool_calls_json, models_json, first_seen_at, last_seen_at, files_touched
            FROM cursor_sessions
            ORDER BY last_seen_at ASC, source_file ASC
            "#,
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(CursorSessionRecord {
                source_file: row.get(0)?,
                session_id: row.get(1)?,
                project: row.get(2)?,
                turn_count: row.get(3)?,
                success_count: row.get(4)?,
                error_count: row.get(5)?,
                aborted_count: row.get(6)?,
                tool_calls_json: row.get(7)?,
                models_json: row.get(8)?,
                first_seen_at: row.get(9)?,
                last_seen_at: row.get(10)?,
                files_touched: row.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn reconcile_cursor_sessions(
    conn: &Connection,
    seen_paths: &BTreeSet<String>,
) -> Result<u64, String> {
    let cached: Vec<String> = conn
        .prepare("SELECT source_file FROM cursor_sessions")
        .map_err(|e| e.to_string())?
        .query_map([], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let mut removed = 0u64;
    for path in cached {
        if seen_paths.contains(&path) {
            continue;
        }
        conn.execute(
            "DELETE FROM cursor_sessions WHERE source_file = ?1",
            params![path],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM cursor_session_files WHERE path = ?1",
            params![path],
        )
        .map_err(|e| e.to_string())?;
        removed += 1;
    }
    Ok(removed)
}

pub fn set_cursor_session_as_of(conn: &Connection, as_of: &str) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO cursor_session_meta(key, value) VALUES('as_of', ?1)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
        params![as_of],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn cursor_session_as_of(conn: &Connection) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM cursor_session_meta WHERE key = 'as_of'",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| e.to_string())
}

pub fn set_cursor_tracking_fingerprint(conn: &Connection, fingerprint: &str) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO cursor_session_meta(key, value) VALUES('tracking_fingerprint', ?1)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
        params![fingerprint],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn cursor_tracking_fingerprint(conn: &Connection) -> Result<String, String> {
    conn.query_row(
        "SELECT value FROM cursor_session_meta WHERE key = 'tracking_fingerprint'",
        [],
        |row| row.get(0),
    )
    .optional()
    .map(|value| value.unwrap_or_default())
    .map_err(|e| e.to_string())
}
