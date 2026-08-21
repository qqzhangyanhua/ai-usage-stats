use crate::official_quota;

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
