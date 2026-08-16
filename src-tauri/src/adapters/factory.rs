use serde_json::Value;

use crate::adapters::project::decode_dashed_dir;
use crate::adapters::{finish, i64_field, text_field};
use crate::domain::{Source, UsageRecord};

pub fn parse_factory_settings(content: &str, source_file: &str) -> Vec<UsageRecord> {
    let value: Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let usage = match value.get("tokenUsage") {
        Some(v) if !v.is_null() => v.clone(),
        _ => return Vec::new(),
    };
    let file_name = std::path::Path::new(source_file)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let session_id = file_name
        .strip_suffix(".settings.json")
        .unwrap_or(file_name)
        .to_string();
    let project = std::path::Path::new(source_file)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(decode_dashed_dir)
        .unwrap_or_default();
    vec![finish(UsageRecord {
        occurred_at: text_field(&value, &["providerLockTimestamp"]),
        source: Source::Factory,
        model: String::new(),
        provider: text_field(&value, &["providerLock"]),
        project,
        session_id,
        source_file: source_file.to_string(),
        input_tokens: i64_field(&usage, &["inputTokens"]),
        output_tokens: i64_field(&usage, &["outputTokens"]),
        cache_read_tokens: i64_field(&usage, &["cacheReadTokens"]),
        cache_creation_tokens: i64_field(&usage, &["cacheCreationTokens"]),
        reasoning_tokens: i64_field(&usage, &["thinkingTokens"]),
        total_tokens: 0,
        native_cost: None,
    })]
}
