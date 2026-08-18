use crate::test_support::*;

#[test]
fn billing_window_keeps_activity_within_five_hours() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![
        window_rec("2026-08-17T08:10:00Z", Source::Claude, "s1", 100),
        window_rec("2026-08-17T09:10:00Z", Source::Claude, "s1", 50),
    ];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.current.len(), 1);
    assert!(dto.recent.is_empty());
    let window = &dto.current[0];
    assert_eq!(window.source, "claude");
    assert_eq!(window.start, "2026-08-17T08:00:00Z");
    assert_eq!(window.end, "2026-08-17T13:00:00Z");
    assert_eq!(window.total_tokens, 150);
    assert_eq!(window.session_count, 1);
    assert_eq!(window.remaining_minutes, Some(60));
    let burn = window.burn.as_ref().expect("应有燃烧速率");
    assert!((burn.tokens_per_minute - 2.5).abs() < 1e-9);
    let projection = window.projection.as_ref().expect("应有预测");
    assert_eq!(projection.total_tokens, 300);
}

#[test]
fn billing_window_opens_after_five_hour_gap() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![
        window_rec("2026-08-17T02:00:00Z", Source::Claude, "s1", 80),
        window_rec("2026-08-17T08:00:00Z", Source::Claude, "s1", 40),
    ];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.current.len(), 1);
    assert_eq!(dto.recent.len(), 1);
    assert_eq!(dto.recent[0].start, "2026-08-17T02:00:00Z");
    assert_eq!(dto.recent[0].end, "2026-08-17T07:00:00Z");
    assert!(!dto.recent[0].is_active);
    assert_eq!(dto.current[0].start, "2026-08-17T08:00:00Z");
    assert_eq!(dto.current[0].total_tokens, 40);
}

#[test]
fn billing_window_floors_start_to_utc_hour() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![window_rec("2026-08-17T08:37:12Z", Source::Claude, "s1", 10)];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.current[0].start, "2026-08-17T08:00:00Z");
    assert_eq!(dto.current[0].end, "2026-08-17T13:00:00Z");
    assert!(dto.current[0].burn.is_none());
    assert!(dto.current[0].projection.is_none());
}

#[test]
fn billing_window_expires_after_end_or_idle() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let expired = vec![window_rec("2026-08-17T06:00:00Z", Source::Claude, "s1", 20)];
    let dto = billing_window::summarize(&expired, &PriceTable::default(), now);
    assert!(dto.current.is_empty());
    assert_eq!(dto.recent.len(), 1);
    assert!(!dto.recent[0].is_active);
    assert_eq!(dto.recent[0].remaining_minutes, None);
}

#[test]
fn billing_windows_do_not_mix_sources() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![
        window_rec("2026-08-17T10:00:00Z", Source::Claude, "c1", 30),
        window_rec("2026-08-17T10:05:00Z", Source::Codex, "x1", 90),
    ];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.current.len(), 2);
    let claude = dto
        .current
        .iter()
        .find(|window| window.source == "claude")
        .expect("claude");
    let codex = dto
        .current
        .iter()
        .find(|window| window.source == "codex")
        .expect("codex");
    assert_eq!(claude.total_tokens, 30);
    assert_eq!(codex.total_tokens, 90);
}

#[test]
fn weekly_window_sums_last_seven_days_per_source() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    let records = vec![
        window_rec("2026-08-11T12:00:00Z", Source::Claude, "s1", 100),
        window_rec("2026-08-16T09:00:00Z", Source::Claude, "s1", 50),
        // 8 天前，超出 7 天滚动窗口，不应计入。
        window_rec("2026-08-09T12:00:00Z", Source::Claude, "s2", 999),
        window_rec("2026-08-15T00:00:00Z", Source::Codex, "x1", 70),
    ];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert_eq!(dto.weekly_window_days, 7);
    assert_eq!(dto.weekly.len(), 2);

    let claude = dto
        .weekly
        .iter()
        .find(|window| window.source == "claude")
        .expect("claude weekly window");
    assert_eq!(claude.total_tokens, 150);
    assert_eq!(claude.session_count, 1);
    assert_eq!(claude.end, "2026-08-17T12:00:00Z");
    assert_eq!(claude.start, "2026-08-10T12:00:00Z");
    assert!((claude.daily_average_tokens - 150.0 / 7.0).abs() < 1e-9);
    let claude_cost = claude.cost.expect("claude weekly cost");
    assert!((claude_cost - 0.15).abs() < 1e-9);
    let claude_daily_cost = claude.daily_average_cost.expect("claude daily cost");
    assert!((claude_daily_cost - claude_cost / 7.0).abs() < 1e-9);

    let codex = dto
        .weekly
        .iter()
        .find(|window| window.source == "codex")
        .expect("codex weekly window");
    assert_eq!(codex.total_tokens, 70);

    // 按 total_tokens 降序排列。
    assert_eq!(dto.weekly[0].source, "claude");
}

#[test]
fn weekly_window_excludes_activity_older_than_seven_days_but_within_the_lookback() {
    let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
    // 10 天前：仍落在 14 天摄取回看窗内，但超出 7 天滚动窗口，不应计入 weekly。
    let records = vec![window_rec("2026-08-07T12:00:00Z", Source::Claude, "s1", 40)];
    let dto = billing_window::summarize(&records, &PriceTable::default(), now);
    assert!(dto.weekly.is_empty());
    // 仍应出现在 recent（5 小时窗）里，证明记录本身被正常摄取，只是不满足 weekly 的时间范围。
    assert_eq!(dto.recent.len(), 1);
}
