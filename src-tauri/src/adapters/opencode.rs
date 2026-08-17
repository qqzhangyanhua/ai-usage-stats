use serde_json::Value;

use crate::adapters::{finish, has_billable_tokens, i64_field, text_field};
use crate::domain::{Source, UsageRecord};

pub fn parse_opencode_messages(rows: &[OpencodeMessage]) -> Vec<UsageRecord> {
    rows.iter().filter_map(parse_one).collect()
}

#[derive(Debug, Clone)]
pub struct OpencodeMessage {
    pub session_id: String,
    pub source_file: String,
    pub data: Value,
}

fn parse_one(row: &OpencodeMessage) -> Option<UsageRecord> {
    if row.data.get("role").and_then(|v| v.as_str()) != Some("assistant") {
        return None;
    }
    let tokens = row.data.get("tokens").cloned().unwrap_or_default();
    if !tokens.is_object() {
        return None;
    }
    // 进行中的消息只有半截 token，与 cc-switch 一样等 time.completed 再入账。
    if row
        .data
        .get("time")
        .and_then(|t| t.get("completed"))
        .is_none()
    {
        return None;
    }
    let cache = tokens.get("cache").cloned().unwrap_or_default();
    let path = row.data.get("path").cloned().unwrap_or_default();
    let project = text_field(&path, &["root", "cwd"]);
    let occurred = row
        .data
        .get("time")
        .and_then(|t| t.get("created"))
        .and_then(|v| v.as_i64())
        .map(millis_to_rfc3339)
        .unwrap_or_default();
    let native_cost = row
        .data
        .get("cost")
        .and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_i64().map(|n| n as f64))
                .or_else(|| v.get("total").and_then(|n| n.as_f64()))
        })
        .filter(|amount| *amount > 0.0);
    let record = finish(UsageRecord {
        occurred_at: occurred,
        source: Source::Opencode,
        model: text_field(&row.data, &["modelID", "modelId"]),
        provider: text_field(&row.data, &["providerID", "providerId"]),
        project,
        session_id: row.session_id.clone(),
        source_file: row.source_file.clone(),
        input_tokens: i64_field(&tokens, &["input"]),
        output_tokens: i64_field(&tokens, &["output"]),
        cache_read_tokens: i64_field(&cache, &["read"]),
        cache_creation_tokens: i64_field(&cache, &["write"]),
        reasoning_tokens: i64_field(&tokens, &["reasoning"]),
        total_tokens: 0,
        native_cost,
    });
    has_billable_tokens(&record).then_some(record)
}

fn millis_to_rfc3339(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}
