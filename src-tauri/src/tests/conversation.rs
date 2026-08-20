use crate::test_support::*;

fn seed_codex_conversation(home: &std::path::Path) -> std::path::PathBuf {
    let path = home.join(".codex/sessions/2026/08/rollout-conv-1.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, fixture("codex-conversation.jsonl")).unwrap();
    path
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
    assert_eq!(row.capabilities, vec!["messages"]);
    assert_eq!(row.support_status, "experimental");

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
    assert!(error.contains("原始文件"), "unexpected error: {error}");
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
fn codex_conversation_refresh_reconciles_deleted_files_after_a_clean_scan() {
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

    std::fs::write(&first, "{not-json").unwrap();
    std::fs::remove_file(&second).unwrap();
    let issues = crate::conversation::refresh_codex(&conn, home).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap()
            .total,
        2,
        "任一文件解析失败时应保留全部最后一次正确索引"
    );

    std::fs::write(&first, fixture("codex-conversation.jsonl")).unwrap();
    assert!(crate::conversation::refresh_codex(&conn, home)
        .unwrap()
        .is_empty());
    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].session_id, "conv-1");
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

    let report = ingest::ingest_all(&conn, home).unwrap();
    assert_eq!(report.files_failed, 0);
    assert!(store::load_all(&conn).unwrap().is_empty());

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

    let report = ingest::ingest_all(&conn, home).unwrap();

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
}
