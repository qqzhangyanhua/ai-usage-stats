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
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].kind, "billing_cycle");
    assert_eq!(windows[0].label, "总量");
    assert_eq!(windows[0].used_percent, Some(80.0));
}

#[test]
fn cursor_usage_summary_parses_auto_api_and_on_demand() {
    let raw = r#"{
        "billingCycleEnd": "2026-09-02T14:11:55.000Z",
        "individualUsage": {
            "plan": {
                "enabled": true,
                "used": 940,
                "limit": 1000,
                "autoPercentUsed": 100,
                "apiPercentUsed": 44,
                "totalPercentUsed": 94
            },
            "onDemand": { "enabled": true, "used": 2309, "limit": 5000 }
        }
    }"#;
    let windows = official_quota::cursor::parse_usage_summary(raw).unwrap();
    assert_eq!(windows.len(), 4);
    assert_eq!(windows[0].kind, "billing_cycle");
    assert_eq!(windows[0].used_percent, Some(94.0));
    assert_eq!(windows[1].kind, "auto");
    assert_eq!(windows[1].label, "Auto");
    assert_eq!(windows[1].used_percent, Some(100.0));
    assert_eq!(windows[2].kind, "api");
    assert_eq!(windows[2].used_percent, Some(44.0));
    assert_eq!(windows[3].kind, "on_demand");
    assert_eq!(windows[3].label, "按需");
    assert_eq!(windows[3].used_percent, Some(46.18));
}

#[test]
fn cursor_on_demand_falls_back_to_team_when_individual_has_no_limit() {
    let raw = r#"{
        "billingCycleEnd": "2026-09-02T14:11:55.000Z",
        "individualUsage": {
            "plan": { "totalPercentUsed": 10, "autoPercentUsed": 0, "apiPercentUsed": 20 },
            "onDemand": { "enabled": true, "used": 1840, "limit": null }
        },
        "teamUsage": { "onDemand": { "used": 2500, "limit": 10000 } }
    }"#;
    let windows = official_quota::cursor::parse_usage_summary(raw).unwrap();
    assert_eq!(windows.len(), 4);
    let on_demand = windows
        .iter()
        .find(|window| window.kind == "on_demand")
        .unwrap();
    assert_eq!(on_demand.used_percent, Some(25.0));
}

#[test]
fn cursor_skips_disabled_on_demand_without_limit() {
    let raw = r#"{
        "individualUsage": {
            "plan": { "autoPercentUsed": 12, "apiPercentUsed": 8, "totalPercentUsed": 10 },
            "onDemand": { "enabled": false, "used": 0, "limit": null }
        }
    }"#;
    let windows = official_quota::cursor::parse_usage_summary(raw).unwrap();
    assert_eq!(windows.len(), 3);
    assert!(windows.iter().all(|window| window.kind != "on_demand"));
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
fn grok_credits_parse_weekly_and_build() {
    let raw = r#"{
        "config": {
            "creditUsagePercent": 34.0,
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "end": "2026-08-05T01:12:18.000Z"
            },
            "productUsage": [{ "product": "GrokBuild", "usagePercent": 45.0 }]
        }
    }"#;
    let windows = official_quota::grok::parse_credits(raw).unwrap();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].kind, "weekly");
    assert_eq!(windows[0].label, "周额度");
    assert_eq!(windows[0].used_percent, Some(34.0));
    assert_eq!(
        windows[0].resets_at.as_deref(),
        Some("2026-08-05T01:12:18.000Z")
    );
    assert_eq!(windows[1].kind, "product_grokbuild");
    assert_eq!(windows[1].label, "Grok Build");
    assert_eq!(windows[1].used_percent, Some(45.0));
}

#[test]
fn grok_credits_use_build_percent_when_weekly_missing() {
    let raw = r#"{
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "end": "2026-08-05T01:12:18.000Z"
            },
            "productUsage": [{ "product": "GrokBuild", "usagePercent": 12.5 }]
        }
    }"#;
    let windows = official_quota::grok::parse_credits(raw).unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].kind, "weekly");
    assert_eq!(windows[0].used_percent, Some(12.5));
}

#[test]
fn grok_credits_treat_empty_weekly_period_as_zero() {
    let raw = r#"{
        "config": {
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-07-29T01:12:18.000Z",
                "end": "2026-08-05T01:12:18.000Z"
            }
        }
    }"#;
    let windows = official_quota::grok::parse_credits(raw).unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].kind, "weekly");
    assert_eq!(windows[0].used_percent, Some(0.0));
}

#[test]
fn grok_credits_skip_zero_on_demand_cap() {
    let raw = r#"{
        "config": {
            "creditUsagePercent": 10,
            "onDemandUsed": { "val": 0 },
            "onDemandCap": { "val": 0 }
        }
    }"#;
    let windows = official_quota::grok::parse_credits(raw).unwrap();
    assert_eq!(windows.len(), 1);
    assert!(windows.iter().all(|window| window.kind != "on_demand"));
}

#[test]
fn grok_credits_parse_on_demand_when_cap_present() {
    let raw = r#"{
        "config": {
            "creditUsagePercent": 10,
            "onDemandUsed": { "val": 250 },
            "onDemandCap": { "val": 1000 }
        }
    }"#;
    let windows = official_quota::grok::parse_credits(raw).unwrap();
    let on_demand = windows
        .iter()
        .find(|window| window.kind == "on_demand")
        .unwrap();
    assert_eq!(on_demand.label, "按需");
    assert_eq!(on_demand.used_percent, Some(25.0));
}

#[test]
fn grok_monthly_parses_used_limit_wrappers() {
    let raw = r#"{
        "config": {
            "used": { "val": 2000 },
            "monthlyLimit": { "val": 8000 },
            "billingPeriodEnd": "2026-09-01T00:00:00Z"
        }
    }"#;
    let windows = official_quota::grok::parse_monthly(raw).unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].kind, "monthly");
    assert_eq!(windows[0].label, "月额度");
    assert_eq!(windows[0].used_percent, Some(25.0));
    assert_eq!(
        windows[0].resets_at.as_deref(),
        Some("2026-09-01T00:00:00Z")
    );
}

#[test]
fn grok_monthly_skips_when_used_missing() {
    let raw = r#"{ "config": { "monthlyLimit": { "val": 8000 } } }"#;
    assert!(official_quota::grok::parse_monthly(raw).unwrap().is_empty());
}

#[test]
fn grok_rejects_leaked_percent_and_unknown_shape() {
    let leaked = r#"{ "config": { "creditUsagePercent": 1776950400 } }"#;
    assert!(official_quota::grok::parse_credits(leaked)
        .unwrap()
        .is_empty());
    assert!(official_quota::grok::parse_credits(r#"{"ok":true}"#)
        .unwrap()
        .is_empty());
}

#[test]
fn grok_auth_prefers_supergrok_scope_and_skips_expired() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-08-19T00:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let raw = r#"{
        "https://auth.x.ai::openid": {
            "key": "supergrok-token",
            "auth_mode": "oidc",
            "expires_at": "2026-08-20T00:00:00+00:00"
        },
        "https://accounts.x.ai/sign-in": {
            "key": "legacy-token",
            "auth_mode": "oidc",
            "expires_at": "2026-08-20T00:00:00+00:00"
        }
    }"#;
    let session = official_quota::grok::parse_auth_json(raw, now).unwrap();
    assert_eq!(session.token, "supergrok-token");
    assert_eq!(session.user_id, None);

    let expired = r#"{
        "https://auth.x.ai::openid": {
            "key": "old",
            "auth_mode": "oidc",
            "expires_at": "2026-08-18T00:00:00+00:00"
        }
    }"#;
    let error = official_quota::grok::parse_auth_json(expired, now).unwrap_err();
    assert!(error.contains("已过期"));
}

#[test]
fn grok_auth_rejects_api_key_and_weblogin() {
    let now = chrono::Utc::now();
    let api_key = r#"{
        "xai::api_key": { "key": "xai-secret", "auth_mode": "api_key" }
    }"#;
    let error = official_quota::grok::parse_auth_json(api_key, now).unwrap_err();
    assert!(error.contains("会话登录"));

    let web_login = r#"{
        "https://accounts.x.ai/sign-in": { "key": "legacy-web", "auth_mode": "web_login" }
    }"#;
    let error = official_quota::grok::parse_auth_json(web_login, now).unwrap_err();
    assert!(error.contains("无效"));
}

#[test]
fn grok_auth_reads_user_id_from_session() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-08-19T00:00:00+00:00")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let raw = r#"{
        "https://auth.x.ai::openid": {
            "key": "supergrok-token",
            "auth_mode": "oidc",
            "user_id": "user-123",
            "expires_at": "2026-08-20T00:00:00+00:00"
        }
    }"#;
    let session = official_quota::grok::parse_auth_json(raw, now).unwrap();
    assert_eq!(session.token, "supergrok-token");
    assert_eq!(session.user_id.as_deref(), Some("user-123"));
}

#[test]
fn grok_user_response_reads_camel_case_user_id() {
    assert_eq!(
        official_quota::grok::parse_user_id_response(r#"{"userId":"mock-user","email":"a@b.c"}"#)
            .unwrap(),
        "mock-user"
    );
    assert!(official_quota::grok::parse_user_id_response(r#"{"email":"a@b.c"}"#).is_err());
}

#[test]
fn grok_rest_serialize_error_falls_back_to_grpc() {
    assert!(official_quota::grok_grpc::should_fallback_to_grpc(
        "拉取 Grok 限额失败：Failed to serialize billing response"
    ));
    assert!(official_quota::grok_grpc::should_fallback_to_grpc(
        "拉取 Grok 限额失败：HTTP 500"
    ));
    assert!(!official_quota::grok_grpc::should_fallback_to_grpc(
        "Grok 登录已过期，请重新运行 grok login"
    ));
}

#[test]
fn grok_grpc_parses_ratio_and_reset() {
    let inner = {
        let mut body = vec![0x0d];
        body.extend_from_slice(&0.425f32.to_le_bytes());
        let mut timestamp = vec![0x08];
        timestamp.extend(encode_varint(1_800_000_000));
        body.push(0x2a);
        body.extend(encode_varint(timestamp.len() as u64));
        body.extend(timestamp);
        body
    };
    let mut payload = vec![0x0a];
    payload.extend(encode_varint(inner.len() as u64));
    payload.extend(inner);
    let mut framed = vec![0x00];
    framed.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    framed.extend(payload);

    let windows = official_quota::grok_grpc::parse_credits_grpc(&framed, 1_700_000_000).unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].kind, "weekly");
    assert!((windows[0].used_percent.unwrap() - 42.5).abs() < 0.01);
    assert!(windows[0]
        .resets_at
        .as_deref()
        .unwrap()
        .starts_with("2027-01-15"));
}

fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
    out
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
}
