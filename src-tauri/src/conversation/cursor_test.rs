use std::collections::BTreeMap;

use super::*;

#[test]
fn adapter_projects_cursor_records_with_stable_ids_and_structural_unknowns() {
    let temp = tempfile::tempdir().unwrap();
    let session_dir = temp
        .path()
        .join(".cursor/projects/Users-workspace-project/agent-transcripts/sess-1");
    std::fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join("sess-1.jsonl");
    let initial = concat!(
        "{\"role\":\"user\",\"timestamp\":\"2026-08-22T00:00:00Z\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Read this\"}]}}\n",
        "{\"role\":\"assistant\",\"timestamp\":\"2026-08-22T00:00:01Z\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"Working\"},{\"type\":\"tool_use\",\"id\":\"call-1\",\"name\":\"Read\",\"input\":{\"path\":\"src/lib.rs\"}},{\"type\":\"tool_result\",\"tool_use_id\":\"call-1\",\"content\":\"done\"}]}}\n",
        "{\"type\":\"future_cursor_record\",\"secret_body\":\"must not appear\"}\n"
    );
    std::fs::write(&path, initial).unwrap();

    let first = index(&path).unwrap();
    assert_eq!(first.conversations.len(), 1);
    let parsed = &first.conversations[0];
    assert_eq!(parsed.session.session_id, "sess-1");
    assert_eq!(parsed.session.project, "/Users/workspace/project");
    assert_eq!(parsed.messages.len(), 2);
    assert!(parsed.events.iter().any(|event| {
        event.kind == EventKind::ToolCall
            && event.name.as_deref() == Some("Read")
            && event
                .text
                .as_deref()
                .is_some_and(|text| text.contains("src/lib.rs"))
            && event.details.get("call_id").and_then(Value::as_str) == Some("call-1")
    }));
    assert!(parsed.events.iter().any(|event| {
        event.kind == EventKind::ToolResult
            && event.text.as_deref() == Some("done")
            && event.details.get("call_id").and_then(Value::as_str) == Some("call-1")
    }));
    let unknown = parsed
        .events
        .iter()
        .find(|event| event.kind == EventKind::Unadapted)
        .unwrap();
    assert_eq!(unknown.name.as_deref(), Some("future_cursor_record"));
    assert!(!unknown.details.to_string().contains("must not appear"));
    assert!(first
        .diagnostics
        .iter()
        .all(|issue| !issue.message.contains("must not appear")));

    let stable_ids = parsed
        .events
        .iter()
        .map(|event| {
            (
                (
                    event.kind.as_str().to_string(),
                    event.name.clone(),
                    event.text.clone(),
                ),
                event.event_id.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    std::fs::write(
        &path,
        format!(
            "{initial}{}\n",
            serde_json::json!({
                "type":"turn_ended",
                "timestamp":"2026-08-22T00:00:02Z",
                "status":"success"
            })
        ),
    )
    .unwrap();
    let appended = detail(&path, "sess-1", false).unwrap();
    for event in appended
        .events
        .iter()
        .filter(|event| event.name.as_deref() != Some("turn_success"))
    {
        let key = (
            event.kind.as_str().to_string(),
            event.name.clone(),
            event.text.clone(),
        );
        assert_eq!(stable_ids.get(&key), Some(&event.event_id));
    }
}
