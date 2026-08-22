//! Antigravity 官方额度：读本机 Antigravity 客户端的 Google 登录态，
//! 打 `POST /v1internal:retrieveUserQuotaSummary`。
//!
//! 和 Cursor 一样，凭证在 VSCode 风格的 `state.vscdb` 里，但存的是 Google OAuth
//! access token（`ya29.`，只活约 1 小时）。所以先拿现成的打，401 了再用同一条记录里的
//! refresh token 现刷。refresh token 埋在 `antigravityUnifiedStateSync.oauthToken`
//! 的嵌套 protobuf 里（外层 base64 → protobuf → 内层 base64 → protobuf）。
//!
//! 刷新要用 Antigravity 自己的 OAuth 客户端。**我们不内嵌它的 client secret**——
//! 那是 Google 发给 Antigravity 的凭证，不该进本仓库，GitHub 的 secret scanning 也会拦。
//! 改成运行时从本机安装的 `out/main.js` 里提取，顺带在 Google 轮换密钥时自动跟上。
//!
//! cloudcode-pa 按 **User-Agent** 判定 Code Assist 权限：UA 里不带 `Antigravity/`
//! 标记就一律 403「no valid license」。实测版本号不影响，只认这个标记。

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use chrono::Utc;
use serde_json::Value;

use crate::domain::OfficialQuotaWindow;
use crate::official_quota::{parse_resets_at, sanitize_percent};
use crate::vscode_state;

const APP_DIR: &str = "Antigravity";
const AUTH_STATUS_KEY: &str = "antigravityAuthStatus";
const OAUTH_TOKEN_KEY: &str = "antigravityUnifiedStateSync.oauthToken";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// 必须带 `Antigravity/` 标记，否则 cloudcode-pa 直接 403。
const USER_AGENT: &str = "vscode/1.X.X (Antigravity/4.3.0)";
/// prod 已验证可用；daily / sandbox 是 Antigravity 自己也会走的备用环境。
const SUMMARY_URLS: [&str; 3] = [
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal:retrieveUserQuotaSummary",
];
const TIMEOUT: Duration = Duration::from_secs(15);
const NOT_SIGNED_IN: &str = "尚未登录 Antigravity，请先打开 Antigravity 客户端并登录 Google 账号";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct LocalTokens {
    /// `ya29.` 开头，约 1 小时过期。
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

enum SummaryError {
    Unauthorized,
    Other(String),
}

pub fn fetch_rate_limits() -> Result<(Vec<OfficialQuotaWindow>, String), String> {
    let tokens = load_local_tokens()?;
    let raw = match tokens.access_token.as_deref().map(request_summary) {
        // 存的 token 还没过期，省一次刷新。
        Some(Ok(raw)) => raw,
        Some(Err(SummaryError::Other(error))) => return Err(error),
        // 没有 token，或者已经过期：现刷一个。
        _ => {
            let refresh_token = tokens
                .refresh_token
                .ok_or_else(|| NOT_SIGNED_IN.to_string())?;
            let access_token = refresh_access_token(&refresh_token)?;
            request_summary(&access_token).map_err(|error| match error {
                SummaryError::Unauthorized => {
                    "Antigravity 登录已失效，请重新打开客户端登录".to_string()
                }
                SummaryError::Other(message) => message,
            })?
        }
    };
    Ok((parse_quota_summary(&raw)?, Utc::now().to_rfc3339()))
}

/// `groups[].buckets[]` → 每个桶一个窗口。`remainingFraction` 是「剩余」，取反才是已用。
pub fn parse_quota_summary(raw: &str) -> Result<Vec<OfficialQuotaWindow>, String> {
    let value: Value =
        serde_json::from_str(raw).map_err(|e| format!("Antigravity 限额 JSON 解析失败：{e}"))?;
    let groups = value
        .get("groups")
        .and_then(Value::as_array)
        .ok_or_else(|| "Antigravity 限额响应里没有 groups".to_string())?;

    let mut windows = Vec::new();
    for group in groups {
        let group_label = group
            .get("displayName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        for bucket in group
            .get("buckets")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
        {
            let Some(percent) = bucket
                .get("remainingFraction")
                .and_then(Value::as_f64)
                .map(|remaining| (1.0 - remaining) * 100.0)
                .and_then(sanitize_percent)
            else {
                continue;
            };
            let Some(kind) = bucket
                .get("bucketId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
            else {
                continue;
            };
            windows.push(OfficialQuotaWindow {
                kind: kind.replace('-', "_"),
                label: bucket_label(group_label, bucket),
                used_percent: Some(percent),
                resets_at: bucket.get("resetTime").and_then(parse_resets_at),
            });
        }
    }

    if windows.is_empty() {
        return Err("Antigravity 限额响应里没有可用的额度桶".to_string());
    }
    Ok(windows)
}

/// 官方给的是「Weekly Limit Remaining」这种剩余口径的名字，我们展示的是已用，
/// 直接沿用会读反，所以按窗口自己起名，group 名做前缀区分模型池。
fn bucket_label(group_label: &str, bucket: &Value) -> String {
    let window = match bucket.get("window").and_then(Value::as_str) {
        Some("weekly") => "周",
        Some("5h") => "5 小时",
        Some(other) if !other.is_empty() => return format!("{group_label} {other}").trim().into(),
        _ => "额度",
    };
    format!("{group_label} {window}").trim().to_string()
}

fn load_local_tokens() -> Result<LocalTokens, String> {
    let dir = vscode_state::global_storage_dir(APP_DIR)
        .ok_or_else(|| "无法定位 Antigravity 配置目录".to_string())?;
    let tokens = read_local_tokens_at(&dir)?;
    if tokens == LocalTokens::default() {
        return Err(NOT_SIGNED_IN.to_string());
    }
    Ok(tokens)
}

pub fn read_local_tokens_at(global_storage: &Path) -> Result<LocalTokens, String> {
    let Some(conn) = vscode_state::open_read_only(global_storage)? else {
        return Ok(LocalTokens::default());
    };
    let access_token = vscode_state::read_item(&conn, AUTH_STATUS_KEY)
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| {
            value
                .get("apiKey")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(str::to_string)
        });
    let refresh_token = vscode_state::read_item(&conn, OAUTH_TOKEN_KEY)
        .as_deref()
        .and_then(extract_refresh_token);
    Ok(LocalTokens {
        access_token,
        refresh_token,
    })
}

/// 外层 base64 → protobuf，里面某个字符串字段又是一层 base64 → protobuf，
/// refresh token 是内层的一个 `1//` 开头的字符串。字段号不稳定，所以按形状找。
pub fn extract_refresh_token(encoded: &str) -> Option<String> {
    let blob = decode_base64(encoded.trim())?;
    proto_strings(&blob, 0)
        .into_iter()
        .filter(|value| value.len() > 40)
        .filter_map(|value| decode_base64(&value))
        .flat_map(|inner| proto_strings(&inner, 0))
        .find(|value| value.starts_with("1//"))
}

/// 内层那段 base64 是从 protobuf 里切出来的，padding 未必齐，按无 padding 解。
fn decode_base64(value: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(value.trim_end_matches('='))
        .ok()
}

/// 极简 protobuf 遍历：只收 wire type 2 里能当 UTF-8 打印的字段，其余递归下钻。
fn proto_strings(buf: &[u8], depth: u8) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < buf.len() {
        let Some((tag, next)) = read_varint(buf, i) else {
            break;
        };
        i = next;
        match tag & 7 {
            0 => match read_varint(buf, i) {
                Some((_, next)) => i = next,
                None => break,
            },
            1 => i += 8,
            5 => i += 4,
            2 => {
                let Some((len, next)) = read_varint(buf, i) else {
                    break;
                };
                let len = len as usize;
                if next + len > buf.len() {
                    break;
                }
                let value = &buf[next..next + len];
                i = next + len;
                match std::str::from_utf8(value) {
                    Ok(text) if text.chars().all(|c| c.is_ascii_graphic() || c == ' ') => {
                        out.push(text.to_string());
                    }
                    _ if depth < 5 => out.extend(proto_strings(value, depth + 1)),
                    _ => {}
                }
            }
            _ => break,
        }
    }
    out
}

fn read_varint(buf: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut result = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *buf.get(i)?;
        i += 1;
        result |= u64::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some((result, i));
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

/// 从本机安装的 Antigravity 里找 OAuth 客户端。`main.js` 里同时有多个 id 和多个
/// secret，配对关系看不出来，所以全组合都留着，由令牌接口来筛（错配返回
/// `invalid_client`，很快失败）。
pub fn parse_oauth_clients(source: &str) -> Vec<(String, String)> {
    let ids = scan(source, |window| {
        window.ends_with(".apps.googleusercontent.com")
    });
    let secrets = scan(source, |window| window.starts_with("GOCSPX-"));
    let mut pairs = Vec::new();
    for id in &ids {
        for secret in &secrets {
            pairs.push((id.clone(), secret.clone()));
        }
    }
    pairs
}

/// 不引正则依赖：按「凭证允许出现的字符」切分，再筛出形状对的片段。
fn scan(source: &str, keep: impl Fn(&str) -> bool) -> Vec<String> {
    let mut found: Vec<String> = source
        .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .filter(|token| token.len() >= 20 && keep(token))
        .map(str::to_string)
        .collect();
    found.sort();
    found.dedup();
    found
}

fn local_oauth_clients() -> Result<Vec<(String, String)>, String> {
    let path = antigravity_main_js().ok_or_else(|| {
        "找不到本机 Antigravity 安装目录，无法取得刷新登录所需的客户端信息".to_string()
    })?;
    let source = std::fs::read_to_string(&path)
        .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
    let clients = parse_oauth_clients(&source);
    if clients.is_empty() {
        return Err("本机 Antigravity 里没找到 OAuth 客户端信息".to_string());
    }
    Ok(clients)
}

/// 先顺 PATH 上的 `antigravity` 启动器反查安装根目录，再退回各平台默认安装位置。
fn antigravity_main_js() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(bin) = which_antigravity() {
        if let Some(parent) = bin.parent().and_then(Path::parent) {
            roots.push(parent.to_path_buf());
        }
    }
    if let Some(local) = dirs::data_local_dir() {
        roots.push(local.join("Programs").join("Antigravity"));
    }
    roots.push(PathBuf::from(
        "/Applications/Antigravity.app/Contents/Resources/app",
    ));
    roots.push(PathBuf::from("/usr/share/antigravity"));
    roots.push(PathBuf::from("/opt/Antigravity"));

    roots.into_iter().find_map(|root| {
        // Win / Linux 是 <root>/resources/app/out，macOS 的 bin 已经在 app 目录下。
        [
            root.join("resources")
                .join("app")
                .join("out")
                .join("main.js"),
            root.join("out").join("main.js"),
        ]
        .into_iter()
        .find(|path| path.is_file())
    })
}

fn which_antigravity() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        ["antigravity", "antigravity.cmd", "antigravity.exe"]
            .into_iter()
            .map(|name| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// 本机存的 access token 只活约 1 小时，过期了就用 refresh token 换一个。
fn refresh_access_token(refresh_token: &str) -> Result<String, String> {
    let clients = local_oauth_clients()?;
    let mut last = "Antigravity 登录已失效，请重新打开客户端登录".to_string();
    for (client_id, client_secret) in clients {
        let response = ureq::post(TOKEN_URL).timeout(TIMEOUT).send_form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ]);
        match response {
            Ok(ok) => {
                let body = ok
                    .into_string()
                    .map_err(|e| format!("读取 Antigravity 令牌响应失败：{e}"))?;
                if let Some(token) = serde_json::from_str::<Value>(&body).ok().and_then(|value| {
                    value
                        .get("access_token")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }) {
                    return Ok(token);
                }
                last = "Antigravity 令牌响应里没有 access_token".to_string();
            }
            // 配错客户端会是 400/401，换下一组再试。
            Err(ureq::Error::Status(400 | 401, response)) => {
                let _ = response.into_string();
            }
            Err(ureq::Error::Status(code, response)) => {
                let _ = response.into_string();
                last = format!("刷新 Antigravity 登录失败：HTTP {code}");
            }
            Err(_) => {
                return Err("无法连接 Google 令牌接口，请检查网络后重试".to_string());
            }
        }
    }
    Err(last)
}

fn request_summary(access_token: &str) -> Result<String, SummaryError> {
    let mut last = SummaryError::Other("无法连接 Antigravity 限额接口，请检查网络后重试".into());
    for url in SUMMARY_URLS {
        let result = ureq::post(url)
            .timeout(TIMEOUT)
            .set("Authorization", &format!("Bearer {access_token}"))
            .set("User-Agent", USER_AGENT)
            .set("Content-Type", "application/json")
            .send_string("{}");
        match result {
            Ok(response) => {
                return response.into_string().map_err(|e| {
                    SummaryError::Other(format!("读取 Antigravity 限额响应失败：{e}"))
                })
            }
            // 换环境也不会变，交给上层去刷新重试。
            Err(ureq::Error::Status(401 | 403, _)) => return Err(SummaryError::Unauthorized),
            Err(ureq::Error::Status(code, response)) => {
                let _ = response.into_string();
                last = SummaryError::Other(format!("拉取 Antigravity 限额失败：HTTP {code}"));
            }
            Err(_) => {}
        }
    }
    Err(last)
}
