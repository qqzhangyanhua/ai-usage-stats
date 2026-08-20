use crate::aggregate::work_timeline;
use crate::domain::Source;
use crate::test_support::{local_time_iso, rec};
use chrono::NaiveDate;

fn day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, 15).expect("valid date")
}

fn day_str() -> &'static str {
    "2026-08-15"
}

#[test]
fn single_session_within_day_sums_tokens() {
    let records = vec![
        rec(
            &local_time_iso(day(), 10, 0, 0),
            Source::Codex,
            "gpt-5.1-codex",
            "official",
            "/proj/a",
            "s1",
            100,
        ),
        rec(
            &local_time_iso(day(), 10, 30, 0),
            Source::Codex,
            "gpt-5.1-codex",
            "official",
            "/proj/a",
            "s1",
            50,
        ),
    ];
    let dto = work_timeline(&records, day_str());
    assert_eq!(dto.day, day_str());
    assert_eq!(dto.segment_count, 1);
    assert_eq!(dto.total_tokens, 150);
    let segment = &dto.segments[0];
    assert_eq!(segment.session_id, "s1");
    assert_eq!(segment.project, "/proj/a");
    assert_eq!(segment.model, "gpt-5.1-codex");
    assert_eq!(segment.total_tokens, 150);
    assert!(segment.start <= segment.end);
}

#[test]
fn session_crossing_midnight_is_clipped_and_tokens_split_by_day() {
    let yesterday = day().pred_opt().expect("valid date");
    let records = vec![
        rec(
            &local_time_iso(yesterday, 23, 50, 0),
            Source::Claude,
            "claude-sonnet-5",
            "anthropic",
            "/proj/b",
            "s2",
            20,
        ),
        rec(
            &local_time_iso(day(), 0, 10, 0),
            Source::Claude,
            "claude-sonnet-5",
            "anthropic",
            "/proj/b",
            "s2",
            30,
        ),
    ];

    // 昨天视角：只统计落在昨天的那条记录。
    let yesterday_str = yesterday.format("%Y-%m-%d").to_string();
    let dto_yesterday = work_timeline(&records, &yesterday_str);
    assert_eq!(dto_yesterday.segment_count, 1);
    assert_eq!(dto_yesterday.total_tokens, 20);
    assert_eq!(
        dto_yesterday.segments[0].end,
        local_time_iso(day(), 0, 0, 0)
    );

    // 今天视角：会话区间与今天有交集，片段从今天零点开始，只统计落在今天的那条记录。
    let dto_today = work_timeline(&records, day_str());
    assert_eq!(dto_today.segment_count, 1);
    assert_eq!(dto_today.total_tokens, 30);
    let segment = &dto_today.segments[0];
    assert_eq!(segment.start, local_time_iso(day(), 0, 0, 0));
    assert_eq!(segment.end, local_time_iso(day(), 0, 10, 0));
    assert_eq!(segment.total_tokens, 30);
}

#[test]
fn session_entirely_before_or_after_day_is_excluded() {
    let before = day() - chrono::Duration::days(2);
    let after = day() + chrono::Duration::days(2);
    let records = vec![
        rec(
            &local_time_iso(before, 10, 0, 0),
            Source::Pi,
            "gpt-5.5",
            "subapi",
            "/proj/c",
            "s3",
            10,
        ),
        rec(
            &local_time_iso(after, 10, 0, 0),
            Source::Pi,
            "gpt-5.5",
            "subapi",
            "/proj/c",
            "s4",
            10,
        ),
    ];
    let dto = work_timeline(&records, day_str());
    assert_eq!(dto.segment_count, 0);
    assert_eq!(dto.total_tokens, 0);
    assert!(dto.segments.is_empty());
}

#[test]
fn single_turn_session_yields_zero_width_segment() {
    let records = vec![rec(
        &local_time_iso(day(), 15, 0, 0),
        Source::Gemini,
        "gemini-pro",
        "google",
        "/proj/d",
        "s5",
        40,
    )];
    let dto = work_timeline(&records, day_str());
    assert_eq!(dto.segment_count, 1);
    let segment = &dto.segments[0];
    assert_eq!(segment.start, segment.end);
    assert_eq!(segment.total_tokens, 40);
}

#[test]
fn same_session_id_different_source_are_separate_segments() {
    let records = vec![
        rec(
            &local_time_iso(day(), 9, 0, 0),
            Source::Codex,
            "gpt-5.1-codex",
            "official",
            "/proj/a",
            "dup",
            10,
        ),
        rec(
            &local_time_iso(day(), 9, 30, 0),
            Source::Claude,
            "claude-sonnet-5",
            "anthropic",
            "/proj/a",
            "dup",
            20,
        ),
    ];
    let dto = work_timeline(&records, day_str());
    assert_eq!(dto.segment_count, 2);
    assert_eq!(dto.total_tokens, 30);
}

#[test]
fn empty_records_yield_zero_summary() {
    let dto = work_timeline(&[], day_str());
    assert_eq!(dto.day, day_str());
    assert_eq!(dto.segment_count, 0);
    assert_eq!(dto.total_tokens, 0);
    assert!(dto.segments.is_empty());
}

#[test]
fn invalid_day_string_returns_empty_without_panicking() {
    let records = vec![rec(
        &local_time_iso(day(), 10, 0, 0),
        Source::Codex,
        "gpt-5.1-codex",
        "official",
        "/proj/a",
        "s1",
        100,
    )];
    let dto = work_timeline(&records, "not-a-date");
    assert_eq!(dto.day, "not-a-date");
    assert_eq!(dto.segment_count, 0);
    assert!(dto.segments.is_empty());
}
