use crate::official_quota::opencode;

const LIVE_SHAPE: &str = r#"{
    "usage": {
        "rolling": { "percent": 12.5, "resetsAt": "2026-08-22T06:00:00Z" },
        "weekly":  { "percent": 40,   "resetsAt": "2026-08-29T00:00:00Z" },
        "monthly": { "percent": 8,    "resetsAt": "2026-09-01T00:00:00Z" }
    }
}"#;

#[test]
fn opencode_quota_reads_three_windows() {
    let windows = opencode::parse_usage(LIVE_SHAPE).unwrap();
    let kinds: Vec<&str> = windows.iter().map(|w| w.kind.as_str()).collect();
    assert_eq!(kinds, ["session", "weekly", "monthly"]);
    assert_eq!(windows[0].label, "滚动");
    assert_eq!(windows[0].used_percent, Some(12.5));
    assert_eq!(
        windows[0].resets_at.as_deref(),
        Some("2026-08-22T06:00:00Z")
    );
    assert_eq!(windows[2].used_percent, Some(8.0));
}

#[test]
fn opencode_quota_skips_missing_windows_and_flags_structure_change() {
    let partial = r#"{"usage":{"weekly":{"percent":3}}}"#;
    let windows = opencode::parse_usage(partial).unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].kind, "weekly");
    assert_eq!(windows[0].resets_at, None);

    assert!(opencode::parse_usage("not json").is_err());
    assert!(opencode::parse_usage(r#"{"ok":true}"#).is_err());
    assert!(opencode::parse_usage(r#"{"usage":{}}"#).is_err());
}

#[test]
fn opencode_api_key_absent_file_means_logged_out_but_broken_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");

    // 文件不存在 = 没登录，不是错误。
    assert_eq!(opencode::load_api_key(&path).unwrap(), None);

    std::fs::write(
        &path,
        r#"{"opencode-go":{"key":"sk-zen-1"},"anthropic":{"key":"other"}}"#,
    )
    .unwrap();
    assert_eq!(
        opencode::load_api_key(&path).unwrap().as_deref(),
        Some("sk-zen-1")
    );

    // 别的 provider 有条目但自己没有 → 仍是没登录。
    std::fs::write(&path, r#"{"anthropic":{"key":"other"}}"#).unwrap();
    assert_eq!(opencode::load_api_key(&path).unwrap(), None);

    std::fs::write(&path, r#"{"opencode-go":{"key":"   "}}"#).unwrap();
    assert_eq!(opencode::load_api_key(&path).unwrap(), None);

    // 文件在但坏了要报错，不能当成没登录。
    std::fs::write(&path, "{not json").unwrap();
    assert!(opencode::load_api_key(&path).is_err());
}
