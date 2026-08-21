//! Factory / Droid 官方额度：读本机 `~/.factory` 的登录态，打 `GET /api/billing/limits`。
//!
//! 凭证优先 `auth.v2.file`（AES-256-GCM，`base64(iv):base64(tag):base64(密文)`，
//! 密钥就是旁边明文的 `auth.v2.key`），解出 `{access_token, refresh_token,
//! active_organization_id}`；旧版 `auth.json` 是明文，作为兜底。
//! macOS 上 droid 可能把凭证放进系统钥匙串，那种情况读不到，降级为 unavailable。

use std::path::PathBuf;
use std::time::Duration;

use aes_gcm::aead::consts::{U12, U16};
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::aes::Aes256;
use aes_gcm::{AesGcm, Key, Nonce};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::domain::OfficialQuotaWindow;
use crate::official_quota::sanitize_percent;

const LIMITS_URL: &str = "https://api.factory.ai/api/billing/limits";
const TIMEOUT: Duration = Duration::from_secs(12);
/// 响应里的两个额度池：standard 是主池，core 是 Droid Core。
const POOLS: [(&str, &str, &str); 2] = [("standard", "", "标准"), ("core", "core_", "Core")];
const BUCKETS: [(&str, &str); 3] = [("fiveHour", "5 小时"), ("weekly", "周"), ("monthly", "月")];

pub fn fetch_rate_limits() -> Result<(Vec<OfficialQuotaWindow>, String), String> {
    let token = load_access_token()?;
    let raw = request_limits(&token)?;
    let windows = parse_limits(&raw, Utc::now())?;
    Ok((windows, Utc::now().to_rfc3339()))
}

/// standard / core 两个池各三档；`windowEnd` 已过去的档位说明该桶没在计费窗内，跳过
/// （对齐 droid 自己的显示逻辑）。全部跳过才算结构异常。
pub fn parse_limits(raw: &str, now: DateTime<Utc>) -> Result<Vec<OfficialQuotaWindow>, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("Droid 限额 JSON 解析失败：{e}"))?;
    let limits = value
        .get("limits")
        .ok_or_else(|| "Droid 限额响应里没有 limits".to_string())?;

    let mut windows = Vec::new();
    let mut saw_pool = false;
    for (pool_key, kind_prefix, pool_label) in POOLS {
        let Some(pool) = limits.get(pool_key) else {
            continue;
        };
        saw_pool = true;
        for (bucket_key, bucket_label) in BUCKETS {
            let Some(bucket) = pool.get(bucket_key) else {
                continue;
            };
            let resets_at = window_end(bucket);
            if !is_active(resets_at.as_ref(), now) {
                continue;
            }
            let Some(percent) = bucket
                .get("usedPercent")
                .and_then(Value::as_f64)
                .and_then(sanitize_percent)
            else {
                continue;
            };
            windows.push(OfficialQuotaWindow {
                kind: format!("{kind_prefix}{}", bucket_kind(bucket_key)),
                label: format!("{pool_label} {bucket_label}"),
                used_percent: Some(percent),
                resets_at: resets_at.map(|value| value.to_rfc3339()),
            });
        }
    }

    if windows.is_empty() {
        if saw_pool {
            return Err("Droid 限额响应里没有仍在计费窗内的额度".to_string());
        }
        return Err("Droid 限额响应里没有可用的额度池".to_string());
    }
    Ok(windows)
}

fn bucket_kind(bucket_key: &str) -> &'static str {
    match bucket_key {
        "fiveHour" => "five_hour",
        "weekly" => "weekly",
        _ => "monthly",
    }
}

fn window_end(bucket: &Value) -> Option<DateTime<Utc>> {
    bucket
        .get("windowEnd")
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|value| value.with_timezone(&Utc))
}

/// 没给 `windowEnd` 就不判断，直接当有效；给了就必须还没过。
fn is_active(resets_at: Option<&DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    resets_at.is_none_or(|value| *value > now)
}

fn factory_home() -> PathBuf {
    std::env::var("FACTORY_HOME_OVERRIDE")
        .ok()
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| crate::ingest::default_home().join(".factory"))
}

pub fn load_access_token() -> Result<String, String> {
    let home = factory_home();
    if let Some(token) = read_keyfile_v2(&home)? {
        return Ok(token);
    }
    if let Some(token) = read_legacy(&home) {
        return Ok(token);
    }
    Err("尚未登录 Droid，请先运行 droid 并登录 app.factory.ai".to_string())
}

fn read_keyfile_v2(home: &std::path::Path) -> Result<Option<String>, String> {
    let (Ok(payload), Ok(key)) = (
        std::fs::read_to_string(home.join("auth.v2.file")),
        std::fs::read_to_string(home.join("auth.v2.key")),
    ) else {
        return Ok(None);
    };
    let plain = decrypt_credentials(payload.trim(), key.trim())?;
    Ok(access_token_from(&plain))
}

fn read_legacy(home: &std::path::Path) -> Option<String> {
    let raw = std::fs::read_to_string(home.join("auth.json")).ok()?;
    access_token_from(&raw)
}

fn access_token_from(raw: &str) -> Option<String> {
    serde_json::from_str::<Value>(raw)
        .ok()?
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

/// `base64(iv):base64(tag):base64(密文)`，AES-256-GCM，密钥是 `auth.v2.key` 的 base64。
pub fn decrypt_credentials(payload: &str, key_b64: &str) -> Result<String, String> {
    let engine = base64::engine::general_purpose::STANDARD;
    let parts: Vec<&str> = payload.split(':').collect();
    let [iv, tag, ciphertext] = parts.as_slice() else {
        return Err("Droid 登录凭证格式不是 iv:tag:密文".to_string());
    };
    let decode = |part: &str, what: &str| {
        engine
            .decode(part)
            .map_err(|e| format!("Droid 登录凭证的{what}解码失败：{e}"))
    };
    let key = decode(key_b64, "密钥")?;
    if key.len() != 32 {
        return Err("Droid 登录凭证密钥长度不是 32 字节".to_string());
    }
    let iv = decode(iv, "iv")?;
    let tag = decode(tag, "tag")?;
    let mut sealed = decode(ciphertext, "密文")?;
    // aes-gcm 要求 tag 拼在密文尾部，droid 是分开存的。
    sealed.extend_from_slice(&tag);

    let plain = decrypt_gcm(&key, &iv, &sealed)
        .ok_or_else(|| "Droid 登录凭证解密失败，可能已被 droid 重新加密".to_string())?;
    String::from_utf8(plain).map_err(|e| format!("Droid 登录凭证不是合法 UTF-8：{e}"))
}

/// droid 用的是 16 字节 IV（GCM 允许非 96-bit），而 `Aes256Gcm` 别名固定 12 字节，
/// 所以按 IV 长度选具体的 nonce 尺寸。
fn decrypt_gcm(key: &[u8], iv: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
    let payload = || Payload {
        msg: sealed,
        aad: &[],
    };
    match iv.len() {
        12 => AesGcm::<Aes256, U12>::new(Key::<AesGcm<Aes256, U12>>::from_slice(key))
            .decrypt(Nonce::<U12>::from_slice(iv), payload())
            .ok(),
        16 => AesGcm::<Aes256, U16>::new(Key::<AesGcm<Aes256, U16>>::from_slice(key))
            .decrypt(Nonce::<U16>::from_slice(iv), payload())
            .ok(),
        _ => None,
    }
}

fn request_limits(token: &str) -> Result<String, String> {
    let request = ureq::get(LIMITS_URL)
        .timeout(TIMEOUT)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Accept", "application/json");
    match request.call() {
        Ok(response) => response
            .into_string()
            .map_err(|e| format!("读取 Droid 限额响应失败：{e}")),
        Err(ureq::Error::Status(401 | 403, _)) => {
            Err("Droid 登录已过期，请重新运行 droid 登录".to_string())
        }
        Err(ureq::Error::Status(code, response)) => {
            let _ = response.into_string();
            Err(format!("拉取 Droid 限额失败：HTTP {code}"))
        }
        Err(_) => Err("无法连接 Droid 限额接口，请检查网络后重试".to_string()),
    }
}
