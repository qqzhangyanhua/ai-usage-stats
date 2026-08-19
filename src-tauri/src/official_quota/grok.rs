use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::domain::OfficialQuotaWindow;
use crate::official_quota::{parse_resets_at, sanitize_percent};

const BILLING_CREDITS_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const BILLING_MONTHLY_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing";
const TIMEOUT: Duration = Duration::from_secs(12);
const LEGACY_SCOPE: &str = "https://accounts.x.ai/sign-in";
const SUPERGROK_SCOPE_PREFIX: &str = "https://auth.x.ai";
const API_KEY_SCOPE: &str = "xai::api_key";

pub fn fetch_rate_limits() -> Result<(Vec<OfficialQuotaWindow>, String), String> {
    let token = load_auth_token()?;
    let credits_raw = request_billing(&token, BILLING_CREDITS_URL)?;
    let mut windows = parse_credits(&credits_raw)?;
    if let Ok(monthly_raw) = request_billing(&token, BILLING_MONTHLY_URL) {
        if let Ok(monthly) = parse_monthly(&monthly_raw) {
            merge_windows(&mut windows, monthly);
        }
    }
    if windows.is_empty() {
        return Err("Grok 限额响应里没有可用的已用百分比".to_string());
    }
    Ok((windows, Utc::now().to_rfc3339()))
}

/// 解析 `GET /v1/billing?format=credits`：周额度池 + Grok Build 分项 + 按需。
pub fn parse_credits(raw: &str) -> Result<Vec<OfficialQuotaWindow>, String> {
    let value = parse_object(raw, "Grok 周额度")?;
    let config = value.get("config").unwrap_or(&value);
    let weekly_resets = period_end(config);
    let weekly_period = is_weekly_period(config);
    let mut windows = Vec::new();

    let weekly_percent = named_percent(config, "creditUsagePercent");
    let build_percent = product_percent(config, "GrokBuild");
    if let Some(percent) = weekly_percent {
        push_window(
            &mut windows,
            Some(percent),
            "weekly",
            "周额度",
            weekly_resets.clone(),
        );
        push_window(
            &mut windows,
            build_percent,
            "product_grokbuild",
            "Grok Build",
            weekly_resets.clone(),
        );
    } else if let Some(percent) = build_percent {
        push_window(
            &mut windows,
            Some(percent),
            "weekly",
            "周额度",
            weekly_resets.clone(),
        );
    } else if weekly_period {
        push_window(
            &mut windows,
            Some(0.0),
            "weekly",
            "周额度",
            weekly_resets.clone(),
        );
    }

    if let Some(window) = parse_on_demand(config, weekly_resets) {
        windows.push(window);
    }
    Ok(windows)
}

/// 解析 `GET /v1/billing`：月度 included 额度。缺 used 不当成 0%。
pub fn parse_monthly(raw: &str) -> Result<Vec<OfficialQuotaWindow>, String> {
    let value = parse_object(raw, "Grok 月额度")?;
    let config = value.get("config").unwrap_or(&value);
    let used = money_val(config.get("used"))
        .or_else(|| money_val(value.pointer("/usage/totalUsed")))
        .or_else(|| money_val(value.pointer("/usage/includedUsed")));
    let limit =
        money_val(config.get("monthlyLimit")).or_else(|| money_val(value.get("monthlyLimit")));
    let Some((used, limit)) = used.zip(limit) else {
        return Ok(Vec::new());
    };
    if limit <= 0.0 {
        return Ok(Vec::new());
    }
    let percent = sanitize_percent((used / limit * 100.0).clamp(0.0, 100.0));
    let resets_at = period_end(config).or_else(|| period_end(&value));
    let mut windows = Vec::new();
    push_window(&mut windows, percent, "monthly", "月额度", resets_at);
    Ok(windows)
}

pub fn parse_auth_json(raw: &str, now: DateTime<Utc>) -> Result<String, String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|_| "Grok 登录凭证无效，请重新运行 grok login".to_string())?;
    let Some(map) = value.as_object() else {
        return Err("Grok 登录凭证无效，请重新运行 grok login".to_string());
    };

    let mut expired_only = false;
    let mut saw_api_key_only = true;
    let mut saw_any = false;
    let mut preferred = None;
    let mut legacy = None;
    let mut other = None;

    for (scope, node) in map {
        if !node.is_object() {
            continue;
        }
        if is_blocked_mode(node) {
            continue;
        }
        let Some(token) = token_of(node) else {
            continue;
        };
        saw_any = true;
        if is_expired(node, now) {
            expired_only = true;
            continue;
        }
        if is_api_key_entry(scope, node) {
            continue;
        }
        saw_api_key_only = false;
        if scope.starts_with(SUPERGROK_SCOPE_PREFIX) {
            preferred = Some(token);
            break;
        }
        if scope == LEGACY_SCOPE {
            legacy = Some(token);
        } else {
            other = Some(token);
        }
    }

    if let Some(token) = preferred.or(legacy).or(other) {
        return Ok(token);
    }
    if saw_any && saw_api_key_only && !expired_only {
        return Err(
            "Grok 官方额度需要 grok login 的会话登录，API key 无法查询订阅限额".to_string(),
        );
    }
    if expired_only {
        return Err("Grok 登录已过期，请重新运行 grok login".to_string());
    }
    Err("Grok 登录凭证无效，请重新运行 grok login".to_string())
}

fn load_auth_token() -> Result<String, String> {
    let path = auth_path();
    if !path.exists() {
        return Err("尚未登录 Grok CLI，请先运行 grok login".to_string());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取 Grok 登录凭证失败：{e}"))?;
    parse_auth_json(&raw, Utc::now())
}

fn auth_path() -> PathBuf {
    grok_home().join("auth.json")
}

fn grok_home() -> PathBuf {
    std::env::var("GROK_HOME")
        .ok()
        .and_then(|raw| {
            raw.split(',')
                .map(str::trim)
                .find(|segment| !segment.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| crate::ingest::default_home().join(".grok"))
}

fn request_billing(token: &str, url: &str) -> Result<String, String> {
    let request = ureq::get(url)
        .timeout(TIMEOUT)
        .set("Authorization", &format!("Bearer {token}"))
        .set("X-XAI-Token-Auth", "xai-grok-cli")
        .set("Accept", "application/json");
    match request.call() {
        Ok(response) => response
            .into_string()
            .map_err(|e| format!("读取 Grok 限额响应失败：{e}")),
        Err(ureq::Error::Status(401 | 403, _)) => {
            Err("Grok 登录已过期，请重新运行 grok login".to_string())
        }
        Err(ureq::Error::Status(code, response)) => {
            let _ = response.into_string();
            Err(format!("拉取 Grok 限额失败：HTTP {code}"))
        }
        Err(_) => Err("无法连接 Grok 限额接口，请检查网络后重试".to_string()),
    }
}

fn parse_object(raw: &str, label: &str) -> Result<Value, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("{label} JSON 解析失败：{e}"))?;
    if !value.is_object() {
        return Err(format!("{label} JSON 不是对象"));
    }
    Ok(value)
}

fn named_percent(node: &Value, field: &str) -> Option<f64> {
    node.get(field)
        .and_then(Value::as_f64)
        .or_else(|| node.get(field).and_then(Value::as_i64).map(|n| n as f64))
        .and_then(sanitize_percent)
}

fn product_percent(config: &Value, product: &str) -> Option<f64> {
    let products = config.get("productUsage")?.as_array()?;
    products.iter().find_map(|item| {
        let name = item.get("product").and_then(Value::as_str)?;
        if !name.eq_ignore_ascii_case(product) {
            return None;
        }
        named_percent(item, "usagePercent")
    })
}

fn parse_on_demand(config: &Value, resets_at: Option<String>) -> Option<OfficialQuotaWindow> {
    let used = money_val(config.get("onDemandUsed"))?;
    let cap = money_val(config.get("onDemandCap"))?;
    if cap <= 0.0 {
        return None;
    }
    let percent = sanitize_percent((used / cap * 100.0).clamp(0.0, 100.0))?;
    Some(OfficialQuotaWindow {
        kind: "on_demand".to_string(),
        label: "按需".to_string(),
        used_percent: Some(percent),
        resets_at,
    })
}

fn money_val(node: Option<&Value>) -> Option<f64> {
    let node = node?;
    if let Some(n) = node.as_f64() {
        return Some(n);
    }
    if let Some(n) = node.as_i64() {
        return Some(n as f64);
    }
    node.get("val")
        .and_then(|val| val.as_f64().or_else(|| val.as_i64().map(|n| n as f64)))
}

fn period_end(node: &Value) -> Option<String> {
    node.pointer("/currentPeriod/end")
        .or_else(|| node.get("billingPeriodEnd"))
        .or_else(|| node.pointer("/billingCycle/billingPeriodEnd"))
        .and_then(parse_resets_at)
}

fn is_weekly_period(config: &Value) -> bool {
    let period = match config.get("currentPeriod") {
        Some(period) if period.is_object() => period,
        _ => return false,
    };
    let kind = period.get("type").and_then(Value::as_str).unwrap_or("");
    kind == "USAGE_PERIOD_TYPE_WEEKLY" && period.get("end").and_then(parse_resets_at).is_some()
}

fn push_window(
    windows: &mut Vec<OfficialQuotaWindow>,
    percent: Option<f64>,
    kind: &str,
    label: &str,
    resets_at: Option<String>,
) {
    let Some(percent) = percent else {
        return;
    };
    windows.push(OfficialQuotaWindow {
        kind: kind.to_string(),
        label: label.to_string(),
        used_percent: Some(percent),
        resets_at,
    });
}

fn merge_windows(into: &mut Vec<OfficialQuotaWindow>, extra: Vec<OfficialQuotaWindow>) {
    for window in extra {
        if into.iter().any(|existing| existing.kind == window.kind) {
            continue;
        }
        into.push(window);
    }
}

fn token_of(node: &Value) -> Option<String> {
    ["key", "access_token"].into_iter().find_map(|field| {
        node.get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn is_blocked_mode(node: &Value) -> bool {
    matches!(
        node.get("auth_mode").and_then(Value::as_str),
        Some("web_login" | "grok")
    )
}

fn is_api_key_entry(scope: &str, node: &Value) -> bool {
    scope == API_KEY_SCOPE || node.get("auth_mode").and_then(Value::as_str) == Some("api_key")
}

fn is_expired(node: &Value, now: DateTime<Utc>) -> bool {
    let Some(raw) = node.get("expires_at") else {
        return false;
    };
    let Some(expires) = parse_resets_at(raw)
        .and_then(|text| DateTime::parse_from_rfc3339(&text).ok())
        .map(|dt| dt.with_timezone(&Utc))
    else {
        return false;
    };
    now >= expires
}
