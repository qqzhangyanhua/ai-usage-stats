use aes_gcm::aead::consts::U16;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::aes::Aes256;
use aes_gcm::{AesGcm, Key, Nonce};
use base64::Engine;
use chrono::{DateTime, Utc};

use crate::official_quota::droid;

type Gcm16 = AesGcm<Aes256, U16>;

fn at(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .unwrap()
        .with_timezone(&Utc)
}

/// 真机响应的形状：两个额度池，各三档。
const LIVE_SHAPE: &str = r#"{
    "usesTokenRateLimitsBilling": true,
    "limits": {
        "standard": {
            "fiveHour": { "usedPercent": 1, "windowEnd": "2026-08-21T19:39:28.232Z", "secondsRemaining": 17329 },
            "weekly":   { "usedPercent": 3, "windowEnd": "2026-08-28T04:34:07.665Z", "secondsRemaining": 567809 },
            "monthly":  { "usedPercent": 6, "windowEnd": "2026-09-19T04:38:58.577Z", "secondsRemaining": 2468900 }
        },
        "core": {
            "fiveHour": { "usedPercent": 80, "windowEnd": "2026-08-20T17:05:57.869Z", "secondsRemaining": null },
            "weekly":   { "usedPercent": 40, "windowEnd": "2026-08-27T12:05:57.869Z", "secondsRemaining": 508519 },
            "monthly":  { "usedPercent": 15, "windowEnd": "2026-09-19T12:05:57.869Z", "secondsRemaining": 2495719 }
        }
    },
    "extraUsageBalanceCents": 0
}"#;

#[test]
fn droid_quota_splits_standard_and_core_pools() {
    let windows = droid::parse_limits(LIVE_SHAPE, at("2026-08-21T14:00:00Z")).unwrap();
    let kinds: Vec<&str> = windows.iter().map(|w| w.kind.as_str()).collect();
    // core 的 5 小时窗已经过期，按 droid 自己的显示逻辑跳过。
    assert_eq!(
        kinds,
        [
            "five_hour",
            "weekly",
            "monthly",
            "core_weekly",
            "core_monthly"
        ]
    );
    assert_eq!(windows[0].label, "标准 5 小时");
    assert_eq!(windows[0].used_percent, Some(1.0));
    assert_eq!(
        windows[0].resets_at.as_deref(),
        Some("2026-08-21T19:39:28.232+00:00")
    );
    assert_eq!(windows[3].label, "Core 周");
    assert_eq!(windows[3].used_percent, Some(40.0));
}

#[test]
fn droid_quota_keeps_bucket_without_window_end() {
    let raw = r#"{"limits":{"standard":{"weekly":{"usedPercent":12}}}}"#;
    let windows = droid::parse_limits(raw, at("2026-08-21T14:00:00Z")).unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].kind, "weekly");
    assert_eq!(windows[0].resets_at, None);
}

#[test]
fn droid_quota_reports_structure_change_instead_of_empty() {
    assert!(droid::parse_limits("not json", at("2026-08-21T14:00:00Z")).is_err());
    assert!(droid::parse_limits(r#"{"ok":true}"#, at("2026-08-21T14:00:00Z")).is_err());
    // 有池但全部过期：说明账号当下没有计费窗，不是「0%」。
    let stale = droid::parse_limits(LIVE_SHAPE, at("2027-01-01T00:00:00Z"));
    assert!(stale.is_err());
}

#[test]
fn droid_credentials_decrypt_from_keyfile_triplet() {
    let engine = base64::engine::general_purpose::STANDARD;
    let key = [7u8; 32];
    let iv = [3u8; 16];
    let plain = r#"{"access_token":"tok-abc","refresh_token":"r","active_organization_id":"org"}"#;

    let sealed = Gcm16::new(Key::<Gcm16>::from_slice(&key))
        .encrypt(
            Nonce::<U16>::from_slice(&iv),
            Payload {
                msg: plain.as_bytes(),
                aad: &[],
            },
        )
        .unwrap();
    // droid 把 tag 单独存一段，不是拼在密文尾部。
    let (ciphertext, tag) = sealed.split_at(sealed.len() - 16);
    let payload = format!(
        "{}:{}:{}",
        engine.encode(iv),
        engine.encode(tag),
        engine.encode(ciphertext)
    );

    let decrypted = droid::decrypt_credentials(&payload, &engine.encode(key)).unwrap();
    assert_eq!(decrypted, plain);
}

/// `security find-generic-password -w` 找不到条目时退出码非 0（比如 44 = item not
/// found）；这种情况和「密钥为空」都要当成「这条存储不可用」，落回 keyfile-v2，
/// 不能直接报错断掉后面的兜底路径。
#[cfg(target_os = "macos")]
#[test]
fn droid_security_output_ignored_when_command_failed_or_empty() {
    assert_eq!(droid::parse_security_output(false, b"irrelevant"), None);
    assert_eq!(droid::parse_security_output(true, b""), None);
    assert_eq!(droid::parse_security_output(true, b"   \n"), None);
}

#[cfg(target_os = "macos")]
#[test]
fn droid_security_output_trims_trailing_newline() {
    assert_eq!(
        droid::parse_security_output(true, b"9k3F2m1z==\n"),
        Some("9k3F2m1z==".to_string())
    );
}

#[test]
fn droid_credentials_reject_bad_shape_and_key() {
    let engine = base64::engine::general_purpose::STANDARD;
    let good_key = engine.encode([7u8; 32]);
    assert!(droid::decrypt_credentials("only:two", &good_key).is_err());
    assert!(droid::decrypt_credentials("a:b:c", &engine.encode([7u8; 16])).is_err());
    let bogus = format!(
        "{}:{}:{}",
        engine.encode([3u8; 16]),
        engine.encode([0u8; 16]),
        engine.encode([9u8; 32])
    );
    assert!(droid::decrypt_credentials(&bogus, &good_key).is_err());
}
