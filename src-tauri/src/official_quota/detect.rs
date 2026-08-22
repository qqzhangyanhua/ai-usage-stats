//! 本机是否有某家的登录态——只看本地文件，不联网、不写任何东西。
//!
//! 用途有两个：没凭证的 provider 不去打网（省一次必然失败的请求），
//! 界面上也不给它留一行永远好不了的红字。
//!
//! 判定要「宁可显示、不可误藏」：探针只在能确定读不到凭证时返回 false，
//! 拿不准就返回 true，让真正的刷新去报准确的错。

use crate::domain::OfficialQuotaProvider;
use crate::official_quota::{
    antigravity, capture_path, claude_usage, codex_usage, copilot, devin, droid, grok, opencode,
};

pub fn has_local_credentials(provider: OfficialQuotaProvider) -> bool {
    match provider {
        // 官方登录态可用，或者装过 statusline hook 留下了捕获文件。
        OfficialQuotaProvider::Claude => {
            claude_usage::load_access_token(&claude_usage::credentials_path()).is_ok()
                || capture_path().exists()
        }
        // 纯 API key 的账号按量计费，没有额度可言，app-server 也给不出来。
        OfficialQuotaProvider::Codex => codex_usage::load_auth(&codex_usage::auth_path()).is_ok(),
        OfficialQuotaProvider::Cursor => crate::cursor_credentials::read_local_credential()
            .is_some_and(|credential| !credential.is_expired()),
        OfficialQuotaProvider::Grok => grok::auth_file_exists(),
        OfficialQuotaProvider::Droid => droid::load_access_token().is_ok(),
        OfficialQuotaProvider::Antigravity => antigravity::has_local_tokens(),
        OfficialQuotaProvider::OpenCode => {
            // 文件读坏了不算「没有」——让刷新去报错，别把故障当成没登录。
            opencode::load_api_key(&opencode::auth_path()).map_or(true, |key| key.is_some())
        }
        OfficialQuotaProvider::Copilot => copilot::credential_paths()
            .into_iter()
            .any(|path| path.exists()),
        OfficialQuotaProvider::Devin => devin::has_local_api_key(),
    }
}
