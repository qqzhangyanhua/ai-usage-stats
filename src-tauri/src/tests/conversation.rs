use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use crate::test_support::*;

fn test_png_bytes() -> Vec<u8> {
    let pixels = image::RgbaImage::from_pixel(2, 2, image::Rgba([24, 160, 200, 255]));
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();
    output.into_inner()
}

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

fn seed_rich_codex_conversation(
    home: &std::path::Path,
) -> (std::path::PathBuf, String, std::path::PathBuf) {
    let attachment = home.join("attachments/screenshot.png");
    std::fs::create_dir_all(attachment.parent().unwrap()).unwrap();
    std::fs::write(&attachment, test_png_bytes()).unwrap();
    let missing = home.join("attachments/missing.pdf");
    let large_output = format!("{}FULL-END", "large tool output\n".repeat(400));
    let records = [
        serde_json::json!({
            "type": "session_meta",
            "timestamp": "2026-08-24T00:00:00Z",
            "payload": {"id": "rich-1", "cwd": home, "title": "富内容会话"}
        }),
        serde_json::json!({
            "type": "response_item",
            "timestamp": "2026-08-24T00:00:01Z",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "查看附件"},
                    {"type": "input_image", "file_path": attachment, "name": "screenshot.png", "mime_type": "image/png"},
                    {"type": "input_file", "file_path": missing, "name": "missing.pdf", "mime_type": "application/pdf"}
                ]
            }
        }),
        serde_json::json!({
            "type": "response_item",
            "timestamp": "2026-08-24T00:00:02Z",
            "payload": {
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "# 结果\n\n|列|值|\n|-|-|\n|状态|完成|\n\n```rust\nfn main() {}\n```\n\n<iframe src=\"https://unsafe.invalid\"></iframe>\n\n[危险](javascript:alert(1))"
                }]
            }
        }),
        serde_json::json!({
            "type": "response_item",
            "timestamp": "2026-08-24T00:00:03Z",
            "payload": {"type": "function_call_output", "call_id": "call-rich", "output": large_output}
        }),
    ];
    let transcript = records
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let path = home.join(".codex/sessions/2026/08/rollout-rich-1.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{transcript}\n")).unwrap();
    (path, large_output, missing)
}

#[test]
fn codex_conversation_detail_defers_large_tool_results_until_requested() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (_, large_output, _) = seed_rich_codex_conversation(home);
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let detail = crate::conversation::load_detail(&conn, home, "codex", "rich-1").unwrap();
    let event = detail
        .events
        .iter()
        .find(|event| event.sequence == 3)
        .unwrap();

    assert_eq!(
        event.content_status,
        ConversationEventContentStatus::Deferred
    );
    assert!(event
        .text
        .as_ref()
        .unwrap()
        .starts_with("large tool output"));
    assert!(!event.text.as_ref().unwrap().contains("FULL-END"));
    assert!(event.details.get("output").is_none());

    let full = crate::conversation::load_event_content(&conn, home, "codex", "rich-1", 3).unwrap();
    assert_eq!(full.sequence, 3);
    assert_eq!(full.text.as_deref(), Some(large_output.as_str()));
    assert_eq!(full.details["output"], large_output);
}

#[test]
fn codex_conversation_detail_reports_attachments_and_loads_images_on_demand() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (_, _, missing_path) = seed_rich_codex_conversation(home);
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let detail = crate::conversation::load_detail(&conn, home, "codex", "rich-1").unwrap();
    let event = detail
        .events
        .iter()
        .find(|event| event.sequence == 1)
        .unwrap();

    assert!(
        event.details.get("content").is_none(),
        "message details must not eagerly return attachment bodies"
    );
    assert_eq!(event.attachments.len(), 2);
    assert_eq!(event.attachments[0].id, "1:0");
    assert_eq!(event.attachments[0].kind, ConversationAttachmentKind::Image);
    assert_eq!(
        event.attachments[0].status,
        ConversationAttachmentStatus::Available
    );
    assert_eq!(
        event.attachments[0].size_bytes,
        Some(test_png_bytes().len() as u64)
    );
    assert_eq!(event.attachments[1].name, "missing.pdf");
    assert_eq!(
        event.attachments[1].original_path,
        missing_path.to_string_lossy()
    );
    assert_eq!(
        event.attachments[1].status,
        ConversationAttachmentStatus::Missing
    );

    let thumbnail =
        crate::conversation::load_attachment_thumbnail(&conn, home, "codex", "rich-1", "1:0")
            .unwrap();
    assert_eq!(thumbnail.attachment, event.attachments[0]);
    let thumbnail_bytes = BASE64
        .decode(
            thumbnail
                .data_url
                .strip_prefix("data:image/png;base64,")
                .unwrap(),
        )
        .unwrap();
    let decoded_thumbnail = image::load_from_memory(&thumbnail_bytes).unwrap();
    assert_eq!(
        (decoded_thumbnail.width(), decoded_thumbnail.height()),
        (2, 2)
    );

    let image =
        crate::conversation::load_attachment(&conn, home, "codex", "rich-1", "1:0").unwrap();
    assert_eq!(image.attachment, event.attachments[0]);
    assert_eq!(
        image.data_url,
        format!("data:image/png;base64,{}", BASE64.encode(test_png_bytes()))
    );

    assert_eq!(detail.events.len(), 4, "缺失附件不应阻断其余事件");
}

#[test]
fn codex_conversation_attachment_loader_rejects_unrelated_source_siblings() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (source_path, _, _) = seed_rich_codex_conversation(home);
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let outside_image = source_path.parent().unwrap().join("unrelated.png");
    std::fs::write(&outside_image, test_png_bytes()).unwrap();
    let mut records = std::fs::read_to_string(&source_path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    records[0]["payload"]["cwd"] = serde_json::json!(project);
    records[1]["payload"]["content"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "type": "input_image",
            "file_path": outside_image,
            "name": "outside.png",
            "mime_type": "image/png"
        }));
    let content = records
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&source_path, format!("{content}\n")).unwrap();
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let error =
        crate::conversation::load_attachment(&conn, home, "codex", "rich-1", "1:2").unwrap_err();

    assert!(error.contains("允许的目录"), "unexpected error: {error}");
}

#[test]
fn codex_conversation_exports_markdown_and_raw_json_from_the_current_source_file() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let (source_path, _, missing_path) = seed_rich_codex_conversation(home);
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();

    let changed = std::fs::read_to_string(&source_path)
        .unwrap()
        .replace("# 结果", "# 导出后的结果");
    std::fs::write(&source_path, &changed).unwrap();

    let markdown = crate::conversation::build_export(
        &conn,
        home,
        "codex",
        "rich-1",
        ConversationExportFormat::Markdown,
    )
    .unwrap();
    assert_eq!(markdown.default_name, "富内容会话.md");
    let markdown_text = String::from_utf8(markdown.content.clone()).unwrap();
    assert!(markdown_text.contains("# 导出后的结果"));
    assert!(markdown_text.contains("FULL-END"));
    assert!(markdown_text.contains(&missing_path.to_string_lossy().to_string()));
    assert!(markdown_text.contains("附件缺失"));
    let markdown_path = home.join("exported.md");
    crate::user_files::write_export(&markdown_path, &markdown.content, None).unwrap();
    assert_eq!(std::fs::read(&markdown_path).unwrap(), markdown.content);
    let error =
        crate::user_files::write_export(&markdown_path, b"replacement export\n", None).unwrap_err();
    assert!(error.contains("已存在"));
    assert_eq!(std::fs::read(&markdown_path).unwrap(), markdown.content);
    let rejected_path = home.join("exported.txt");
    let error = crate::user_files::write_export(&rejected_path, b"not allowed", None).unwrap_err();
    assert!(error.contains("可写名单"));
    assert!(!rejected_path.exists());

    let raw_json = crate::conversation::build_export(
        &conn,
        home,
        "codex",
        "rich-1",
        ConversationExportFormat::Json,
    )
    .unwrap();
    assert_eq!(raw_json.default_name, "富内容会话.jsonl");
    assert_eq!(raw_json.content, changed.as_bytes());
    assert!(String::from_utf8_lossy(&raw_json.content).contains("FULL-END"));
    let json_path = home.join("exported.jsonl");
    crate::user_files::write_export(&json_path, &raw_json.content, None).unwrap();
    assert_eq!(std::fs::read(json_path).unwrap(), changed.as_bytes());
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
}
