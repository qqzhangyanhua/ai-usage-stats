//! 从本机 Cursor 客户端读登录态，免去手动粘贴 cookie。
//!
//! Cursor 是 VSCode fork，把 WorkOS 会话 JWT 明文写在 globalStorage 的
//! `state.vscdb` 里（`ItemTable` 的 `cursorAuth/accessToken`），续期时自己写回。
//! 所以每次刷新重读一次就能拿到长期有效的凭证，用户不用管。
//!
//! cursor.com 的接口要的是 cookie 值 `<userId>%3A%3A<jwt>`，userId 是 JWT
//! `sub`（形如 `google-oauth|user_01J…`）里 `|` 之后那段。
//!
//! 只读，不写 Cursor 的任何文件；读不到就静默返回 None。

use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::vscode_state;

const ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";
const EMAIL_KEY: &str = "cursorAuth/cachedEmail";
const MEMBERSHIP_KEY: &str = "cursorAuth/stripeMembershipType";
/// 快到期就当过期，别让一次刷新卡在半路。
const EXPIRY_SKEW_MS: i64 = 60_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalCredential {
    /// 拼好的 `WorkosCursorSessionToken` cookie 值。
    pub session_token: String,
    pub email: Option<String>,
    pub membership: Option<String>,
    /// JWT `exp`，毫秒；缺字段则为 None（不当成过期）。
    pub expires_at_ms: Option<i64>,
}

impl LocalCredential {
    pub fn is_expired_at(&self, now_ms: i64) -> bool {
        match self.expires_at_ms {
            Some(exp) => exp <= now_ms + EXPIRY_SKEW_MS,
            None => false,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.is_expired_at(chrono::Utc::now().timestamp_millis())
    }

    pub fn expires_at_rfc3339(&self) -> Option<String> {
        self.expires_at_ms
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|dt| dt.to_rfc3339())
    }
}

pub fn global_storage_dir() -> Option<PathBuf> {
    vscode_state::global_storage_dir("Cursor")
}

/// 读本机登录态。任何一步失败都返回 None：这是「有就用」的加分项，不是必需路径。
pub fn read_local_credential() -> Option<LocalCredential> {
    read_credential_at(&global_storage_dir()?).ok().flatten()
}

pub fn read_credential_at(global_storage: &Path) -> Result<Option<LocalCredential>, String> {
    let Some(conn) = vscode_state::open_read_only(global_storage)? else {
        return Ok(None);
    };
    let Some(access_token) = vscode_state::read_item(&conn, ACCESS_TOKEN_KEY) else {
        return Ok(None);
    };
    if access_token.is_empty() {
        return Ok(None);
    }
    Ok(Some(LocalCredential {
        session_token: build_session_token(&access_token)?,
        email: vscode_state::read_item(&conn, EMAIL_KEY).filter(|value| !value.is_empty()),
        membership: vscode_state::read_item(&conn, MEMBERSHIP_KEY)
            .filter(|value| !value.is_empty()),
        expires_at_ms: expires_at_ms(&access_token),
    }))
}

/// `<userId>%3A%3A<jwt>` —— cursor.com 认的就是这个形状。
pub fn build_session_token(access_token: &str) -> Result<String, String> {
    let claims = decode_claims(access_token)?;
    let sub = claims
        .get("sub")
        .and_then(Value::as_str)
        .ok_or_else(|| "本机 Cursor 登录态缺少 sub".to_string())?;
    let user_id = sub.rsplit('|').next().unwrap_or(sub).trim();
    if user_id.is_empty() {
        return Err("本机 Cursor 登录态的 sub 里没有用户 ID".to_string());
    }
    Ok(format!("{user_id}%3A%3A{access_token}"))
}

pub fn expires_at_ms(access_token: &str) -> Option<i64> {
    decode_claims(access_token)
        .ok()?
        .get("exp")
        .and_then(Value::as_i64)
        .map(|seconds| seconds * 1000)
}

fn decode_claims(jwt: &str) -> Result<Value, String> {
    let payload = jwt
        .split('.')
        .nth(1)
        .filter(|part| !part.is_empty())
        .ok_or_else(|| "本机 Cursor 登录态不是合法 JWT".to_string())?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .map_err(|error| format!("解析本机 Cursor 登录态失败：{error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("解析本机 Cursor 登录态失败：{error}"))
}
