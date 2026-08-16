use crate::adapters::project::decode_dashed_dir;
use crate::adapters::{finish, i64_field, parse_jsonl_values, text_field};
use crate::domain::{Source, UsageRecord};

pub fn parse_claude_jsonl(content: &str, source_file: &str) -> Vec<UsageRecord> {
    let values = parse_jsonl_values(content);
    let mut project = String::new();
    let mut session_id = String::new();
    for value in &values {
        if project.is_empty() {
            project = text_field(value, &["cwd"]);
        }
        if session_id.is_empty() {
            session_id = text_field(value, &["sessionId", "session_id"]);
        }
    }
    if project.is_empty() {
        project = project_from_path(source_file);
    }
    if session_id.is_empty() {
        session_id = crate::adapters::project::session_id_from_source_file(source_file);
    }
    let mut records = Vec::new();

    for value in values {
        if value.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let message = value.get("message").cloned().unwrap_or_default();
        let usage = message.get("usage").cloned().unwrap_or_default();
        if usage.is_null() {
            continue;
        }
        records.push(finish(UsageRecord {
            occurred_at: text_field(&value, &["timestamp"]),
            source: Source::Claude,
            model: text_field(&message, &["model"]),
            provider: String::new(),
            project: project.clone(),
            session_id: session_id.clone(),
            source_file: source_file.to_string(),
            input_tokens: i64_field(&usage, &["input_tokens"]),
            output_tokens: i64_field(&usage, &["output_tokens"]),
            cache_read_tokens: i64_field(&usage, &["cache_read_input_tokens"]),
            cache_creation_tokens: i64_field(&usage, &["cache_creation_input_tokens"]),
            reasoning_tokens: 0,
            total_tokens: 0,
            native_cost: None,
        }));
    }

    records
}

fn project_from_path(source_file: &str) -> String {
    std::path::Path::new(source_file)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(decode_dashed_dir)
        .unwrap_or_default()
}
