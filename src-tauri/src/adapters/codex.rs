use serde_json::Value;

use crate::adapters::{finish, i64_field, parse_jsonl_values, text_field};
use crate::domain::{Source, UsageRecord};

pub fn parse_codex_jsonl(content: &str, source_file: &str) -> Vec<UsageRecord> {
    let mut session_id = String::new();
    let mut project = String::new();
    let mut provider = String::new();
    let mut model = String::new();
    let mut last_usage: Option<(i64, i64, i64, i64, i64)> = None;
    let mut records = Vec::new();

    for value in parse_jsonl_values(content) {
        let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let payload = value.get("payload").cloned().unwrap_or(Value::Null);
        let timestamp = text_field(&value, &["timestamp"]);

        match kind {
            "session_meta" => {
                session_id = text_field(&payload, &["id", "session_id"]);
                project = text_field(&payload, &["cwd"]);
                provider = text_field(&payload, &["model_provider"]);
            }
            "turn_context" => {
                let next_model = text_field(&payload, &["model"]);
                if !next_model.is_empty() {
                    model = next_model;
                }
                let cwd = text_field(&payload, &["cwd"]);
                if project.is_empty() && !cwd.is_empty() {
                    project = cwd;
                }
            }
            "event_msg" => {
                if payload.get("type").and_then(|v| v.as_str()) != Some("token_count") {
                    continue;
                }
                let info = payload.get("info").cloned().unwrap_or(Value::Null);
                let usage = info.get("last_token_usage").cloned().unwrap_or(Value::Null);
                if usage.is_null() {
                    continue;
                }
                let input = i64_field(&usage, &["input_tokens"]);
                let output = i64_field(&usage, &["output_tokens"]);
                let cache_read = i64_field(&usage, &["cached_input_tokens"]);
                let reasoning = i64_field(&usage, &["reasoning_output_tokens"]);
                let total = i64_field(&usage, &["total_tokens"]);
                let fingerprint = (input, output, cache_read, reasoning, total);
                if last_usage == Some(fingerprint) {
                    continue;
                }
                last_usage = Some(fingerprint);
                records.push(finish(UsageRecord {
                    occurred_at: timestamp,
                    source: Source::Codex,
                    model: model.clone(),
                    provider: provider.clone(),
                    project: project.clone(),
                    session_id: session_id.clone(),
                    source_file: source_file.to_string(),
                    input_tokens: input,
                    output_tokens: output,
                    cache_read_tokens: cache_read,
                    cache_creation_tokens: 0,
                    reasoning_tokens: reasoning,
                    total_tokens: total,
                    native_cost: None,
                }));
            }
            _ => {}
        }
    }

    records
}
