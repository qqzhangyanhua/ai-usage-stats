pub mod claude;
pub mod codex;
pub mod cursor;
pub mod droid;
pub mod grok;
pub(crate) mod grok_grpc;
pub mod hook;
pub mod notify;

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::domain::{
    OfficialQuotaConfig, OfficialQuotaDto, OfficialQuotaFreshness, OfficialQuotaProvider,
    OfficialQuotaRow, OfficialQuotaWindow,
};
use crate::store;

pub const STALE_AFTER_MINUTES: i64 = 10;
pub const CONFIG_NAME: &str = "official_quota.json";
pub const NOTIFY_NAME: &str = "official_quota_notify_state.json";
pub const CAPTURE_NAME: &str = "claude_statusline.json";

pub fn capture_path() -> PathBuf {
    crate::paths::app_data_dir().join(CAPTURE_NAME)
}

pub fn load_config(path: &Path) -> OfficialQuotaConfig {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_config(path: &Path, config: &OfficialQuotaConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub fn freshness(captured_at: &str, now: DateTime<Utc>) -> OfficialQuotaFreshness {
    if captured_at.is_empty() {
        return OfficialQuotaFreshness::Unavailable;
    }
    let Ok(captured) = DateTime::parse_from_rfc3339(captured_at) else {
        return OfficialQuotaFreshness::Unavailable;
    };
    if now - captured.with_timezone(&Utc) > Duration::minutes(STALE_AFTER_MINUTES) {
        OfficialQuotaFreshness::Stale
    } else {
        OfficialQuotaFreshness::Official
    }
}

pub fn load_dto(
    conn: &Connection,
    config: &OfficialQuotaConfig,
    now: DateTime<Utc>,
) -> OfficialQuotaDto {
    let rows = OfficialQuotaProvider::ALL
        .into_iter()
        .map(|provider| load_row(conn, provider, now))
        .collect();
    OfficialQuotaDto {
        rows,
        alerts_enabled: config.alerts_enabled,
        stale_after_minutes: STALE_AFTER_MINUTES,
    }
}

fn load_row(
    conn: &Connection,
    provider: OfficialQuotaProvider,
    now: DateTime<Utc>,
) -> OfficialQuotaRow {
    match store::load_official_quota_row(conn, provider.as_str()) {
        Ok(Some((windows, captured_at, error))) => {
            let freshness = if windows.is_empty() && captured_at.is_empty() {
                OfficialQuotaFreshness::Unavailable
            } else {
                freshness(&captured_at, now)
            };
            OfficialQuotaRow {
                provider: provider.as_str().to_string(),
                application: provider.display_name().to_string(),
                windows,
                freshness,
                captured_at: if captured_at.is_empty() {
                    None
                } else {
                    Some(captured_at)
                },
                error,
            }
        }
        Ok(None) => empty_row(provider, None),
        Err(error) => empty_row(provider, Some(error)),
    }
}

fn empty_row(provider: OfficialQuotaProvider, error: Option<String>) -> OfficialQuotaRow {
    OfficialQuotaRow {
        provider: provider.as_str().to_string(),
        application: provider.display_name().to_string(),
        windows: Vec::new(),
        freshness: OfficialQuotaFreshness::Unavailable,
        captured_at: None,
        error,
    }
}

pub fn apply_success(
    conn: &Connection,
    provider: OfficialQuotaProvider,
    windows: Vec<OfficialQuotaWindow>,
    captured_at: &str,
) -> Result<(), String> {
    store::upsert_official_quota(conn, provider.as_str(), &windows, captured_at, None)
}

pub fn apply_failure(
    conn: &Connection,
    provider: OfficialQuotaProvider,
    error: &str,
) -> Result<(), String> {
    store::set_official_quota_error(conn, provider.as_str(), error)
}

pub fn parse_provider(value: &str) -> Result<OfficialQuotaProvider, String> {
    OfficialQuotaProvider::parse(value).ok_or_else(|| format!("未知的官方额度账号：{value}"))
}

pub type ProviderFetch = Result<(Vec<OfficialQuotaWindow>, String), String>;

pub fn fetch_provider(provider: OfficialQuotaProvider) -> ProviderFetch {
    match provider {
        OfficialQuotaProvider::Claude => claude::refresh_from_capture(&capture_path()),
        OfficialQuotaProvider::Codex => codex::fetch_rate_limits(),
        OfficialQuotaProvider::Cursor => cursor::fetch_usage_summary(),
        OfficialQuotaProvider::Grok => grok::fetch_rate_limits(),
        OfficialQuotaProvider::Droid => droid::fetch_rate_limits(),
    }
}

/// 先取数再交给调用方加锁写入，避免在持锁期间打网络。
pub fn fetch_all_providers() -> Vec<(OfficialQuotaProvider, ProviderFetch)> {
    OfficialQuotaProvider::ALL
        .into_iter()
        .map(|provider| (provider, fetch_provider(provider)))
        .collect()
}

/// 打开总览或手动刷新时尝试更新各路；取数在调用方锁外完成，写入彼此隔离。
pub fn apply_fetch_results(
    conn: &Connection,
    results: impl IntoIterator<
        Item = (
            OfficialQuotaProvider,
            Result<(Vec<OfficialQuotaWindow>, String), String>,
        ),
    >,
) -> Result<(), String> {
    for (provider, result) in results {
        apply_result(conn, provider, result)?;
    }
    Ok(())
}

fn apply_result(
    conn: &Connection,
    provider: OfficialQuotaProvider,
    result: Result<(Vec<OfficialQuotaWindow>, String), String>,
) -> Result<(), String> {
    match result {
        Ok((windows, captured_at)) => apply_success(conn, provider, windows, &captured_at),
        Err(error) => apply_failure(conn, provider, &error),
    }
}

/// 捕获文件比缓存新时写入 sqlite，返回是否发生了更新。
pub fn sync_claude_capture(conn: &Connection) -> Result<bool, String> {
    let path = capture_path();
    if !path.exists() {
        return Ok(false);
    }
    let cached = store::load_official_quota_row(conn, OfficialQuotaProvider::Claude.as_str())?;
    let file_stamp = claude::file_captured_at(&path)?;
    if let Some((_, captured_at, _)) = &cached {
        if !captured_at.is_empty() && captured_at == &file_stamp {
            return Ok(false);
        }
    }
    match claude::refresh_from_capture(&path) {
        Ok((windows, captured_at)) => {
            apply_success(conn, OfficialQuotaProvider::Claude, windows, &captured_at)?;
            Ok(true)
        }
        Err(error) => {
            apply_failure(conn, OfficialQuotaProvider::Claude, &error)?;
            Ok(false)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TightestQuota {
    pub provider: String,
    pub label: String,
    pub used_percent: f64,
    pub stale: bool,
}

pub fn tightest_window(dto: &OfficialQuotaDto) -> Option<TightestQuota> {
    let mut best: Option<TightestQuota> = None;
    for row in &dto.rows {
        let stale = match row.freshness {
            OfficialQuotaFreshness::Official => false,
            OfficialQuotaFreshness::Stale => true,
            OfficialQuotaFreshness::Unavailable => continue,
        };
        for window in &row.windows {
            let Some(percent) = window.used_percent else {
                continue;
            };
            let candidate = TightestQuota {
                provider: row.application.clone(),
                label: short_label(&window.kind, &window.label),
                used_percent: percent,
                stale,
            };
            let take = match &best {
                None => true,
                Some(current) => {
                    (!stale && current.stale)
                        || (stale == current.stale && percent > current.used_percent)
                }
            };
            if take {
                best = Some(candidate);
            }
        }
    }
    best
}

fn short_label(kind: &str, label: &str) -> String {
    match kind {
        "session_5h" => "5h".to_string(),
        "weekly" => "7d".to_string(),
        "monthly" => "月".to_string(),
        "billing_cycle" => "总量".to_string(),
        "auto" => "Auto".to_string(),
        "api" => "API".to_string(),
        "on_demand" => "按需".to_string(),
        "product_grokbuild" => "Build".to_string(),
        _ => label.to_string(),
    }
}

pub fn parse_resets_at(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        if DateTime::parse_from_rfc3339(text).is_ok() {
            return Some(if text.ends_with('Z') {
                text.to_string()
            } else {
                DateTime::parse_from_rfc3339(text)
                    .ok()?
                    .with_timezone(&Utc)
                    .to_rfc3339()
            });
        }
        if let Ok(secs) = text.parse::<i64>() {
            return unix_to_rfc3339(secs);
        }
        return None;
    }
    if let Some(secs) = value.as_i64() {
        return unix_to_rfc3339(secs);
    }
    if let Some(secs) = value.as_f64() {
        return unix_to_rfc3339(secs as i64);
    }
    None
}

fn unix_to_rfc3339(raw: i64) -> Option<String> {
    let secs = if raw > 1_000_000_000_000 {
        raw / 1000
    } else {
        raw
    };
    DateTime::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
}

pub fn sanitize_percent(value: f64) -> Option<f64> {
    if (0.0..=100.0).contains(&value) {
        Some(value)
    } else {
        None
    }
}
