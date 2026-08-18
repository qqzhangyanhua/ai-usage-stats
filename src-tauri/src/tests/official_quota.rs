use crate::official_quota;
use crate::store;

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
fn official_quota_keeps_last_good_windows_on_fetch_failure() {
    let conn = store::open_memory().unwrap();
    let windows = vec![crate::domain::OfficialQuotaWindow {
        kind: "session_5h".into(),
        label: "5 小时".into(),
        used_percent: Some(40.0),
        resets_at: Some("2026-08-18T12:00:00+00:00".into()),
    }];
    official_quota::apply_success(
        &conn,
        crate::domain::OfficialQuotaProvider::Claude,
        windows,
        "2026-08-18T11:00:00+00:00",
    )
    .unwrap();
    official_quota::apply_failure(
        &conn,
        crate::domain::OfficialQuotaProvider::Claude,
        "解析失败",
    )
    .unwrap();
    let row = store::load_official_quota_row(&conn, "claude")
        .unwrap()
        .unwrap();
    assert_eq!(row.0[0].used_percent, Some(40.0));
    assert_eq!(row.2.as_deref(), Some("解析失败"));
}

#[test]
fn official_quota_freshness_turns_stale_after_ten_minutes() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-08-18T12:10:01+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(
        official_quota::freshness("2026-08-18T12:00:00+00:00", now),
        crate::domain::OfficialQuotaFreshness::Stale
    );
    assert_eq!(
        official_quota::freshness("2026-08-18T12:05:00+00:00", now),
        crate::domain::OfficialQuotaFreshness::Official
    );
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

#[test]
fn quota_alerts_dedupe_by_reset_and_skip_stale() {
    let official = crate::domain::OfficialQuotaDto {
        rows: vec![crate::domain::OfficialQuotaRow {
            provider: "claude".into(),
            application: "Claude".into(),
            windows: vec![crate::domain::OfficialQuotaWindow {
                kind: "session_5h".into(),
                label: "5 小时".into(),
                used_percent: Some(82.0),
                resets_at: Some("2026-08-18T15:00:00+00:00".into()),
            }],
            freshness: crate::domain::OfficialQuotaFreshness::Official,
            captured_at: Some("2026-08-18T12:00:00+00:00".into()),
            error: None,
        }],
        alerts_enabled: true,
        stale_after_minutes: 10,
    };
    let (after, alerts) = official_quota::notify::prepare_notifications(
        official_quota::notify::NotifyState::default(),
        &official,
    );
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].threshold, 80);
    let (_, again) = official_quota::notify::prepare_notifications(after.clone(), &official);
    assert!(again.is_empty());

    let mut stale = official.clone();
    stale.rows[0].freshness = crate::domain::OfficialQuotaFreshness::Stale;
    stale.rows[0].windows[0].used_percent = Some(100.0);
    let (_, stale_alerts) = official_quota::notify::prepare_notifications(after, &stale);
    assert!(stale_alerts.is_empty());
}

#[test]
fn quota_alerts_reset_when_resets_at_changes() {
    let first = crate::domain::OfficialQuotaDto {
        rows: vec![crate::domain::OfficialQuotaRow {
            provider: "claude".into(),
            application: "Claude".into(),
            windows: vec![crate::domain::OfficialQuotaWindow {
                kind: "weekly".into(),
                label: "7 天".into(),
                used_percent: Some(100.0),
                resets_at: Some("2026-08-20T00:00:00+00:00".into()),
            }],
            freshness: crate::domain::OfficialQuotaFreshness::Official,
            captured_at: Some("2026-08-18T12:00:00+00:00".into()),
            error: None,
        }],
        alerts_enabled: true,
        stale_after_minutes: 10,
    };
    let (state, alerts) = official_quota::notify::prepare_notifications(
        official_quota::notify::NotifyState::default(),
        &first,
    );
    assert_eq!(alerts[0].threshold, 100);
    let mut next = first;
    next.rows[0].windows[0].resets_at = Some("2026-08-27T00:00:00+00:00".into());
    next.rows[0].windows[0].used_percent = Some(81.0);
    let (_, alerts) = official_quota::notify::prepare_notifications(state, &next);
    assert_eq!(alerts[0].threshold, 80);
}

#[test]
fn cursor_usage_summary_parses_plan_percent() {
    let raw = r#"{
        "billingCycleEnd": "2026-09-02T14:11:55.000Z",
        "individualUsage": { "plan": { "used": 800, "limit": 1000, "totalPercentUsed": 80 } }
    }"#;
    let windows = official_quota::cursor::parse_usage_summary(raw).unwrap();
    assert_eq!(windows[0].kind, "billing_cycle");
    assert_eq!(windows[0].used_percent, Some(80.0));
}

#[test]
fn cursor_usage_summary_keeps_error_on_unknown_shape() {
    assert!(official_quota::cursor::parse_usage_summary(r#"{"ok":true}"#).is_err());
}

#[test]
fn codex_rate_limits_parse_primary_window() {
    let raw = r#"{
        "id": 2,
        "result": {
            "rateLimits": {
                "primary": { "usedPercent": 25, "windowDurationMins": 15, "resetsAt": 1730947200 }
            }
        }
    }"#;
    let windows = official_quota::codex::parse_rate_limits(raw).unwrap();
    assert_eq!(windows[0].used_percent, Some(25.0));
    assert_eq!(windows[0].kind, "primary");
}

#[test]
fn tray_title_includes_tightest_official_percent() {
    let quota = official_quota::TightestQuota {
        provider: "Claude".into(),
        label: "5h".into(),
        used_percent: 82.0,
        stale: false,
    };
    assert_eq!(
        crate::tray::format_title_with_quota(Some(1.23), false, Some(&quota)),
        "$1.23 · Claude 5h 82%"
    );
    let stale = official_quota::TightestQuota {
        stale: true,
        ..quota
    };
    assert_eq!(
        crate::tray::format_title_with_quota(Some(1.23), false, Some(&stale)),
        "$1.23 · Claude 5h 82%*"
    );
    assert_eq!(crate::tray::format_title(Some(1.23), false), "$1.23");
}

#[test]
fn apply_fetch_results_isolates_provider_failures() {
    let conn = store::open_memory().unwrap();
    official_quota::apply_fetch_results(
        &conn,
        Ok((
            vec![crate::domain::OfficialQuotaWindow {
                kind: "session_5h".into(),
                label: "5 小时".into(),
                used_percent: Some(10.0),
                resets_at: None,
            }],
            "2026-08-18T12:00:00+00:00".into(),
        )),
        Err("Codex 不可用".into()),
        Err("尚未配置 Cursor 会话 token".into()),
    )
    .unwrap();
    let claude = store::load_official_quota_row(&conn, "claude")
        .unwrap()
        .unwrap();
    assert_eq!(claude.0[0].used_percent, Some(10.0));
    let codex = store::load_official_quota_row(&conn, "codex")
        .unwrap()
        .unwrap();
    assert_eq!(codex.2.as_deref(), Some("Codex 不可用"));
    assert!(codex.0.is_empty());
}
