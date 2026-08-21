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

fn seed_codex_records(
    home: &std::path::Path,
    file_name: &str,
    records: &[serde_json::Value],
) -> std::path::PathBuf {
    let path = home.join(".codex/sessions/2026/08").join(file_name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let content = records
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, format!("{content}\n")).unwrap();
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

    let full =
        crate::conversation::load_event_content(&conn, home, "codex", "rich-1", &event.event_id)
            .unwrap();
    assert_eq!(full.event_id, event.event_id);
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
    assert!(event.attachments[0].id.starts_with(&event.event_id));
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

    let thumbnail = crate::conversation::load_attachment_thumbnail(
        &conn,
        home,
        "codex",
        "rich-1",
        &event.attachments[0].id,
    )
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

    let image = crate::conversation::load_attachment(
        &conn,
        home,
        "codex",
        "rich-1",
        &event.attachments[0].id,
    )
    .unwrap();
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
    let detail = crate::conversation::load_detail(&conn, home, "codex", "rich-1").unwrap();
    let attachment_id = detail
        .events
        .iter()
        .flat_map(|event| &event.attachments)
        .find(|attachment| attachment.name == "outside.png")
        .unwrap()
        .id
        .clone();
    let error =
        crate::conversation::load_attachment(&conn, home, "codex", "rich-1", &attachment_id)
            .unwrap_err();

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
        .map(|event| (event.kind.as_str(), event.sequence, event.source_sequence))
        .collect::<Vec<_>>();

    assert_eq!(
        order,
        vec![
            ("system_status", 0, 0),
            ("plan", 1, 2),
            ("error", 2, 1),
            ("unadapted", 3, 3),
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
fn codex_conversation_merges_duplicate_session_files_in_stable_order() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let meta = serde_json::json!({
        "type": "session_meta",
        "timestamp": "2026-08-21T00:00:00Z",
        "payload": {"id": "split-1", "cwd": "/workspace/split", "title": "Split session"}
    });
    let duplicate = serde_json::json!({
        "type": "response_item",
        "timestamp": "2026-08-21T00:00:02Z",
        "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "shared"}]}
    });
    seed_codex_records(
        home,
        "rollout-split-a.jsonl",
        &[
            meta.clone(),
            duplicate.clone(),
            serde_json::json!({
                "type": "response_item",
                "timestamp": "2026-08-21T00:00:04Z",
                "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "late"}]}
            }),
        ],
    );
    seed_codex_records(
        home,
        "rollout-split-b.jsonl",
        &[
            meta,
            serde_json::json!({
                "type": "response_item",
                "timestamp": "2026-08-21T00:00:01Z",
                "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "early"}]}
            }),
            duplicate,
        ],
    );
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    let detail = crate::conversation::load_detail(&conn, home, "codex", "split-1").unwrap();

    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].source_files.len(), 2);
    assert_eq!(detail.session.source_files, page.rows[0].source_files);
    assert_eq!(
        std::path::Path::new(&page.rows[0].source_file)
            .file_name()
            .and_then(|name| name.to_str()),
        Some("rollout-split-a.jsonl")
    );
    let texts = detail
        .events
        .iter()
        .filter_map(|event| event.text.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(texts, vec!["early", "shared", "late"]);
    assert!(detail
        .events
        .windows(2)
        .all(|pair| pair[0].sequence < pair[1].sequence));
    let event_ids = detail
        .events
        .iter()
        .map(|event| event.event_id.clone())
        .collect::<Vec<_>>();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    let refreshed = crate::conversation::load_detail(&conn, home, "codex", "split-1").unwrap();
    assert_eq!(
        refreshed
            .events
            .iter()
            .map(|event| event.event_id.clone())
            .collect::<Vec<_>>(),
        event_ids
    );
    assert!(crate::conversation::build_export(
        &conn,
        home,
        "codex",
        "split-1",
        crate::domain::ConversationExportFormat::Json,
    )
    .unwrap_err()
    .contains("多个原始文件"));
}

#[test]
fn codex_conversation_parse_failure_preserves_the_last_good_multi_file_aggregate() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let meta = serde_json::json!({
        "type": "session_meta",
        "timestamp": "2026-08-21T00:00:00Z",
        "payload": {"id": "last-good-1", "cwd": "/workspace/last-good"}
    });
    let first_path = seed_codex_records(
        home,
        "rollout-last-good-a.jsonl",
        &[
            meta.clone(),
            serde_json::json!({
                "type": "response_item",
                "timestamp": "2026-08-21T00:00:01Z",
                "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "first"}]}
            }),
        ],
    );
    let second_path = seed_codex_records(
        home,
        "rollout-last-good-b.jsonl",
        &[
            meta,
            serde_json::json!({
                "type": "response_item",
                "timestamp": "2026-08-21T00:00:02Z",
                "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "second"}]}
            }),
        ],
    );
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();

    let mut first = std::fs::read_to_string(&first_path).unwrap();
    first.push_str(
        &serde_json::json!({
            "type": "response_item",
            "timestamp": "2026-08-21T00:00:09Z",
            "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "partial update"}]}
        })
        .to_string(),
    );
    first.push('\n');
    std::fs::write(first_path, first).unwrap();
    std::fs::write(second_path, "{not-json}\n").unwrap();

    let issues = crate::conversation::refresh_codex(&conn, home).unwrap();
    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();

    assert_eq!(issues.len(), 1);
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].ended_at, "2026-08-21T00:00:02Z");
    assert_eq!(page.rows[0].source_files.len(), 2);
}

#[test]
fn codex_conversation_links_structured_child_agents_and_preserves_launch_events() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_records(
        home,
        "rollout-parent.jsonl",
        &[
            serde_json::json!({
                "type": "session_meta",
                "timestamp": "2026-08-21T00:00:00Z",
                "payload": {"id": "parent-1", "cwd": "/workspace/agents", "title": "Parent"}
            }),
            serde_json::json!({
                "type": "response_item",
                "timestamp": "2026-08-21T00:00:01Z",
                "payload": {"type": "function_call", "name": "spawn_agent", "call_id": "spawn-1", "arguments": "{\"message\":\"Inspect child work\"}"}
            }),
            serde_json::json!({
                "type": "response_item",
                "timestamp": "2026-08-21T00:00:02Z",
                "payload": {"type": "function_call_output", "call_id": "spawn-1", "output": "{\"agent_id\":\"child-1\"}"}
            }),
        ],
    );
    seed_codex_records(
        home,
        "rollout-child.jsonl",
        &[
            serde_json::json!({
                "type": "session_meta",
                "timestamp": "2026-08-21T00:00:03Z",
                "payload": {"id": "child-1", "parent_id": "parent-1", "cwd": "/workspace/agents", "title": "Child"}
            }),
            serde_json::json!({
                "type": "response_item",
                "timestamp": "2026-08-21T00:00:04Z",
                "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "child result"}]}
            }),
        ],
    );
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let parent = crate::conversation::load_detail(&conn, home, "codex", "parent-1").unwrap();
    let child = crate::conversation::load_detail(&conn, home, "codex", "child-1").unwrap();

    let launch = parent
        .events
        .iter()
        .find(|event| event.name.as_deref() == Some("spawn_agent"))
        .unwrap();
    assert_eq!(parent.agent_relations.children.len(), 1);
    let child_link = &parent.agent_relations.children[0];
    assert_eq!(
        child_link.status,
        crate::domain::ConversationAgentLinkStatus::Linked
    );
    assert_eq!(
        child_link.launch_event_id.as_deref(),
        Some(launch.event_id.as_str())
    );
    assert_eq!(child_link.session.as_ref().unwrap().session_id, "child-1");
    assert_eq!(
        child.events.last().unwrap().text.as_deref(),
        Some("child result")
    );
    assert_eq!(
        child
            .agent_relations
            .parent
            .as_ref()
            .and_then(|link| link.session.as_ref())
            .map(|session| session.session_id.as_str()),
        Some("parent-1")
    );
}

#[test]
fn codex_conversation_rejects_fuzzy_child_merging_and_reports_unavailable_linkage() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_records(
        home,
        "rollout-unresolved-parent.jsonl",
        &[
            serde_json::json!({
                "type": "session_meta",
                "timestamp": "2026-08-21T00:00:00Z",
                "payload": {"id": "unresolved-parent", "cwd": "/workspace/same", "title": "Same title"}
            }),
            serde_json::json!({
                "type": "response_item",
                "timestamp": "2026-08-21T00:00:01Z",
                "payload": {"type": "function_call", "name": "spawn_agent", "call_id": "spawn-plain", "arguments": "{}"}
            }),
            serde_json::json!({
                "type": "response_item",
                "timestamp": "2026-08-21T00:00:02Z",
                "payload": {"type": "function_call_output", "call_id": "spawn-plain", "output": "agent_id: possible-child"}
            }),
        ],
    );
    seed_codex_records(
        home,
        "rollout-possible-child.jsonl",
        &[serde_json::json!({
            "type": "session_meta",
            "timestamp": "2026-08-21T00:00:02Z",
            "payload": {"id": "possible-child", "cwd": "/workspace/same", "title": "Same title"}
        })],
    );
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    let parent =
        crate::conversation::load_detail(&conn, home, "codex", "unresolved-parent").unwrap();
    let candidate =
        crate::conversation::load_detail(&conn, home, "codex", "possible-child").unwrap();

    assert_eq!(page.total, 2);
    assert_eq!(parent.agent_relations.children.len(), 1);
    assert_eq!(
        parent.agent_relations.capability_status,
        crate::domain::ConversationAgentCapabilityStatus::Unavailable
    );
    assert_eq!(
        parent.agent_relations.children[0].status,
        crate::domain::ConversationAgentLinkStatus::Unresolved
    );
    assert!(parent.agent_relations.children[0].session.is_none());
    assert!(candidate.agent_relations.parent.is_none());
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
    let mut early_copy = early.clone();
    early_copy.source_file = "duplicate-channel.jsonl".to_string();
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
    store::insert_records(
        &conn,
        &[late, wrong_source, early_copy, early, wrong_session],
    )
    .unwrap();

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
