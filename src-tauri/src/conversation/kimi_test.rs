use super::*;

fn seed(temp: &Path, status: &str, include_update: bool) -> PathBuf {
    let root = temp.join("kimi");
    let path = root.join("sessions/hash/kimi-native-id/wire.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut content = format!(
        "{{\"timestamp\":1787000000.0,\"message\":{{\"type\":\"StatusUpdate\",\"payload\":{{\"message_id\":\"status-native\",\"status\":\"{status}\"}}}}}}\n{{\"timestamp\":1787000001.0,\"message\":{{\"type\":\"FutureWire\",\"payload\":{{\"secret_body\":\"raw generic body\"}}}}}}\n"
    );
    if include_update {
        content.push_str("{\"timestamp\":1787000002.0,\"message\":{\"type\":\"StatusUpdate\",\"payload\":{\"message_id\":\"status-native\",\"status\":\"done\"}}}\n");
    }
    std::fs::write(&path, content).unwrap();
    std::fs::write(root.join("kimi.json"), "{\"work_dirs\":[]}").unwrap();
    path
}

#[test]
fn adapter_merges_kimi_native_identity_and_keeps_unknown_json_out_of_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let path = seed(temp.path(), "working", false);
    let first = index(&path).unwrap();
    let first_status = first.conversations[0]
        .events
        .iter()
        .find(|event| event.name.as_deref() == Some("working"))
        .unwrap();
    let stable_id = first_status.event_id.clone();

    seed(temp.path(), "working", true);
    let updated = index(&path).unwrap();
    let statuses = updated.conversations[0]
        .events
        .iter()
        .filter(|event| matches!(event.name.as_deref(), Some("working" | "done")))
        .collect::<Vec<_>>();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].name.as_deref(), Some("done"));
    assert_eq!(statuses[0].event_id, stable_id);
    let unknown = updated.conversations[0]
        .events
        .iter()
        .find(|event| event.kind == EventKind::Unadapted)
        .unwrap();
    assert!(unknown.details.to_string().contains("raw generic body"));
    assert!(updated
        .diagnostics
        .iter()
        .all(|issue| !issue.message.contains("raw generic body")));

    std::fs::write(&path, "{not-json\n").unwrap();
    assert!(index(&path).is_err());
}
