use crate::official_quota;
use crate::store;

#[test]
fn parse_provider_accepts_known_accounts_and_rejects_unknown() {
    assert_eq!(
        official_quota::parse_provider("cursor").unwrap(),
        crate::domain::OfficialQuotaProvider::Cursor
    );
    assert_eq!(
        official_quota::parse_provider("grok").unwrap(),
        crate::domain::OfficialQuotaProvider::Grok
    );
    assert!(official_quota::parse_provider("amp")
        .unwrap_err()
        .contains("未知的官方额度账号"));
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
fn tightest_window_picks_highest_cursor_dimension() {
    let quota = crate::domain::OfficialQuotaDto {
        rows: vec![crate::domain::OfficialQuotaRow {
            provider: "cursor".into(),
            application: "Cursor".into(),
            windows: vec![
                crate::domain::OfficialQuotaWindow {
                    kind: "billing_cycle".into(),
                    label: "总量".into(),
                    used_percent: Some(94.0),
                    resets_at: None,
                },
                crate::domain::OfficialQuotaWindow {
                    kind: "auto".into(),
                    label: "Auto".into(),
                    used_percent: Some(100.0),
                    resets_at: None,
                },
                crate::domain::OfficialQuotaWindow {
                    kind: "api".into(),
                    label: "API".into(),
                    used_percent: Some(44.0),
                    resets_at: None,
                },
            ],
            freshness: crate::domain::OfficialQuotaFreshness::Official,
            captured_at: Some("2026-08-18T12:00:00+00:00".into()),
            error: None,
        }],
        alerts_enabled: true,
        stale_after_minutes: 10,
    };
    let tightest = official_quota::tightest_window(&quota).unwrap();
    assert_eq!(tightest.provider, "Cursor");
    assert_eq!(tightest.label, "Auto");
    assert_eq!(tightest.used_percent, 100.0);
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
        [
            (
                crate::domain::OfficialQuotaProvider::Claude,
                Ok((
                    vec![crate::domain::OfficialQuotaWindow {
                        kind: "session_5h".into(),
                        label: "5 小时".into(),
                        used_percent: Some(10.0),
                        resets_at: None,
                    }],
                    "2026-08-18T12:00:00+00:00".into(),
                )),
            ),
            (
                crate::domain::OfficialQuotaProvider::Codex,
                Err("Codex 不可用".into()),
            ),
            (
                crate::domain::OfficialQuotaProvider::Cursor,
                Err("尚未配置 Cursor 会话 token".into()),
            ),
            (
                crate::domain::OfficialQuotaProvider::Grok,
                Err("尚未登录 Grok CLI，请先运行 grok login".into()),
            ),
            (
                crate::domain::OfficialQuotaProvider::Droid,
                Err("尚未登录 Droid".into()),
            ),
            (
                crate::domain::OfficialQuotaProvider::Antigravity,
                Err("尚未登录 Antigravity".into()),
            ),
        ],
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
    let grok = store::load_official_quota_row(&conn, "grok")
        .unwrap()
        .unwrap();
    assert_eq!(
        grok.2.as_deref(),
        Some("尚未登录 Grok CLI，请先运行 grok login")
    );
    assert!(grok.0.is_empty());
    let droid = store::load_official_quota_row(&conn, "droid")
        .unwrap()
        .unwrap();
    assert_eq!(droid.2.as_deref(), Some("尚未登录 Droid"));
    assert!(droid.0.is_empty());
    let antigravity = store::load_official_quota_row(&conn, "antigravity")
        .unwrap()
        .unwrap();
    assert_eq!(antigravity.2.as_deref(), Some("尚未登录 Antigravity"));
    assert!(antigravity.0.is_empty());
}
