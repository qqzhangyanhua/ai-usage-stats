use rusqlite::params;

use super::*;

#[test]
fn adapter_preserves_recognized_content_and_reports_body_free_degradation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("opencode.db");
    let source_db = rusqlite::Connection::open(&path).unwrap();
    source_db
        .execute_batch(
            r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                title TEXT,
                directory TEXT,
                time_created INTEGER,
                time_updated INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE part (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                data TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
    source_db
        .execute(
            "INSERT INTO session VALUES(?1, ?2, ?3, ?4, ?5)",
            params!["ses-partial", "Partial data", "/workspace", 1_i64, 2_i64],
        )
        .unwrap();
    source_db
        .execute(
            "INSERT INTO session VALUES(?1, ?2, ?3, ?4, ?5)",
            params!["ses-empty", "Empty data", "/workspace", 3_i64, 4_i64],
        )
        .unwrap();
    source_db
        .execute(
            "INSERT INTO message VALUES(?1, ?2, ?3)",
            params![
                "msg-partial",
                "ses-partial",
                serde_json::json!({"role":"assistant","time":{"created":1_i64}}).to_string()
            ],
        )
        .unwrap();
    source_db
        .execute(
            "INSERT INTO part VALUES(?1, ?2, ?3)",
            params![
                "part-text",
                "msg-partial",
                serde_json::json!({"type":"text","text":"recognized sibling"}).to_string()
            ],
        )
        .unwrap();
    source_db
        .execute(
            "INSERT INTO part VALUES(?1, ?2, ?3)",
            params![
                "part-invalid",
                "msg-partial",
                "{\"text\":\"SENTINEL_INVALID\","
            ],
        )
        .unwrap();
    source_db
        .execute(
            "INSERT INTO part VALUES(?1, ?2, ?3)",
            params![
                "part-future",
                "msg-partial",
                serde_json::json!({"type":"future-part","secret_body":"SENTINEL_UNKNOWN"})
                    .to_string()
            ],
        )
        .unwrap();
    drop(source_db);

    let batch = index(&path).unwrap();
    assert_eq!(batch.conversations.len(), 2);
    let conversation = batch
        .conversations
        .iter()
        .find(|conversation| conversation.session.session_id == "ses-partial")
        .unwrap();
    let empty = batch
        .conversations
        .iter()
        .find(|conversation| conversation.session.session_id == "ses-empty")
        .unwrap();
    assert_eq!(
        empty.session.capabilities,
        vec!["messages", "events", "usage"]
    );
    assert_eq!(conversation.messages[0].text, "recognized sibling");
    assert_eq!(
        conversation
            .events
            .iter()
            .filter(|event| event.kind == EventKind::Unadapted)
            .count(),
        2
    );
    assert!(batch
        .diagnostics
        .iter()
        .any(|issue| issue.event_type.as_deref() == Some("part_json")));
    assert!(batch
        .diagnostics
        .iter()
        .any(|issue| issue.event_type.as_deref() == Some("part_type")));
    let diagnostic_text = batch
        .diagnostics
        .iter()
        .map(|issue| format!("{} {:?}", issue.message, issue.event_type))
        .collect::<String>();
    let event_text = serde_json::to_string(&conversation.events).unwrap();
    assert!(!diagnostic_text.contains("SENTINEL_INVALID"));
    assert!(!diagnostic_text.contains("SENTINEL_UNKNOWN"));
    assert!(!event_text.contains("SENTINEL_INVALID"));
    assert!(!event_text.contains("SENTINEL_UNKNOWN"));
}
