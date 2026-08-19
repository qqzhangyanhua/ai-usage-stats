use rusqlite::Connection;

use crate::adapters::cursor_account::{
    parse_cursor_usage_events, parse_cursor_usage_page, summarize_cursor_usage,
};
use crate::domain::{CursorAccountUsageDto, CursorUsageEvent, Filter};
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

pub fn events_page(
    conn: &Connection,
    query: &crate::domain::CursorAccountEventQuery,
) -> Result<crate::domain::CursorAccountEventPage, String> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 20_000);
    let sort_dir = query.sort_dir.as_deref().unwrap_or("desc");
    let (total, events) = store::cursor_account_events_page(conn, page, page_size, sort_dir)?;
    let rows = events
        .into_iter()
        .map(|event| {
            let total_tokens = event.total_tokens();
            crate::domain::CursorAccountEventRow {
                occurred_at: event.occurred_at,
                model: event.model,
                input_tokens: event.input_tokens,
                output_tokens: event.output_tokens,
                cache_read_tokens: event.cache_read_tokens,
                cache_creation_tokens: event.cache_creation_tokens,
                total_tokens,
                is_headless: event.is_headless,
            }
        })
        .collect();
    Ok(crate::domain::CursorAccountEventPage { rows, total })
}

pub fn load_summary(conn: &Connection) -> Result<CursorAccountUsageDto, String> {
    load_summary_filtered(conn, None)
}

pub fn load_summary_filtered(
    conn: &Connection,
    filter: Option<&Filter>,
) -> Result<CursorAccountUsageDto, String> {
    let events = store::load_cursor_account_events(conn)?;
    let filtered = match filter {
        Some(filter) => events
            .into_iter()
            .filter(|event| event_matches_filter(event, filter))
            .collect(),
        None => events,
    };
    let mut dto = summarize_cursor_usage(&filtered);
    dto.as_of = store::cursor_account_as_of(conn)?;
    Ok(dto)
}

/// 供概览 7 天滚动挂一行：只认模型筛选，不套用来源/项目/provider，也不跟总览日期预设。
pub fn events_for_weekly_window(
    conn: &Connection,
    filter: &Filter,
) -> Result<Vec<CursorUsageEvent>, String> {
    if !filter.sources.is_empty()
        && !filter
            .sources
            .iter()
            .any(|source| source == crate::billing_window::CURSOR_WEEKLY_SOURCE)
    {
        return Ok(Vec::new());
    }
    let scoped = Filter {
        from: None,
        to: None,
        sources: Vec::new(),
        models: filter.models.clone(),
        projects: Vec::new(),
        providers: Vec::new(),
    };
    let events = store::load_cursor_account_events(conn)?;
    Ok(events
        .into_iter()
        .filter(|event| event_matches_filter(event, &scoped))
        .collect())
}

/// 账号用量只认时间与模型；来源 / 项目 / provider 是本机消耗记录维度，不套到这里。
pub fn event_matches_filter(event: &CursorUsageEvent, filter: &Filter) -> bool {
    if let Some(from) = filter.from.as_deref() {
        if !timestamp_ge(&event.occurred_at, from) {
            return false;
        }
    }
    if let Some(to) = filter.to.as_deref() {
        if !timestamp_le(&event.occurred_at, to) {
            return false;
        }
    }
    if !filter.models.is_empty() && !filter.models.iter().any(|model| model == &event.model) {
        return false;
    }
    true
}

fn timestamp_ge(occurred_at: &str, bound: &str) -> bool {
    match (parse_millis(occurred_at), parse_millis(bound)) {
        (Some(value), Some(limit)) => value >= limit,
        _ => occurred_at >= bound,
    }
}

fn timestamp_le(occurred_at: &str, bound: &str) -> bool {
    match (parse_millis(occurred_at), parse_millis(bound)) {
        (Some(value), Some(limit)) => value <= limit,
        _ => occurred_at <= bound,
    }
}

fn parse_millis(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

pub fn clear_cache(conn: &Connection) -> Result<CursorAccountUsageDto, String> {
    store::clear_cursor_account_usage(conn)?;
    load_summary(conn)
}
