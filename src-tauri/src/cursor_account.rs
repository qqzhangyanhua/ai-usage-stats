use rusqlite::Connection;

use crate::adapters::cursor_account::{
    parse_cursor_usage_events, parse_cursor_usage_page, summarize_cursor_usage,
};
use crate::domain::CursorAccountUsageDto;
use crate::store;

const KEYRING_SERVICE: &str = "ai-usage-stats";
const KEYRING_ACCOUNT: &str = "cursor-session-token";
const USAGE_EVENTS_URL: &str = "https://cursor.com/api/dashboard/get-filtered-usage-events";
const PAGE_SIZE: u32 = 100;

pub fn normalize_token(raw: &str) -> String {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix("WorkosCursorSessionToken=")
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

pub fn save_token(token: &str) -> Result<(), String> {
    let token = normalize_token(token);
    if token.is_empty() {
        return Err("Cursor 会话 token 不能为空".to_string());
    }
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| format!("打开钥匙串失败：{e}"))?;
    entry
        .set_password(&token)
        .map_err(|e| format!("写入钥匙串失败：{e}"))
}

pub fn load_token() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|e| format!("打开钥匙串失败：{e}"))?;
    match entry.get_password() {
        Ok(value) if !value.is_empty() => Ok(Some(value)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(format!("读取钥匙串失败：{error}")),
    }
}

pub fn has_token() -> Result<bool, String> {
    Ok(load_token()?.is_some())
}

pub fn incremental_start_ms(conn: &Connection) -> Result<i64, String> {
    Ok(store::max_cursor_account_occurred_ms(conn)?.unwrap_or(0))
}

pub fn auth_expired_error() -> String {
    "Cursor 会话已过期，请重新粘贴 WorkosCursorSessionToken".to_string()
}

pub fn network_failure_error() -> String {
    "无法连接 Cursor 用量接口，请检查网络后重试".to_string()
}

pub fn ingest_raw_pages(conn: &Connection, pages: &[&str]) -> Result<u64, String> {
    let mut events = Vec::new();
    for raw in pages {
        events.extend(parse_cursor_usage_events(raw)?);
    }
    store::upsert_cursor_account_events(conn, &events)
}

pub fn apply_fetched_pages(
    conn: &Connection,
    fetched: Result<Vec<String>, String>,
) -> Result<CursorAccountUsageDto, String> {
    let pages = fetched?;
    let refs: Vec<&str> = pages.iter().map(String::as_str).collect();
    ingest_raw_pages(conn, &refs)?;
    store::set_cursor_account_as_of(conn, &chrono::Utc::now().to_rfc3339())?;
    load_summary(conn)
}

pub fn fetch_usage_events_page(
    token: &str,
    page: u32,
    start_date_ms: i64,
) -> Result<String, String> {
    let body = serde_json::json!({
        "page": page,
        "pageSize": PAGE_SIZE,
        "startDate": start_date_ms
    });
    let request = ureq::post(USAGE_EVENTS_URL)
        .set(
            "Cookie",
            &format!("WorkosCursorSessionToken={}", normalize_token(token)),
        )
        .set("Origin", "https://cursor.com")
        .set("Content-Type", "application/json");
    match request.send_string(&body.to_string()) {
        Ok(response) => response
            .into_string()
            .map_err(|e| format!("读取 Cursor 账号用量响应失败：{e}")),
        Err(ureq::Error::Status(401 | 403, _)) => Err(auth_expired_error()),
        Err(ureq::Error::Status(code, response)) => {
            let _ = response.into_string();
            Err(format!("拉取 Cursor 账号用量失败：HTTP {code}"))
        }
        Err(_) => Err(network_failure_error()),
    }
}

pub fn fetch_refresh_pages(token: &str, start_date_ms: i64) -> Result<Vec<String>, String> {
    let mut page = 1u32;
    let mut pages = Vec::new();
    loop {
        let raw = fetch_usage_events_page(token, page, start_date_ms)?;
        let parsed = parse_cursor_usage_page(&raw)?;
        let page_len = parsed.events.len();
        let total = parsed.total_count;
        pages.push(raw);
        let last_page = page_len == 0 || page_len < PAGE_SIZE as usize;
        let reached_total = total > 0 && u64::from(page) * u64::from(PAGE_SIZE) >= total;
        if last_page || reached_total {
            break;
        }
        page += 1;
    }
    Ok(pages)
}

pub fn resolve_session_token(token: Option<String>) -> Result<String, String> {
    match token
        .as_deref()
        .map(normalize_token)
        .filter(|value| !value.is_empty())
    {
        Some(value) => {
            save_token(&value)?;
            Ok(value)
        }
        None => load_token()?.ok_or_else(|| {
            "尚未配置 Cursor 会话 token，请先粘贴 WorkosCursorSessionToken".to_string()
        }),
    }
}

pub fn load_summary(conn: &Connection) -> Result<CursorAccountUsageDto, String> {
    let events = store::load_cursor_account_events(conn)?;
    let mut dto = summarize_cursor_usage(&events);
    dto.as_of = store::cursor_account_as_of(conn)?;
    Ok(dto)
}
