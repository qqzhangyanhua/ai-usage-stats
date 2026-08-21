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
fn conversation_detail_prepared_context_loads_after_connection_is_dropped() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();

    let prepared = crate::conversation::prepare_detail(&conn, "codex", "conv-1").unwrap();
    drop(conn);

    let detail = crate::conversation::load_prepared_detail(home, prepared).unwrap();
    assert_eq!(detail.session.session_id, "conv-1");
    assert!(!detail.messages.is_empty());
}

#[test]
fn conversation_detail_consistent_snapshot_stops_after_three_changed_attempts() {
    use std::cell::Cell;
    use std::collections::VecDeque;

    let revisions = std::cell::RefCell::new(VecDeque::from([
        "before-1", "after-1", "before-2", "after-2", "before-3", "after-3",
    ]));
    let attempts = Cell::new(0);
    let error = crate::conversation::read_consistent_snapshot(
        || Ok(revisions.borrow_mut().pop_front().unwrap().to_string()),
        || {
            attempts.set(attempts.get() + 1);
            Err::<(), _>("JSON EOF".to_string())
        },
    )
    .unwrap_err();

    assert_eq!(attempts.get(), 3);
    assert!(error.contains("持续变化"), "unexpected error: {error}");
}

#[test]
fn conversation_detail_file_revision_maps_canonicalize_and_metadata_not_found_to_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".codex/sessions");
    std::fs::create_dir_all(&root).unwrap();
    let canonical_root = std::fs::canonicalize(&root).unwrap();

    let missing_during_canonicalize = crate::conversation::checked_detail_file_revision(
        std::slice::from_ref(&root),
        || Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
        |_| Ok("unused".to_string()),
    )
    .unwrap();
    assert_eq!(missing_during_canonicalize, None);

    let missing_during_metadata = crate::conversation::checked_detail_file_revision(
        std::slice::from_ref(&root),
        || Ok(canonical_root.clone()),
        |_| Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
    )
    .unwrap();
    assert_eq!(missing_during_metadata, None);
}

#[test]
fn conversation_detail_revision_uses_modified_nanoseconds_and_size() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let path = seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();

    let detail = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap();
    let metadata = std::fs::metadata(path).unwrap();
    let modified_ns = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    assert_eq!(detail.revision, format!("{modified_ns}:{}", metadata.len()));
}

#[test]
fn conversation_detail_rejects_newline_terminated_invalid_json() {
    use std::io::Write;

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let path = seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    writeln!(
        std::fs::OpenOptions::new().append(true).open(path).unwrap(),
        "{{\"type\":"
    )
    .unwrap();

    let error = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap_err();
    assert!(error.contains("JSON 无效"), "unexpected error: {error}");
}

#[test]
fn conversation_detail_rejects_unterminated_trailing_json_syntax_error() {
    use std::io::Write;

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let path = seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    write!(
        std::fs::OpenOptions::new().append(true).open(path).unwrap(),
        "{{\"type\": nope}}"
    )
    .unwrap();

    let error = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap_err();
    assert!(error.contains("JSON 无效"), "unexpected error: {error}");
}

#[test]
fn conversation_detail_state_detects_append_delete_and_restore_without_refresh() {
    use std::io::Write;

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let path = seed_codex_conversation(home);
    let original = std::fs::read(&path).unwrap();
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let initial = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap();
    assert!(!initial.revision.is_empty());

    let unchanged =
        crate::conversation::detail_state(&conn, home, "codex", "conv-1", &initial.revision)
            .unwrap();
    assert_eq!(unchanged.revision, initial.revision);
    assert!(!unchanged.changed);
    assert!(unchanged.file_available);

    let initial_message_count = initial.messages.len();
    let initial_event_count = initial.events.len();
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap(),
        r#"{{"type":"response_item","timestamp":"2026-08-20T00:04:00Z","payload":{{"type":"message","role":"assistant","content":[{{"type":"output_text","text":"follow-up"}}]}}}}"#
    )
    .unwrap();

    let changed =
        crate::conversation::detail_state(&conn, home, "codex", "conv-1", &initial.revision)
            .unwrap();
    assert!(changed.changed);
    assert!(changed.file_available);
    assert_ne!(changed.revision, initial.revision);

    let updated = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap();
    assert_eq!(updated.messages.len(), initial_message_count + 1);
    assert_eq!(updated.events.len(), initial_event_count + 1);
    assert_eq!(updated.messages.last().unwrap().text, "follow-up");
    assert_eq!(updated.revision, changed.revision);

    std::fs::remove_file(&path).unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    let deleted =
        crate::conversation::detail_state(&conn, home, "codex", "conv-1", &updated.revision)
            .unwrap();
    assert!(!deleted.file_available);

    std::fs::write(&path, original).unwrap();
    let restored =
        crate::conversation::detail_state(&conn, home, "codex", "conv-1", &updated.revision)
            .unwrap();
    assert!(restored.file_available);
    assert!(restored.changed);

    let restored_detail = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap();
    assert!(restored_detail.session.file_available);
}

#[test]
fn conversation_detail_state_reads_metadata_without_parsing_body() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let path = seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let initial = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap();
    std::fs::write(&path, b"this is not valid jsonl").unwrap();

    let changed =
        crate::conversation::detail_state(&conn, home, "codex", "conv-1", &initial.revision)
            .unwrap();
    assert!(changed.changed);
    assert!(changed.file_available);

    let unchanged =
        crate::conversation::detail_state(&conn, home, "codex", "conv-1", &changed.revision)
            .unwrap();
    assert!(!unchanged.changed);
    assert!(unchanged.file_available);
}

#[test]
fn conversation_detail_state_tracks_an_incomplete_trailing_jsonl_line_until_completion() {
    use std::io::Write;

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let path = seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();

    crate::conversation::refresh_codex(&conn, home).unwrap();
    let initial = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap();
    let initial_message_count = initial.messages.len();
    let initial_event_count = initial.events.len();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(
        br#"{"type":"response_item","timestamp":"2026-08-20T00:04:00Z","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"stream"#,
    )
    .unwrap();
    file.flush().unwrap();

    let partial = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap();
    assert_eq!(partial.messages.len(), initial_message_count);
    assert_eq!(partial.events.len(), initial_event_count);
    assert_ne!(partial.revision, initial.revision);

    file.write_all(br#"ed"}]}}"#).unwrap();
    file.write_all(b"\n").unwrap();
    file.flush().unwrap();
    drop(file);

    let completed_state =
        crate::conversation::detail_state(&conn, home, "codex", "conv-1", &partial.revision)
            .unwrap();
    assert!(completed_state.changed);
    assert!(completed_state.file_available);

    let completed = crate::conversation::load_detail(&conn, home, "codex", "conv-1").unwrap();
    assert_eq!(completed.messages.len(), initial_message_count + 1);
    assert_eq!(completed.events.len(), initial_event_count + 1);
    assert_eq!(completed.messages.last().unwrap().text, "streamed");
    assert_eq!(completed.revision, completed_state.revision);
}

#[test]
fn conversation_detail_state_rejects_indexed_path_outside_source_root() {
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

    let error =
        crate::conversation::detail_state(&conn, home, "codex", "conv-1", "known").unwrap_err();
    assert!(
        error.contains("允许的扫描目录"),
        "unexpected error: {error}"
    );
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
fn codex_conversation_refresh_reparses_same_millisecond_nanosecond_change() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    conn.execute_batch(
        r#"
        UPDATE conversation_sessions
        SET title = 'cached-title',
            source_file_mtime_ns =
                (source_file_mtime_ns / 1000000) * 1000000
                + CASE source_file_mtime_ns % 1000000 WHEN 1 THEN 2 ELSE 1 END
        WHERE source = 'codex' AND session_id = 'conv-1';
        "#,
    )
    .unwrap();

    assert!(crate::conversation::refresh_codex(&conn, home)
        .unwrap()
        .is_empty());
    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    assert_eq!(page.rows[0].title, "发布 Tray 客户端版本支持图片编辑透传");
}

#[test]
fn codex_conversation_refresh_reparses_when_file_size_changes() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let path = seed_codex_conversation(home);
    let conn = store::open_memory().unwrap();
    crate::conversation::refresh_codex(&conn, home).unwrap();
    let original_size = std::fs::metadata(&path).unwrap().len();
    let updated_title = "发布 Tray 客户端版本支持图片编辑透传并记录更长标题";
    let updated = fixture("codex-conversation.jsonl")
        .replace("发布 Tray 客户端版本支持图片编辑透传", updated_title);
    std::fs::write(&path, updated).unwrap();
    let updated_size = std::fs::metadata(&path).unwrap().len();
    assert_ne!(updated_size, original_size);

    assert!(crate::conversation::refresh_codex(&conn, home)
        .unwrap()
        .is_empty());

    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    assert_eq!(page.rows[0].title, updated_title);
    let cached_size: i64 = conn
        .query_row(
            "SELECT source_file_size FROM conversation_sessions WHERE source = 'codex' AND session_id = 'conv-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(cached_size, updated_size as i64);
}

#[test]
fn codex_conversation_refresh_reparses_an_ambiguous_shared_path() {
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
    conn.execute_batch(
        r#"
        INSERT INTO conversation_sessions(
            source, session_id, title, project, model, started_at, ended_at,
            source_file, capabilities_json, support_status, file_available,
            source_file_mtime_ms, source_file_size
        )
        SELECT source, 'aaa-history', 'history', project, model, started_at,
               '9999-01-01T00:00:00Z', source_file, capabilities_json, support_status, 1,
               source_file_mtime_ms, source_file_size
        FROM conversation_sessions
        WHERE source = 'codex' AND session_id = 'conv-1';
        "#,
    )
    .unwrap();

    assert!(crate::conversation::refresh_codex(&conn, home)
        .unwrap()
        .is_empty());

    let page =
        crate::conversation::sessions_page(&conn, &crate::domain::ConversationQuery::default())
            .unwrap();
    let current = page
        .rows
        .iter()
        .find(|row| row.session_id == "conv-1")
        .unwrap();
    let history = page
        .rows
        .iter()
        .find(|row| row.session_id == "aaa-history")
        .unwrap();
    assert!(current.file_available);
    assert_eq!(current.title, "发布 Tray 客户端版本支持图片编辑透传");
    assert!(!history.file_available);
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
            "SELECT file_available, source_file_mtime_ms, source_file_mtime_ns, source_file_size FROM conversation_sessions WHERE session_id = 'legacy'",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(lifecycle, (1, 0, 0, 0));
    let indexes: Vec<String> = conn
        .prepare("PRAGMA index_list(conversation_sessions)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(indexes.contains(&"idx_conversation_sessions_source_file".to_string()));
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
