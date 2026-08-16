use std::collections::HashMap;

use crate::adapters::{finish, i64_field, parse_jsonl_values, text_field};
use crate::domain::{Source, UsageRecord};

pub fn parse_kimi_wire(content: &str, source_file: &str, project: &str) -> Vec<UsageRecord> {
    let session_id = std::path::Path::new(source_file)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let mut last_by_message: HashMap<String, UsageRecord> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for value in parse_jsonl_values(content) {
        let message = value.get("message").cloned().unwrap_or_default();
        if message.get("type").and_then(|v| v.as_str()) != Some("StatusUpdate") {
            continue;
        }
        let payload = message.get("payload").cloned().unwrap_or_default();
        let usage = payload.get("token_usage").cloned().unwrap_or_default();
        if usage.is_null() {
            continue;
        }
        let message_id = text_field(&payload, &["message_id"]);
        if message_id.is_empty() {
            continue;
        }
        let occurred = value
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .map(|secs| {
                chrono::DateTime::from_timestamp(secs as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        if !last_by_message.contains_key(&message_id) {
            order.push(message_id.clone());
        }
        last_by_message.insert(
            message_id,
            finish(UsageRecord {
                occurred_at: occurred,
                source: Source::Kimi,
                model: String::new(),
                provider: String::new(),
                project: project.to_string(),
                session_id: session_id.clone(),
                source_file: source_file.to_string(),
                input_tokens: i64_field(&usage, &["input_other"]),
                output_tokens: i64_field(&usage, &["output"]),
                cache_read_tokens: i64_field(&usage, &["input_cache_read"]),
                cache_creation_tokens: i64_field(&usage, &["input_cache_creation"]),
                reasoning_tokens: 0,
                total_tokens: 0,
                native_cost: None,
            }),
        );
    }

    order
        .into_iter()
        .filter_map(|id| last_by_message.remove(&id))
        .collect()
}
