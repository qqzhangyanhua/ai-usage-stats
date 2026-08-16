use std::path::Path;

pub fn decode_dashed_dir(name: &str) -> String {
    let trimmed = name.trim_matches('-');
    if trimmed.is_empty() {
        return name.to_string();
    }
    if name.starts_with('/') {
        return name.to_string();
    }
    format!("/{}", trimmed.replace('-', "/"))
}

pub fn decode_url_dir(name: &str) -> String {
    urlencoding::decode(name)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| name.to_string())
}

pub fn project_from_source_file(source_file: &str) -> String {
    let path = Path::new(source_file);
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");
    if parent.contains('%') {
        return decode_url_dir(parent);
    }
    if parent.starts_with('-') {
        return decode_dashed_dir(parent);
    }
    parent.to_string()
}

pub fn session_id_from_source_file(source_file: &str) -> String {
    Path::new(source_file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("")
        .to_string()
}
