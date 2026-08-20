use crate::test_support::*;

fn seed_codex_conversation(home: &std::path::Path) -> std::path::PathBuf {
    seed_codex_fixture(home, "rollout-conv-1.jsonl", "codex-conversation.jsonl")
}

fn seed_codex_fixture(
    home: &std::path::Path,
    file_name: &str,
    fixture_name: &str,
) -> std::path::PathBuf {
    let path = home.join(".codex/sessions/2026/08").join(file_name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, fixture(fixture_name)).unwrap();
    path
}

#[test]
fn codex_conversation_detail_merges_streamed_text_and_filters_protocol_noise() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_fixture(
        home,
        "rollout-semantic-1.jsonl",
        "codex-semantic-events.jsonl",
    );
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let detail = crate::conversation::load_detail(&conn, home, "codex", "semantic-1").unwrap();

    let message_events = detail
        .events
        .iter()
        .filter(|event| event.kind == ConversationEventKind::Message)
        .collect::<Vec<_>>();
    assert_eq!(message_events.len(), 2);
    assert_eq!(message_events[0].actor, Some(ConversationEventActor::User));
    assert_eq!(message_events[0].text.as_deref(), Some("实现语义时间线"));
    assert_eq!(
        message_events[1].actor,
        Some(ConversationEventActor::Assistant)
    );
    assert_eq!(
        message_events[1].text.as_deref(),
        Some("我先检查现有实现。")
    );
    assert_eq!(message_events[1].sequence, 3);
    assert!(detail
        .events
        .iter()
        .all(|event| { !matches!(event.name.as_deref(), Some("token_count" | "heartbeat")) }));
}

#[test]
fn codex_conversation_detail_deduplicates_final_messages_across_protocol_channels() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_fixture(
        home,
        "rollout-duplicates-1.jsonl",
        "codex-duplicate-messages.jsonl",
    );
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let detail = crate::conversation::load_detail(&conn, home, "codex", "duplicates-1").unwrap();
    let messages = detail
        .events
        .iter()
        .filter(|event| event.kind == ConversationEventKind::Message)
        .map(|event| {
            (
                event.actor.map(ConversationEventActor::as_str),
                event.text.as_deref(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        messages,
        vec![
            (Some("user"), Some("同一条用户消息")),
            (Some("assistant"), Some("同一条助手消息")),
            (Some("user"), Some("同一条用户消息")),
        ]
    );
    assert_eq!(detail.messages.len(), 3);
}

#[test]
fn codex_conversation_detail_orders_by_timestamp_then_source_sequence() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_fixture(
        home,
        "rollout-ordered-1.jsonl",
        "codex-out-of-order-events.jsonl",
    );
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let detail = crate::conversation::load_detail(&conn, home, "codex", "ordered-1").unwrap();
    let order = detail
        .events
        .iter()
        .map(|event| (event.kind.as_str(), event.sequence))
        .collect::<Vec<_>>();

    assert_eq!(
        order,
        vec![
            ("system_status", 0),
            ("plan", 2),
            ("error", 1),
            ("unadapted", 3),
        ]
    );
    assert_eq!(detail.events[3].occurred_at, None);
}

#[test]
fn codex_conversation_detail_projects_semantic_events_and_preserves_unknown_events() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_fixture(
        home,
        "rollout-semantic-1.jsonl",
        "codex-semantic-events.jsonl",
    );
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let detail = crate::conversation::load_detail(&conn, home, "codex", "semantic-1").unwrap();
    let kinds = detail
        .events
        .iter()
        .map(|event| event.kind.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        kinds,
        vec![
            "system_status",
            "model_change",
            "message",
            "message",
            "plan",
            "tool_call",
            "tool_result",
            "model_change",
            "error",
            "unadapted",
        ]
    );
    assert!(detail
        .events
        .windows(2)
        .all(|pair| pair[0].sequence < pair[1].sequence));
    let plan = &detail.events[4];
    assert_eq!(plan.text.as_deref(), Some("按层实现"));
    assert_eq!(plan.details["plan"][0]["step"], "后端事件投影");
    let call = &detail.events[5];
    assert_eq!(call.name.as_deref(), Some("read_file"));
    assert_eq!(call.details["call_id"], "call-1");
    assert_eq!(detail.events[6].text.as_deref(), Some("fn main() {}"));
    assert_eq!(detail.events[7].name.as_deref(), Some("gpt-5.7-codex"));
    assert_eq!(detail.events[8].text.as_deref(), Some("工具执行失败"));
    let unknown = &detail.events[9];
    assert_eq!(unknown.name.as_deref(), Some("future_event"));
    assert_eq!(unknown.occurred_at, None);
    assert_eq!(
        unknown.capability_status,
        ConversationEventCapabilityStatus::UnadaptedMissingTimestamp
    );
    assert_eq!(unknown.details["payload"]["phase"], "next");
}

#[test]
fn codex_conversation_detail_links_existing_usage_by_exact_source_and_session_id() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_fixture(
        home,
        "rollout-semantic-1.jsonl",
        "codex-semantic-events.jsonl",
    );
    let conn = store::open_memory().unwrap();
    let mut early = rec(
        "2026-08-21T00:00:05Z",
        Source::Codex,
        "gpt-5.6-sol",
        "openai",
        "/workspace/semantic-project",
        "semantic-1",
        110,
    );
    early.output_tokens = 10;
    let late = rec(
        "2026-08-21T00:01:00Z",
        Source::Codex,
        "gpt-5.7-codex",
        "openai",
        "/workspace/semantic-project",
        "semantic-1",
        220,
    );
    let wrong_source = rec(
        "2026-08-21T00:02:00Z",
        Source::Claude,
        "claude-sonnet-5",
        "anthropic",
        "/workspace/semantic-project",
        "semantic-1",
        330,
    );
    let wrong_session = rec(
        "2026-08-21T00:03:00Z",
        Source::Codex,
        "gpt-5.7-codex",
        "openai",
        "/workspace/semantic-project",
        "semantic-2",
        440,
    );
    store::insert_records(&conn, &[late, wrong_source, early, wrong_session]).unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let detail = crate::conversation::load_detail(&conn, home, "codex", "semantic-1").unwrap();

    assert_eq!(detail.usage_records.len(), 2);
    assert_eq!(detail.usage_records[0].occurred_at, "2026-08-21T00:00:05Z");
    assert_eq!(detail.usage_records[0].output_tokens, 10);
    assert_eq!(detail.usage_records[1].occurred_at, "2026-08-21T00:01:00Z");
    assert!(detail
        .usage_records
        .iter()
        .all(|record| record.source == Source::Codex && record.session_id == "semantic-1"));
}

#[test]
fn codex_conversation_catalog_indexes_and_loads_messages_without_caching_body() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let source_file = seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();

    assert_eq!(page.total, 1);
    assert_eq!(page.rows.len(), 1);
    let row = &page.rows[0];
    assert_eq!(row.source, "codex");
    assert_eq!(row.session_id, "conv-1");
    assert_eq!(row.title, "发布 Tray 客户端版本支持图片编辑透传");
    assert_eq!(row.project, "/workspace/example-project");
    assert_eq!(row.model, "gpt-5.6-sol");
    assert_eq!(row.started_at, "2026-08-20T00:00:00Z");
    assert_eq!(row.ended_at, "2026-08-20T00:03:00Z");
    assert_eq!(row.capabilities, vec!["messages", "events", "usage"]);
    assert_eq!(row.support_status, "experimental");
    assert!(row.file_available);

    let detail = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap();
    assert_eq!(detail.session, *row);
    assert_eq!(detail.messages.len(), 3);
    assert_eq!(detail.messages[0].role, "user");
    assert_eq!(
        detail.messages[0].text,
        "发布 Tray 客户端版本支持图片编辑透传"
    );
    assert_eq!(detail.messages[1].role, "assistant");
    assert_eq!(detail.messages[1].text, "我先检查现有实现。");
    assert_eq!(detail.messages[2].text, "已完成提交。");

    std::fs::remove_file(source_file).unwrap();
    let error = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap_err();
    assert!(error.contains("原文件已删除"), "unexpected error: {error}");
    assert!(error.contains("详情不可读取"), "unexpected error: {error}");
}

#[test]
fn codex_conversation_catalog_searches_only_indexed_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();

    for search in [
        "Tray",
        "codex",
        "example-project",
        "gpt-5.6-sol",
        "conv-1",
        "2026-08-20",
    ] {
        let page = crate::conversation::sessions_page(
            &conn,
            &crate::domain::ConversationQuery {
                search: Some(search.to_string()),
                page: Some(1),
                page_size: Some(10),
            },
        )
        .unwrap();
        assert_eq!(page.total, 1, "search should match: {search}");
    }

    let missing = crate::conversation::sessions_page(
        &conn,
        &crate::domain::ConversationQuery {
            search: Some("我先检查现有实现".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(missing.total, 0, "正文不应进入元数据搜索索引");
}

#[test]
fn codex_conversation_refresh_tombstones_deleted_files_and_revives_the_same_session() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let first = seed_codex_conversation(home);
    let second = home.join(".codex/sessions/2026/08/rollout-conv-2.jsonl");
    std::fs::write(
        &second,
        fixture("codex-conversation.jsonl").replace("conv-1", "conv-2"),
    )
    .unwrap();
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    assert_eq!(
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap()
            .total,
        2
    );

    std::fs::remove_file(&second).unwrap();
    let issues = crate::conversation::refresh_codex(&conn, home).unwrap();
    assert!(issues.is_empty());
    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    assert_eq!(page.total, 2, "删除源文件后必须保留目录索引");
    let deleted = page
        .rows
        .iter()
        .find(|row| row.session_id == "conv-2")
        .unwrap();
    assert!(!deleted.file_available);
    let error = crate::conversation::load_detail(&conn, home, "codex", "conv-2").unwrap_err();
    assert!(error.contains("原文件已删除"), "unexpected error: {error}");
    assert!(error.contains("详情不可读取"), "unexpected error: {error}");

    std::fs::write(
        &second,
        fixture("codex-conversation.jsonl").replace("conv-1", "conv-2"),
    )
    .unwrap();
    assert!(crate::conversation::refresh_codex(&conn, home)
        .unwrap()
        .is_empty());
    let revived =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    assert_eq!(revived.total, 2, "恢复原路径不得生成重复目录项");
    let revived_row = revived
        .rows
        .iter()
        .find(|row| row.session_id == "conv-2")
        .unwrap();
    assert!(revived_row.file_available);
    crate::conversation::load_detail(&conn, home, "codex", "conv-2").unwrap();

    assert!(first.exists());
}

#[test]
fn codex_conversation_refresh_skips_unchanged_available_files() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    conn.execute(
        "UPDATE conversation_sessions SET title = 'cached-title' WHERE source = 'codex' AND session_id = 'conv-1'",
        [],
    )
    .unwrap();

    assert!(crate::conversation::refresh_codex(&conn, home)
        .unwrap()
        .is_empty());
    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    assert_eq!(page.rows[0].title, "cached-title");
}

#[test]
fn codex_conversation_parse_failure_preserves_metadata_and_reports_safe_location() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let path = seed_codex_conversation(home);
    let removed = home.join(".codex/sessions/2026/08/rollout-conv-2.jsonl");
    std::fs::write(
        &removed,
        fixture("codex-conversation.jsonl").replace("conv-1", "conv-2"),
    )
    .unwrap();
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    let before =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap()
            .rows[0]
            .clone();
    let secret = "PRIVATE_PROMPT_MUST_NOT_APPEAR";
    std::fs::write(
        &path,
        format!(
            "{}\n{{\"secret\":\"{secret}\"",
            fixture("codex-conversation.jsonl").trim_end()
        ),
    )
    .unwrap();
    std::fs::remove_file(removed).unwrap();

    let report = ingest::ingest_all_with_overrides(&conn, home, &Default::default()).unwrap();

    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    assert_eq!(page.total, 2, "解析失败时不得执行墓碑对账");
    let after = page
        .rows
        .iter()
        .find(|row| row.session_id == "conv-1")
        .unwrap();
    assert_eq!(after.title, before.title);
    assert_eq!(after.project, before.project);
    assert_eq!(after.model, before.model);
    assert!(
        page.rows
            .iter()
            .find(|row| row.session_id == "conv-2")
            .unwrap()
            .file_available
    );
    assert_eq!(report.conversation_issues.len(), 1);
    let issue = serde_json::to_value(&report.conversation_issues[0]).unwrap();
    assert_eq!(issue["event_type"], "json_line");
    assert_eq!(issue["line"], 8);
    assert!(!issue["message"].as_str().unwrap().contains(secret));
    assert!(!issue.to_string().contains(secret));
}

#[test]
fn conversation_schema_migrates_lifecycle_columns_for_old_caches() {
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().join("old-cache.sqlite");
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE conversation_sessions (
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
            INSERT INTO conversation_sessions(
                source, session_id, title, project, model, started_at, ended_at, source_file
            ) VALUES('codex', 'legacy', '旧索引', '', '', '', '', 'legacy.jsonl');
            "#,
        )
        .unwrap();
    }

    let conn = store::open_db(db_path.to_str().unwrap()).unwrap();
    let lifecycle = conn
        .query_row(
            "SELECT file_available, source_file_mtime_ms, source_file_size FROM conversation_sessions WHERE session_id = 'legacy'",
            [],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
        )
        .unwrap();
    assert_eq!(lifecycle, (1, 0, 0));
}

#[test]
fn codex_conversation_detail_rejects_indexed_path_outside_source_root() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_conversation(home);
    let outside = home.join("outside.jsonl");
    std::fs::write(&outside, fixture("codex-conversation.jsonl")).unwrap();
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    conn.execute(
        "UPDATE conversation_sessions SET source_file = ?1 WHERE source = 'codex' AND session_id = 'conv-1'",
        rusqlite::params![outside.to_string_lossy().to_string()],
    )
    .unwrap();

    let error = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap_err();
    assert!(
        error.contains("允许的扫描目录"),
        "unexpected error: {error}"
    );
}

#[test]
fn ingest_all_refreshes_codex_conversation_catalog_without_usage_records() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();

    let report = ingest::ingest_all_with_overrides(&conn, home, &Default::default()).unwrap();
    assert_eq!(report.files_failed, 0);
    let records = store::load_all(&conn).unwrap();
    assert!(records.is_empty(), "unexpected usage records: {records:?}");

    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].title, "发布 Tray 客户端版本支持图片编辑透传");
}

#[test]
fn conversation_index_issues_do_not_change_usage_ingest_failure_counts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let path = home.join(".codex/sessions/missing-id.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "{}\n").unwrap();
    let conn = store::open_memory().unwrap();

    let report = ingest::ingest_all_with_overrides(&conn, home, &Default::default()).unwrap();

    assert_eq!(report.files_failed, 0);
    assert!(report.issues.is_empty());
    assert!(report.partial_success);
    assert_eq!(report.conversation_issues.len(), 1);
    assert_eq!(report.conversation_issues[0].source, "codex");
    assert_eq!(
        std::path::PathBuf::from(&report.conversation_issues[0].path),
        path
    );
    assert!(report.conversation_issues[0].message.contains("会话 ID"));
    let issue = serde_json::to_value(&report.conversation_issues[0]).unwrap();
    assert_eq!(issue["event_type"], "session_meta");
    assert!(issue["line"].is_null());
    assert!(!issue.to_string().contains("{}"));
}
