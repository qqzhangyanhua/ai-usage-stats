use serde_json::Value;

use crate::adapters::{finish, i64_field, text_field};
use crate::domain::{Source, UsageRecord};

pub fn parse_gemini_session(content: &str, source_file: &str) -> Vec<UsageRecord> {
    let value: Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let session_id = text_field(&value, &["sessionId"]);
    let project = project_from_path(source_file);
    let messages = value
        .get("messages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    messages
        .into_iter()
        .filter(|msg| {
            matches!(
                msg.get("type").and_then(|v| v.as_str()),
                Some("gemini") | Some("assistant")
            )
        })
        .filter_map(|msg| {
            let tokens = msg.get("tokens")?.clone();
            Some(finish(UsageRecord {
                occurred_at: text_field(&msg, &["timestamp"]),
                source: Source::Gemini,
                model: text_field(&msg, &["model"]),
                provider: String::new(),
                project: project.clone(),
                session_id: session_id.clone(),
                source_file: source_file.to_string(),
                input_tokens: i64_field(&tokens, &["input"]),
                output_tokens: i64_field(&tokens, &["output"]),
                cache_read_tokens: i64_field(&tokens, &["cached"]),
                cache_creation_tokens: 0,
                reasoning_tokens: i64_field(&tokens, &["thoughts"]),
                total_tokens: i64_field(&tokens, &["total"]),
                native_cost: None,
            }))
        })
        .collect()
}

fn project_from_path(source_file: &str) -> String {
    std::path::Path::new(source_file)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}
