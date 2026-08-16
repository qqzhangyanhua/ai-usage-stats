use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::UsageRecord;

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

        CREATE TABLE IF NOT EXISTS ingested_files (
            path TEXT PRIMARY KEY,
            mtime_ms INTEGER NOT NULL,
            size INTEGER NOT NULL
        );
        "#,
    )
    .map_err(|e| e.to_string())
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

pub fn delete_records_for_file(conn: &Connection, source_file: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM usage_records WHERE source_file = ?1",
        params![source_file],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn file_unchanged(conn: &Connection, path: &str, mtime_ms: i64, size: i64) -> Result<bool, String> {
    let row: Option<(i64, i64)> = conn
        .query_row(
            "SELECT mtime_ms, size FROM ingested_files WHERE path = ?1",
            params![path],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(matches!(row, Some((m, s)) if m == mtime_ms && s == size))
}

pub fn mark_file(conn: &Connection, path: &str, mtime_ms: i64, size: i64) -> Result<(), String> {
    conn.execute(
        r#"
        INSERT INTO ingested_files(path, mtime_ms, size) VALUES(?1,?2,?3)
        ON CONFLICT(path) DO UPDATE SET mtime_ms = excluded.mtime_ms, size = excluded.size
        "#,
        params![path, mtime_ms, size],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
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
            Ok(UsageRecord {
                occurred_at: row.get(0)?,
                source: crate::domain::Source::parse(&row.get::<_, String>(1)?).unwrap_or(crate::domain::Source::Codex),
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
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}
