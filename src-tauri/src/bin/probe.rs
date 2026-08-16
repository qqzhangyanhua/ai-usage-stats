//! Probe local session stores for the still-unconfirmed sources.
//!
//! Prints token-field locations only. Does not print session body text.
//!
//!   cargo run --bin probe --manifest-path src-tauri/Cargo.toml

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn main() {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    println!("# Token field probe");
    println!("home={}", home.display());
    println!();
    probe_dsh(&home.join(".dsh/sessions"));
    probe_gemini(&home.join(".gemini/tmp"));
    probe_grok(&home.join(".grok/sessions"));
    probe_qwen(&home.join(".qwen/tmp"));
    probe_factory(&home.join(".factory/sessions"));
}

fn probe_dsh(root: &Path) {
    println!("## dsh");
    println!("path={}", root.display());
    if !root.exists() {
        println!("present=false");
        println!();
        return;
    }
    let Some(file) = smallest_with_suffix(root, "session.jsonl.zstd") else {
        println!("has_token=false");
        println!("note=no session.jsonl.zstd found");
        println!();
        return;
    };
    println!("sample={}", file.display());
    match fs::read(&file).and_then(|bytes| {
        zstd::decode_all(bytes.as_slice()).map_err(|e| std::io::Error::other(e))
    }) {
        Ok(decoded) => {
            let text = String::from_utf8_lossy(&decoded);
            let mut types = Vec::new();
            let mut usage_on_message = false;
            let mut usage_on_chunk = false;
            let mut usage_keys = Vec::new();
            let mut model = String::new();
            let mut provider = String::new();
            let mut session_id = String::new();
            let mut project = String::new();
            for line in text.lines().take(400) {
                let Ok(value) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if !types.iter().any(|t| t == kind) {
                    types.push(kind.to_string());
                }
                if kind == "session" {
                    session_id = text_of(&value, &["id"]);
                    project = text_of(&value, &["cwd"]);
                }
                if kind == "request/header" {
                    if let Some(config) = value.pointer("/data/header/config") {
                        provider = text_of(config, &["provider"]);
                        model = text_of(config, &["model"]);
                    }
                }
                if kind == "assistant/chunk" {
                    if let Some(usage) = value.pointer("/data/chunk/usage") {
                        usage_on_chunk = true;
                        usage_keys = object_keys(usage);
                    }
                }
                if kind == "assistant/message" {
                    if let Some(usage) = value.pointer("/data/usage") {
                        usage_on_message = true;
                        usage_keys = object_keys(usage);
                    }
                    if let Some(source) = value.pointer("/data/message/source") {
                        if model.is_empty() {
                            model = text_of(source, &["model"]);
                        }
                        if provider.is_empty() {
                            provider = text_of(source, &["provider"]);
                        }
                    }
                }
            }
            println!("has_token={}", usage_on_message || usage_on_chunk);
            println!("zstd_ok=true");
            println!("record_types={}", types.join(","));
            println!("usage_on_assistant_message={}", usage_on_message);
            println!("usage_on_assistant_chunk={}", usage_on_chunk);
            println!("usage_keys={}", usage_keys.join(","));
            println!("map.input=inputTokens");
            println!("map.output=outputTokens");
            println!("map.cache_read=cacheReadTokens");
            println!("map.cache_creation=absent");
            println!("map.reasoning=reasoningTokens");
            println!("map.total=sum");
            println!("model={model}");
            println!("provider={provider}");
            println!("project={project}");
            println!("session_id={session_id}");
            println!("dedupe=use assistant/message only, ignore assistant/chunk");
        }
        Err(error) => println!("zstd_ok=false error={error}"),
    }
    println!();
}

fn probe_gemini(root: &Path) {
    println!("## gemini");
    println!("path={}", root.display());
    if !root.exists() {
        println!("present=false");
        println!();
        return;
    }
    let chats: Vec<PathBuf> = walk(root)
        .into_iter()
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("json")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("session-"))
        })
        .collect();
    println!("session_files={}", chats.len());
    let mut found = false;
    for file in chats.into_iter().take(40) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(messages) = value.get("messages").and_then(|v| v.as_array()) else {
            continue;
        };
        if let Some(msg) = messages.iter().find(|m| m.get("tokens").is_some()) {
            found = true;
            println!("sample={}", file.display());
            println!("has_token=true");
            println!("token_object_keys={}", object_keys(&msg["tokens"]).join(","));
            println!("map.input=tokens.input");
            println!("map.output=tokens.output");
            println!("map.cache_read=tokens.cached");
            println!("map.cache_creation=absent");
            println!("map.reasoning=tokens.thoughts");
            println!("map.total=tokens.total");
            println!("model={}", text_of(msg, &["model"]));
            println!("session_id={}", text_of(&value, &["sessionId"]));
            println!(
                "project={}",
                file.parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
            );
            break;
        }
    }
    if !found {
        println!("has_token=false");
        println!("note=logs.json has no token fields; tokens live on chats/session-*.json");
    }
    println!();
}

fn probe_grok(root: &Path) {
    println!("## grok");
    println!("path={}", root.display());
    if !root.exists() {
        println!("present=false");
        println!();
        return;
    }
    let updates: Vec<PathBuf> = walk(root)
        .into_iter()
        .filter(|p| p.file_name().and_then(|n| n.to_str()) == Some("updates.jsonl"))
        .collect();
    println!("updates_files={}", updates.len());
    let mut found = false;
    for file in updates.into_iter().take(20) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let mut last_total = None;
        let mut prompt_id = String::new();
        let mut model = String::new();
        for line in text.lines().take(80) {
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if model.is_empty() {
                model = text_of_pointer(&value, "/params/update/_meta/modelId");
            }
            if let Some(total) = value.pointer("/params/_meta/totalTokens").and_then(|v| {
                v.as_i64().or_else(|| v.as_u64().map(|n| n as i64))
            }) {
                last_total = Some(total);
                prompt_id = text_of_pointer(&value, "/params/_meta/promptId");
            }
        }
        if last_total.is_some() {
            found = true;
            println!("sample={}", file.display());
            println!("has_token=partial");
            println!("map.input=absent");
            println!("map.output=absent");
            println!("map.cache_read=absent");
            println!("map.cache_creation=absent");
            println!("map.reasoning=absent");
            println!("map.total=params._meta.totalTokens");
            println!("sample_total={}", last_total.unwrap_or(0));
            println!("sample_prompt_id={prompt_id}");
            println!("model={model}");
            println!(
                "project={}",
                file.parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .map(decode_url)
                    .unwrap_or_default()
            );
            println!(
                "session_id={}",
                file.parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
            );
            println!("dedupe=last totalTokens per promptId");
            break;
        }
    }
    if !found {
        println!("has_token=false");
    }
    println!();
}

fn probe_qwen(root: &Path) {
    println!("## qwen");
    println!("path={}", root.display());
    if !root.exists() {
        println!("present=false");
        println!();
        return;
    }
    let files = walk(root);
    println!("files={}", files.len());
    let mut any_token = false;
    let mut session_id = String::new();
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        if text.to_ascii_lowercase().contains("token") && text.contains("usage") {
            any_token = true;
        }
        if session_id.is_empty() {
            if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&text) {
                if let Some(first) = items.first() {
                    session_id = text_of(first, &["sessionId"]);
                }
            }
        }
    }
    println!("has_token={any_token}");
    println!("map.input=absent");
    println!("map.output=absent");
    println!("map.cache_read=absent");
    println!("map.cache_creation=absent");
    println!("map.reasoning=absent");
    println!("map.total=absent");
    println!("session_id={session_id}");
    println!("note=local tmp/*/logs.json is user text only");
    println!();
}

fn probe_factory(root: &Path) {
    println!("## factory");
    println!("path={}", root.display());
    if !root.exists() {
        println!("present=false");
        println!();
        return;
    }
    let settings: Vec<PathBuf> = walk(root)
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".settings.json"))
        })
        .collect();
    println!("settings_files={}", settings.len());
    let mut found = false;
    for file in settings {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if let Some(usage) = value.get("tokenUsage") {
            found = true;
            println!("sample={}", file.display());
            println!("has_token=true");
            println!("granularity=session_cumulative");
            println!("usage_keys={}", object_keys(usage).join(","));
            println!("map.input=tokenUsage.inputTokens");
            println!("map.output=tokenUsage.outputTokens");
            println!("map.cache_read=tokenUsage.cacheReadTokens");
            println!("map.cache_creation=tokenUsage.cacheCreationTokens");
            println!("map.reasoning=tokenUsage.thinkingTokens");
            println!("map.total=sum");
            println!("provider={}", text_of(&value, &["providerLock"]));
            println!(
                "session_id={}",
                file.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .trim_end_matches(".settings.json")
            );
            break;
        }
    }
    if !found {
        println!("has_token=false");
        println!("note=jsonl messages have no per-turn usage");
    }
    println!();
}

fn smallest_with_suffix(root: &Path, suffix: &str) -> Option<PathBuf> {
    walk(root)
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(suffix))
        })
        .min_by_key(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(u64::MAX))
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let child = entry.path();
            if child.is_dir() {
                stack.push(child);
            } else {
                out.push(child);
            }
        }
    }
    out
}

fn object_keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn text_of(value: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(s) = value.get(*key).and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }
    String::new()
}

fn text_of_pointer(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn decode_url(name: &str) -> String {
    urlencoding::decode(name)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| name.to_string())
}
