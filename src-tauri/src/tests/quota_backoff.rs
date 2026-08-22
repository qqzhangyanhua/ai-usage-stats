use chrono::{DateTime, Duration, Utc};

use crate::domain::OfficialQuotaProvider;
use crate::official_quota::backoff::{self, BackoffState};

fn at(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn rate_limited_failures_back_off_further_than_ordinary_ones() {
    let now = at("2026-08-22T12:00:00Z");
    let mut state = BackoffState::default();

    backoff::record_failure(
        &mut state,
        OfficialQuotaProvider::Claude,
        "Claude 用量接口被限流，稍后会自动恢复，别反复手动刷新",
        now,
    );
    let limited = backoff::cooldown_remaining(&state, OfficialQuotaProvider::Claude, now).unwrap();
    assert_eq!(limited, Duration::minutes(10));

    backoff::record_failure(
        &mut state,
        OfficialQuotaProvider::Cursor,
        "无法连接 Cursor 用量接口，请检查网络后重试",
        now,
    );
    let ordinary = backoff::cooldown_remaining(&state, OfficialQuotaProvider::Cursor, now).unwrap();
    assert_eq!(ordinary, Duration::minutes(1));
}

#[test]
fn consecutive_failures_double_the_wait_up_to_a_cap() {
    let now = at("2026-08-22T12:00:00Z");
    let mut state = BackoffState::default();
    let limited = "拉取失败：HTTP 429";

    let mut seen = Vec::new();
    for _ in 0..8 {
        backoff::record_failure(&mut state, OfficialQuotaProvider::Claude, limited, now);
        seen.push(
            backoff::cooldown_remaining(&state, OfficialQuotaProvider::Claude, now)
                .unwrap()
                .num_minutes(),
        );
    }
    assert_eq!(seen[0], 10);
    assert_eq!(seen[1], 20);
    assert_eq!(seen[2], 40);
    // 封顶后不再增长，别把 provider 永久拉黑。
    assert!(seen[3..].iter().all(|minutes| *minutes == 60));
}

#[test]
fn success_clears_the_cooldown() {
    let now = at("2026-08-22T12:00:00Z");
    let mut state = BackoffState::default();
    backoff::record_failure(&mut state, OfficialQuotaProvider::Grok, "HTTP 429", now);
    assert!(backoff::cooldown_remaining(&state, OfficialQuotaProvider::Grok, now).is_some());

    backoff::record_success(&mut state, OfficialQuotaProvider::Grok);
    assert!(backoff::cooldown_remaining(&state, OfficialQuotaProvider::Grok, now).is_none());
    // 清干净了，下次失败要从最短的等待重新起步。
    backoff::record_failure(&mut state, OfficialQuotaProvider::Grok, "HTTP 429", now);
    assert_eq!(
        backoff::cooldown_remaining(&state, OfficialQuotaProvider::Grok, now).unwrap(),
        Duration::minutes(10)
    );
}

#[test]
fn cooldown_expires_on_its_own() {
    let start = at("2026-08-22T12:00:00Z");
    let mut state = BackoffState::default();
    backoff::record_failure(&mut state, OfficialQuotaProvider::Claude, "HTTP 429", start);

    assert!(backoff::cooldown_remaining(
        &state,
        OfficialQuotaProvider::Claude,
        at("2026-08-22T12:09:00Z")
    )
    .is_some());
    assert!(backoff::cooldown_remaining(
        &state,
        OfficialQuotaProvider::Claude,
        at("2026-08-22T12:10:01Z")
    )
    .is_none());
}

#[test]
fn cooldown_message_tells_the_user_how_long_is_left() {
    let now = at("2026-08-22T12:00:00Z");
    let mut state = BackoffState::default();
    backoff::record_failure(&mut state, OfficialQuotaProvider::Claude, "HTTP 429", now);

    let message = backoff::cooldown_message(
        &state,
        OfficialQuotaProvider::Claude,
        at("2026-08-22T12:07:30Z"),
    )
    .unwrap();
    assert!(message.contains("Claude"));
    // 向上取整，且永远不会说「0 分钟后」。
    assert!(message.contains("3 分钟"));
    assert!(
        backoff::cooldown_message(&state, OfficialQuotaProvider::Cursor, now).is_none(),
        "没失败过的 provider 不该有冷却提示"
    );
}

#[test]
fn rate_limit_detection_covers_the_wordings_providers_actually_use() {
    assert!(backoff::is_rate_limited(
        "Claude 用量接口被限流，稍后会自动恢复"
    ));
    assert!(backoff::is_rate_limited(
        "拉取 Antigravity 限额失败：HTTP 429"
    ));
    assert!(!backoff::is_rate_limited("Claude 登录已失效，请重新登录"));
    assert!(!backoff::is_rate_limited("无法连接，请检查网络"));
}

#[test]
fn backoff_state_round_trips_through_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(backoff::STATE_NAME);
    assert_eq!(backoff::load_state(&path), BackoffState::default());

    let mut state = BackoffState::default();
    backoff::record_failure(
        &mut state,
        OfficialQuotaProvider::Claude,
        "HTTP 429",
        at("2026-08-22T12:00:00Z"),
    );
    backoff::save_state(&path, &state).unwrap();
    // 重启后仍然生效——否则「重启一下再点」就绕过去了。
    assert_eq!(backoff::load_state(&path), state);

    std::fs::write(&path, "{not json").unwrap();
    assert_eq!(backoff::load_state(&path), BackoffState::default());
}
