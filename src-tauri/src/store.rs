use std::collections::BTreeSet;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::domain::{
    CursorSessionRecord, CursorUsageEvent, OfficialQuotaWindow, Source, UsageRecord,
};

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

/// 只读连接。摄取会长时间占着写连接和写事务；查询必须走另一条连接，才能用上 WAL
/// 的「读者不阻塞未提交写者」。这里不能跑 `init_schema` / `journal_mode`，那些是写操作。
pub fn open_readonly(path: &str) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())
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
            user_prompt_count INTEGER NOT NULL DEFAULT 0,
            subagent_count INTEGER NOT NULL DEFAULT 0,
            tool_calls_json TEXT NOT NULL,
            models_json TEXT NOT NULL DEFAULT '[]',
            sources_json TEXT NOT NULL DEFAULT '[]',
            extensions_json TEXT NOT NULL DEFAULT '{}',
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

        CREATE TABLE IF NOT EXISTS conversation_sessions (
            source TEXT NOT NULL,
            session_id TEXT NOT NULL,
            title TEXT NOT NULL,
            project TEXT NOT NULL,
            model TEXT NOT NULL,
            started_at TEXT NOT NULL,
            ended_at TEXT NOT NULL,
            source_file TEXT NOT NULL,
            capabilities_json TEXT NOT NULL DEFAULT '[]',
            support_status TEXT NOT NULL DEFAULT 'experimental',
            PRIMARY KEY(source, session_id)
        );
        CREATE INDEX IF NOT EXISTS idx_conversation_sessions_ended
            ON conversation_sessions(ended_at DESC);

        CREATE TABLE IF NOT EXISTS official_quota (
            provider TEXT PRIMARY KEY,
            windows_json TEXT NOT NULL,
            captured_at TEXT NOT NULL,
            error TEXT
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
    ensure_column(
        conn,
        "cursor_sessions",
        "user_prompt_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "cursor_sessions",
        "subagent_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "cursor_sessions",
        "sources_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        conn,
        "cursor_sessions",
        "extensions_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
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

/// 托盘心跳用的轻量对账：一次取出比对所需字段，避免扫盘时再逐条查库。
#[derive(Debug, Clone)]
pub struct IngestedFileCacheRow {
    pub path: String,
    pub mtime_ms: i64,
    pub size: i64,
    pub source: String,
    pub fingerprint: String,
    pub adapter_version: i64,
}

pub fn cached_ingested_files(conn: &Connection) -> Result<Vec<IngestedFileCacheRow>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT path, mtime_ms, size, source, fingerprint, adapter_version FROM ingested_files",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(IngestedFileCacheRow {
                path: row.get(0)?,
                mtime_ms: row.get(1)?,
                size: row.get(2)?,
                source: row.get(3)?,
                fingerprint: row.get(4)?,
                adapter_version: row.get(5)?,
            })
        })
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

pub fn cursor_account_events_page(
    conn: &Connection,
    page: u32,
    page_size: u32,
    sort_dir: &str,
) -> Result<(u32, Vec<crate::domain::CursorUsageEvent>), String> {
    let total: u32 = conn
        .query_row("SELECT COUNT(*) FROM cursor_account_usage", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    let dir = if sort_dir.eq_ignore_ascii_case("asc") {
        "ASC"
    } else {
        "DESC"
    };
    let offset = (page.saturating_sub(1) as i64) * page_size as i64;
    let sql = format!(
        r#"
        SELECT occurred_at, model, input_tokens, output_tokens,
               cache_read_tokens, cache_creation_tokens, is_headless
        FROM cursor_account_usage
        ORDER BY occurred_at {dir}, model ASC
        LIMIT ?1 OFFSET ?2
        "#
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![page_size as i64, offset], |row| {
            Ok(crate::domain::CursorUsageEvent {
                occurred_at: row.get(0)?,
                model: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                cache_read_tokens: row.get(4)?,
                cache_creation_tokens: row.get(5)?,
                is_headless: row.get::<_, i64>(6)? != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok((total, rows))
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

pub fn upsert_official_quota(
    conn: &Connection,
    provider: &str,
    windows: &[OfficialQuotaWindow],
    captured_at: &str,
    error: Option<&str>,
) -> Result<(), String> {
    let windows_json = serde_json::to_string(windows).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO official_quota(provider, windows_json, captured_at, error)
         VALUES(?1, ?2, ?3, ?4)
         ON CONFLICT(provider) DO UPDATE SET
            windows_json = excluded.windows_json,
            captured_at = excluded.captured_at,
            error = excluded.error",
        params![provider, windows_json, captured_at, error],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_official_quota_error(
    conn: &Connection,
    provider: &str,
    error: &str,
) -> Result<(), String> {
    let updated = conn
        .execute(
            "UPDATE official_quota SET error = ?2 WHERE provider = ?1",
            params![provider, error],
        )
        .map_err(|e| e.to_string())?;
    if updated == 0 {
        conn.execute(
            "INSERT INTO official_quota(provider, windows_json, captured_at, error)
             VALUES(?1, '[]', '', ?2)",
            params![provider, error],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

type OfficialQuotaRow = (Vec<OfficialQuotaWindow>, String, Option<String>);

pub fn load_official_quota_row(
    conn: &Connection,
    provider: &str,
) -> Result<Option<OfficialQuotaRow>, String> {
    let row = conn
        .query_row(
            "SELECT windows_json, captured_at, error FROM official_quota WHERE provider = ?1",
            params![provider],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let Some((windows_json, captured_at, error)) = row else {
        return Ok(None);
    };
    let windows: Vec<OfficialQuotaWindow> =
        serde_json::from_str(&windows_json).map_err(|e| format!("官方额度缓存损坏：{e}"))?;
    Ok(Some((windows, captured_at, error)))
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
            user_prompt_count, subagent_count, tool_calls_json, models_json, sources_json,
            extensions_json, first_seen_at, last_seen_at, files_touched
        ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
        ON CONFLICT(source_file) DO UPDATE SET
            session_id = excluded.session_id,
            project = excluded.project,
            turn_count = excluded.turn_count,
            success_count = excluded.success_count,
            error_count = excluded.error_count,
            aborted_count = excluded.aborted_count,
            user_prompt_count = excluded.user_prompt_count,
            subagent_count = excluded.subagent_count,
            tool_calls_json = excluded.tool_calls_json,
            models_json = excluded.models_json,
            sources_json = excluded.sources_json,
            extensions_json = excluded.extensions_json,
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
            record.user_prompt_count,
            record.subagent_count,
            record.tool_calls_json,
            record.models_json,
            record.sources_json,
            record.extensions_json,
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
                   user_prompt_count, subagent_count, tool_calls_json, models_json, sources_json,
                   extensions_json, first_seen_at, last_seen_at, files_touched
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
                user_prompt_count: row.get(7)?,
                subagent_count: row.get(8)?,
                tool_calls_json: row.get(9)?,
                models_json: row.get(10)?,
                sources_json: row.get(11)?,
                extensions_json: row.get(12)?,
                first_seen_at: row.get(13)?,
                last_seen_at: row.get(14)?,
                files_touched: row.get(15)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

pub fn load_cursor_session(
    conn: &Connection,
    source_file: &str,
) -> Result<Option<CursorSessionRecord>, String> {
    conn.query_row(
        r#"
        SELECT source_file, session_id, project, turn_count, success_count, error_count, aborted_count,
               user_prompt_count, subagent_count, tool_calls_json, models_json, sources_json,
               extensions_json, first_seen_at, last_seen_at, files_touched
        FROM cursor_sessions
        WHERE source_file = ?1
        "#,
        params![source_file],
        |row| {
            Ok(CursorSessionRecord {
                source_file: row.get(0)?,
                session_id: row.get(1)?,
                project: row.get(2)?,
                turn_count: row.get(3)?,
                success_count: row.get(4)?,
                error_count: row.get(5)?,
                aborted_count: row.get(6)?,
                user_prompt_count: row.get(7)?,
                subagent_count: row.get(8)?,
                tool_calls_json: row.get(9)?,
                models_json: row.get(10)?,
                sources_json: row.get(11)?,
                extensions_json: row.get(12)?,
                first_seen_at: row.get(13)?,
                last_seen_at: row.get(14)?,
                files_touched: row.get(15)?,
            })
        },
    )
    .optional()
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
        removed += 1;
    }
    Ok(removed)
}

pub fn reconcile_cursor_session_files(
    conn: &Connection,
    seen_paths: &BTreeSet<String>,
) -> Result<u64, String> {
    let cached: Vec<String> = conn
        .prepare("SELECT path FROM cursor_session_files")
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
            "DELETE FROM cursor_session_files WHERE path = ?1",
            params![path],
        )
        .map_err(|e| e.to_string())?;
        removed += 1;
    }
    Ok(removed)
}

pub const CURSOR_SESSION_SCHEMA_VERSION: &str = "2";

pub fn cursor_session_schema_version(conn: &Connection) -> Result<String, String> {
    conn.query_row(
        "SELECT value FROM cursor_session_meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )
    .optional()
    .map(|value| value.unwrap_or_default())
    .map_err(|e| e.to_string())
}

pub fn set_cursor_session_schema_version(conn: &Connection, version: &str) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO cursor_session_meta(key, value) VALUES('schema_version', ?1)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
        params![version],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
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
