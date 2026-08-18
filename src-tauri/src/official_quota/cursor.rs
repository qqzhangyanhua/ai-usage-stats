use chrono::Utc;
use serde_json::Value;

use crate::cursor_account;
use crate::domain::OfficialQuotaWindow;
use crate::official_quota::{parse_resets_at, sanitize_percent};

const USAGE_SUMMARY_URL: &str = "https://cursor.com/api/usage-summary";

pub fn fetch_usage_summary() -> Result<(Vec<OfficialQuotaWindow>, String), String> {
    let token = cursor_account::load_token()?.ok_or_else(|| {
        "尚未配置 Cursor 会话 token，请先在设置页粘贴 WorkosCursorSessionToken".to_string()
    })?;
    let raw = request_usage_summary(&token)?;
    let windows = parse_usage_summary(&raw)?;
    Ok((windows, Utc::now().to_rfc3339()))
}

pub fn parse_usage_summary(raw: &str) -> Result<Vec<OfficialQuotaWindow>, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("Cursor 限额 JSON 解析失败：{e}"))?;
    let plan = value
        .pointer("/individualUsage/plan")
        .or_else(|| value.get("plan"))
        .ok_or_else(|| "Cursor 限额接口结构已变更，请稍后再试或检查应用更新".to_string())?;
    let percent = plan
        .get("totalPercentUsed")
        .and_then(Value::as_f64)
        .or_else(|| percent_from_used_limit(plan))
        .and_then(sanitize_percent);
    let resets_at = value.get("billingCycleEnd").and_then(parse_resets_at);
    if percent.is_none() && resets_at.is_none() {
        return Err("Cursor 限额响应里没有可用的已用百分比".to_string());
    }
    Ok(vec![OfficialQuotaWindow {
        kind: "billing_cycle".to_string(),
        label: "账期".to_string(),
        used_percent: percent,
        resets_at,
    }])
}

fn percent_from_used_limit(plan: &Value) -> Option<f64> {
    let used = plan.get("used").and_then(Value::as_f64)?;
    let limit = plan.get("limit").and_then(Value::as_f64)?;
    if limit <= 0.0 {
        return None;
    }
    Some(used / limit * 100.0)
}

fn request_usage_summary(token: &str) -> Result<String, String> {
    let request = ureq::get(USAGE_SUMMARY_URL)
        .set(
            "Cookie",
            &format!(
                "WorkosCursorSessionToken={}",
                cursor_account::normalize_token(token)
            ),
        )
        .set("Origin", "https://cursor.com");
    match request.call() {
        Ok(response) => response
            .into_string()
            .map_err(|e| format!("读取 Cursor 限额响应失败：{e}")),
        Err(ureq::Error::Status(401 | 403, _)) => Err(cursor_account::auth_expired_error()),
        Err(ureq::Error::Status(code, response)) => {
            let _ = response.into_string();
            Err(format!("拉取 Cursor 限额失败：HTTP {code}"))
        }
        Err(_) => Err(cursor_account::network_failure_error()),
    }
}
