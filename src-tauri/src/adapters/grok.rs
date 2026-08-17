use std::collections::HashMap;

use crate::adapters::project::decode_url_dir;
use crate::adapters::{finish, parse_jsonl_values, text_field};
use crate::domain::{Source, UsageRecord};

pub fn parse_grok_updates(
    content: &str,
    source_file: &str,
    fallback_model: &str,
) -> Vec<UsageRecord> {
    let session_id = std::path::Path::new(source_file)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let project = std::path::Path::new(source_file)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(decode_url_dir)
        .unwrap_or_default();

    let mut last_by_prompt: HashMap<String, UsageRecord> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut model = fallback_model.to_string();

    for value in parse_jsonl_values(content) {
        let params = value.get("params").cloned().unwrap_or_default();
        let update = params.get("update").cloned().unwrap_or_default();
        let update_meta = update.get("_meta").cloned().unwrap_or_default();
        let next_model = text_field(&update_meta, &["modelId"]);
        if !next_model.is_empty() {
            model = next_model;
        }
        let meta = params.get("_meta").cloned().unwrap_or_default();
        let prompt_id = text_field(&meta, &["promptId"]);
        let total = meta
            .get("totalTokens")
            .and_then(|v| v.as_i64().or_else(|| v.as_u64().map(|n| n as i64)));
        if prompt_id.is_empty() || total.is_none() {
            continue;
        }
        let occurred = meta
            .get("agentTimestampMs")
            .and_then(|v| v.as_i64())
            .map(millis_to_rfc3339)
            .unwrap_or_default();
        if !last_by_prompt.contains_key(&prompt_id) {
            order.push(prompt_id.clone());
        }
        last_by_prompt.insert(
            prompt_id,
            finish(UsageRecord {
                occurred_at: occurred,
                source: Source::Grok,
                model: model.clone(),
                provider: String::new(),
                project: project.clone(),
                session_id: session_id.clone(),
                source_file: source_file.to_string(),
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                reasoning_tokens: 0,
                total_tokens: total.unwrap_or(0),
                native_cost: None,
            }),
        );
    }

    order
        .into_iter()
        .filter_map(|id| last_by_prompt.remove(&id))
        .collect()
}

fn millis_to_rfc3339(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}
