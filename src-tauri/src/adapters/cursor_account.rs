use std::collections::BTreeMap;

use crate::adapters::i64_field;
use crate::domain::{CursorAccountUsageDto, CursorUsageEvent, NamedAmount, SeriesPoint};

pub struct CursorUsagePage {
    pub events: Vec<CursorUsageEvent>,
    pub total_count: u64,
}

/// 把 Cursor 仪表盘 `get-filtered-usage-events` 的原始 JSON 归一成账号级事件。
/// 坏 JSON 或缺少 `usageEventsDisplay` 返回可读错误；空列表返回空，不 panic。
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
        return Err("Cursor 用量接口结构已变更，请稍后再试或检查应用更新".to_string());
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
    let total_tokens = events.iter().map(CursorUsageEvent::total_tokens).sum();
    let headless_tokens = events
        .iter()
        .filter(|event| event.is_headless)
        .map(CursorUsageEvent::total_tokens)
        .sum();
    let interactive_tokens = total_tokens - headless_tokens;
    CursorAccountUsageDto {
        as_of: None,
        event_count: events.len() as i64,
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_creation_tokens,
        total_tokens,
        daily: bucket_daily(events),
        by_model: bucket_models(events, total_tokens),
        headless_tokens,
        interactive_tokens,
        headless_share: if total_tokens > 0 {
            Some(headless_tokens as f64 / total_tokens as f64)
        } else {
            None
        },
    }
}

fn bucket_models(events: &[CursorUsageEvent], grand: i64) -> Vec<NamedAmount> {
    let mut buckets: BTreeMap<String, i64> = BTreeMap::new();
    for event in events {
        *buckets.entry(event.model.clone()).or_insert(0) += event.total_tokens();
    }
    let mut rows: Vec<NamedAmount> = buckets
        .into_iter()
        .map(|(name, total_tokens)| NamedAmount {
            name,
            total_tokens,
            share: if grand == 0 {
                0.0
            } else {
                total_tokens as f64 / grand as f64
            },
            cost: None,
            unpriced: true,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| a.name.cmp(&b.name))
    });
    rows
}

fn bucket_daily(events: &[CursorUsageEvent]) -> Vec<SeriesPoint> {
    #[derive(Default)]
    struct DayAcc {
        input_tokens: i64,
        output_tokens: i64,
        cache_read_tokens: i64,
        cache_creation_tokens: i64,
        total_tokens: i64,
    }

    let mut buckets: BTreeMap<String, DayAcc> = BTreeMap::new();
    for event in events {
        let key = local_day(&event.occurred_at);
        let entry = buckets.entry(key).or_default();
        entry.input_tokens += event.input_tokens;
        entry.output_tokens += event.output_tokens;
        entry.cache_read_tokens += event.cache_read_tokens;
        entry.cache_creation_tokens += event.cache_creation_tokens;
        entry.total_tokens += event.total_tokens();
    }
    buckets
        .into_iter()
        .map(|(bucket, acc)| SeriesPoint {
            bucket,
            total_tokens: acc.total_tokens,
            input_tokens: acc.input_tokens,
            output_tokens: acc.output_tokens,
            cache_read_tokens: acc.cache_read_tokens,
            cache_creation_tokens: acc.cache_creation_tokens,
            reasoning_tokens: 0,
            cost: None,
        })
        .collect()
}

fn local_day(occurred_at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(occurred_at)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|_| occurred_at.get(..10).unwrap_or(occurred_at).to_string())
}
