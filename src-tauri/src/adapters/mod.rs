pub mod claude;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod cursor_account;
pub mod cursor_agent;
pub mod cursor_session;
pub mod dsh;
pub mod factory;
pub mod gemini;
pub mod grok;
pub mod kimi;
pub mod opencode;
pub mod pi;
pub mod project;
pub mod qwen;

use crate::domain::UsageRecord;

pub fn parse_jsonl_values(content: &str) -> Vec<serde_json::Value> {
    content
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            serde_json::from_str(line).ok()
        })
        .collect()
}

pub fn i64_field(value: &serde_json::Value, keys: &[&str]) -> i64 {
    for key in keys {
        if let Some(n) = value.get(key).and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_u64().map(|n| n as i64))
                .or_else(|| v.as_f64().map(|n| n.round() as i64))
        }) {
            return n;
        }
    }
    0
}

pub fn text_field(value: &serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(s) = value.get(*key).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

pub fn finish(record: UsageRecord) -> UsageRecord {
    record.with_total()
}

pub fn has_billable_tokens(record: &UsageRecord) -> bool {
    record.input_tokens > 0
        || record.output_tokens > 0
        || record.cache_read_tokens > 0
        || record.cache_creation_tokens > 0
        || record.reasoning_tokens > 0
}
