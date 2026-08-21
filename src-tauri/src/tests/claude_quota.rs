use crate::official_quota;

#[test]
fn claude_statusline_parses_rate_limits_and_rejects_leaked_epoch() {
    let raw = r#"{
        "rate_limits": {
            "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
            "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 }
        }
    }"#;
    let (windows, _) = official_quota::claude::parse_statusline(raw).unwrap();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].kind, "session_5h");
    assert_eq!(windows[0].used_percent, Some(23.5));
    assert_eq!(windows[1].kind, "weekly");

    let leaked = r#"{
        "rate_limits": {
            "five_hour": { "used_percentage": 1776950400, "resets_at": 1776950400 }
        }
    }"#;
    assert!(official_quota::claude::parse_statusline(leaked).is_err());
}

#[test]
fn claude_hook_refuses_to_overwrite_existing_status_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(
        &path,
        r#"{"statusLine":{"type":"command","command":"echo old"}}"#,
    )
    .unwrap();
    let preview = official_quota::hook::apply(&path, "\"/app\" statusline").unwrap();
    assert!(preview.conflict);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("echo old"));
}

#[test]
fn claude_hook_writes_when_status_line_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, "{}").unwrap();
    let preview = official_quota::hook::apply(&path, "\"/app\" statusline").unwrap();
    assert!(!preview.conflict);
    assert!(preview.already_configured);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("statusline"));
}
