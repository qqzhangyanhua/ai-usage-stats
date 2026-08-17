use crate::adapters::i64_field;
use crate::domain::{CursorAccountUsageDto, CursorUsageEvent};

pub struct CursorUsagePage {
    pub events: Vec<CursorUsageEvent>,
    pub total_count: u64,
}

/// 把 Cursor 仪表盘 `get-filtered-usage-events` 的原始 JSON 归一成账号级事件。
/// 坏 JSON 返回可读错误；缺 `usageEventsDisplay` 或空列表返回空，不 panic。
pub fn parse_cursor_usage_events(raw: &str) -> Result<Vec<CursorUsageEvent>, String> {
    Ok(parse_cursor_usage_page(raw)?.events)
}

pub fn parse_cursor_usage_page(raw: &str) -> Result<CursorUsagePage, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("Cursor 账号用量 JSON 解析失败：{e}"))?;
    let total_count = value
        .get("totalUsageEventsCount")
        .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n.max(0) as u64)))
        .unwrap_or(0);
    let Some(items) = value.get("usageEventsDisplay").and_then(|v| v.as_array()) else {
        return Ok(CursorUsagePage {
            events: Vec::new(),
            total_count,
        });
    };

    let mut events = Vec::new();
    for item in items {
        let Some(event) = parse_one(item) else {
            continue;
        };
        events.push(event);
    }
    Ok(CursorUsagePage {
        events,
        total_count,
    })
}

fn parse_one(item: &serde_json::Value) -> Option<CursorUsageEvent> {
    let occurred_at = timestamp_to_rfc3339(item.get("timestamp")?)?;
    let model = item.get("model")?.as_str()?.to_string();
    if model.is_empty() {
        return None;
    }
    let usage = item.get("tokenUsage").unwrap_or(&serde_json::Value::Null);
    Some(CursorUsageEvent {
        occurred_at,
        model,
        input_tokens: i64_field(usage, &["inputTokens"]),
        output_tokens: i64_field(usage, &["outputTokens"]),
        cache_read_tokens: i64_field(usage, &["cacheReadTokens"]),
        cache_creation_tokens: i64_field(usage, &["cacheWriteTokens"]),
        is_headless: item
            .get("isHeadless")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

fn timestamp_to_rfc3339(value: &serde_json::Value) -> Option<String> {
    let millis = value
        .as_i64()
        .or_else(|| value.as_u64().map(|n| n as i64))
        .or_else(|| value.as_str()?.parse::<i64>().ok())?;
    chrono::DateTime::from_timestamp_millis(millis).map(|dt| dt.to_rfc3339())
}

pub fn summarize_cursor_usage(events: &[CursorUsageEvent]) -> CursorAccountUsageDto {
    if events.is_empty() {
        return CursorAccountUsageDto::empty();
    }
    let input_tokens = events.iter().map(|e| e.input_tokens).sum();
    let output_tokens = events.iter().map(|e| e.output_tokens).sum();
    let cache_read_tokens = events.iter().map(|e| e.cache_read_tokens).sum();
    let cache_creation_tokens = events.iter().map(|e| e.cache_creation_tokens).sum();
    CursorAccountUsageDto {
        as_of: None,
        event_count: events.len() as i64,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        total_tokens: events.iter().map(CursorUsageEvent::total_tokens).sum(),
    }
}
