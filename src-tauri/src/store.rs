use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{CursorUsageEvent, Source, UsageRecord};

pub const ADAPTER_VERSION: i64 = 6;

pub fn open_db(path: &str) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    init_schema(&conn)?;
    Ok(conn)
}

pub fn open_memory() -> Result<Connection, String> {
    let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    init_schema(&conn)?;
    Ok(conn)
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

    let mut removed = 0;
    for path in cached {
        if !seen_paths.contains(&path) {
            removed += delete_records_for_file(conn, &path)?;
            conn.execute("DELETE FROM ingested_files WHERE path = ?1", params![path])
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(removed)
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

pub fn source_cache_stats(conn: &Connection, source: Source) -> Result<(u64, u64, i64), String> {
    let cached_files = conn
        .query_row(
            "SELECT COUNT(*) FROM ingested_files WHERE source = ?1",
            params![source.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| e.to_string())? as u64;
    let (record_count, total_tokens) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0) FROM usage_records WHERE source = ?1",
            params![source.as_str()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(|e| e.to_string())?;
    Ok((cached_files, record_count as u64, total_tokens))
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
