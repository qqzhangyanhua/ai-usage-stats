use crate::test_support::*;

#[test]
fn opening_legacy_cache_adds_trusted_ingest_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite");
    let legacy = rusqlite::Connection::open(&path).unwrap();
    legacy
        .execute_batch(
            r#"
            CREATE TABLE usage_records (
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
            CREATE TABLE ingested_files (
                path TEXT PRIMARY KEY,
                mtime_ms INTEGER NOT NULL,
                size INTEGER NOT NULL
            );
            INSERT INTO usage_records (
                occurred_at, source, model, provider, project, session_id, source_file,
                input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                reasoning_tokens, total_tokens, native_cost
            ) VALUES ('2026-01-01T00:00:00Z', 'codex', '', '', '', 's', '/one.jsonl', 1, 0, 0, 0, 0, 1, NULL);
            INSERT INTO ingested_files(path, mtime_ms, size) VALUES('/one.jsonl', 1, 1);
            "#,
        )
        .unwrap();
    drop(legacy);

    let conn = store::open_db(path.to_string_lossy().as_ref()).unwrap();
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(ingested_files)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let source: String = conn
        .query_row(
            "SELECT source FROM ingested_files WHERE path = '/one.jsonl'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(columns.contains(&"fingerprint".to_string()));
    assert!(columns.contains(&"adapter_version".to_string()));
    assert_eq!(source, "codex");
}

#[test]
fn usage_records_source_file_operations_use_an_index() {
    let conn = store::open_memory().unwrap();
    for sql in [
        "EXPLAIN QUERY PLAN SELECT COUNT(*) FROM usage_records WHERE source_file = ?1",
        "EXPLAIN QUERY PLAN DELETE FROM usage_records WHERE source_file = ?1",
    ] {
        let plan: Vec<String> = conn
            .prepare(sql)
            .unwrap()
            .query_map(["/one.jsonl"], |row| row.get(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(
            plan.iter().any(|detail| {
                detail.contains("USING")
                    && detail.contains("INDEX")
                    && detail.contains("source_file")
            }),
            "source_file operation must use an index, query plan: {plan:?}"
        );
    }
}

#[test]
fn reconcile_source_lookup_uses_an_index() {
    let conn = store::open_memory().unwrap();
    let plan: Vec<String> = conn
        .prepare("EXPLAIN QUERY PLAN SELECT path FROM ingested_files WHERE source = ?1")
        .unwrap()
        .query_map(["codex"], |row| row.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        plan.iter()
            .any(|detail| detail.contains("USING") && detail.contains("INDEX")),
        "ingested_files(source) lookup must use an index, query plan: {plan:?}"
    );
}

#[test]
fn source_and_occurred_at_filter_uses_composite_index() {
    let conn = store::open_memory().unwrap();
    let plan: Vec<String> = conn
        .prepare(
            "EXPLAIN QUERY PLAN SELECT COUNT(*) FROM usage_records \
             WHERE source = ?1 AND occurred_at >= ?2",
        )
        .unwrap()
        .query_map(["codex", "2026-01-01"], |row| row.get(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(
        plan.iter().any(|detail| {
            detail.contains("USING") && detail.contains("INDEX") && detail.contains("source")
        }),
        "combined source+occurred_at filter must use an index, query plan: {plan:?}"
    );
}

#[test]
fn open_db_enables_wal_and_normal_synchronous() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usage.sqlite");
    let conn = store::open_db(path.to_str().unwrap()).unwrap();
    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode.to_lowercase(), "wal");
    let synchronous: i64 = conn
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();
    assert_eq!(synchronous, 1, "synchronous should be NORMAL (1)");
}

#[test]
fn readonly_query_does_not_block_on_open_write_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("usage.sqlite");
    let path = path.to_str().unwrap();
    let write = store::open_db(path).unwrap();
    store::insert_records(
        &write,
        &[rec(
            "2026-01-01T00:00:00+00:00",
            Source::Codex,
            "gpt",
            "openai",
            "demo",
            "s1",
            10,
        )],
    )
    .unwrap();
    let tx = write.unchecked_transaction().unwrap();
    store::insert_records(
        &tx,
        &[rec(
            "2026-01-02T00:00:00+00:00",
            Source::Codex,
            "gpt",
            "openai",
            "demo",
            "s2",
            20,
        )],
    )
    .unwrap();

    let started = std::time::Instant::now();
    let read = store::open_readonly(path).unwrap();
    let count: i64 = read
        .query_row("SELECT COUNT(*) FROM usage_records", [], |row| row.get(0))
        .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "readonly query must not wait for the uncommitted writer"
    );
    assert_eq!(
        count, 1,
        "reader should see last committed snapshot, not the open txn"
    );
    tx.commit().unwrap();
    let count: i64 = read
        .query_row("SELECT COUNT(*) FROM usage_records", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 2);
}
