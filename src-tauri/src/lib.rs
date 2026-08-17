pub mod adapters;
pub mod aggregate;
pub mod billing_window;
pub mod cost;
pub mod cursor_account;
pub mod domain;
pub mod ingest;
pub mod litellm;
pub mod query;
pub mod store;
pub mod tray;

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::Connection;
use serde::Deserialize;
use tauri::Manager;

use crate::domain::{
    ApplicationAnalyticsDto, BillingWindowsDto, CodeVolumeSummary, CursorAccountUsageDto, Filter,
    FilterOptions, IngestReport, NamedAmount, OverviewDto, PriceSnapshot, PriceSnapshotMeta,
    PriceTable, SeriesPoint, SessionPage, SessionQuery, SessionRow, Source, SourceDiagnostic,
    TurnRow,
};

pub struct AppState {
    pub db_path: PathBuf,
    pub prices_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub conn: Mutex<Connection>,
    pub snapshot: Mutex<PriceSnapshot>,
}

impl AppState {
    /// 生效单价表 = 用户配置的单价 + LiteLLM 快照兜底（用户已配置的模型不被兜底覆盖）。
    /// 所有涉及费用的查询都应经由此方法取价，保证兜底语义在各处一致。
    pub(crate) fn effective_prices(&self) -> PriceTable {
        let user = load_prices(&self.prices_path);
        match self.snapshot.lock() {
            Ok(snapshot) => litellm::merge(&user, &snapshot),
            Err(_) => user,
        }
    }

    fn snapshot_meta(&self) -> PriceSnapshotMeta {
        let bundled = !self.snapshot_path.exists();
        match self.snapshot.lock() {
            Ok(snapshot) => PriceSnapshotMeta {
                as_of: snapshot.as_of.clone(),
                source: snapshot.source.clone(),
                count: snapshot.entries.len(),
                bundled,
            },
            Err(_) => PriceSnapshotMeta {
                as_of: String::new(),
                source: litellm::SOURCE_NAME.to_string(),
                count: 0,
                bundled,
            },
        }
    }
}

fn cache_dir() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("ai-usage-stats");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub(crate) fn load_prices(path: &PathBuf) -> PriceTable {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_prices(path: &PathBuf, prices: &PriceTable) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        path,
        serde_json::to_string_pretty(prices).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

#[tauri::command]
async fn ingest(app: tauri::AppHandle) -> Result<IngestReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        ingest::ingest_all(&conn, &ingest::default_home())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_overview(app: tauri::AppHandle, filter: Filter) -> Result<OverviewDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let prices = state.effective_prices();
        query::overview(&conn, &filter, &prices)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_billing_windows(
    app: tauri::AppHandle,
    filter: Filter,
) -> Result<BillingWindowsDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let prices = state.effective_prices();
        query::billing_windows(&conn, &filter, &prices, chrono::Utc::now())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_trend(
    app: tauri::AppHandle,
    filter: Filter,
    grain: String,
) -> Result<Vec<SeriesPoint>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let prices = state.effective_prices();
        query::trend(&conn, &filter, &prices, &grain)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_application_analytics(
    app: tauri::AppHandle,
    filter: Filter,
    grain: String,
) -> Result<ApplicationAnalyticsDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        query::application_analytics(&conn, &filter, &grain)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
struct NamedQuery {
    filter: Filter,
    dimension: String,
}

#[tauri::command]
async fn get_breakdown(
    app: tauri::AppHandle,
    query: NamedQuery,
) -> Result<Vec<NamedAmount>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let prices = state.effective_prices();
        query::breakdown(&conn, &query.filter, &prices, &query.dimension)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_top_sessions(
    app: tauri::AppHandle,
    filter: Filter,
    limit: Option<usize>,
) -> Result<Vec<SessionRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let prices = state.effective_prices();
        query::top_sessions(&conn, &filter, &prices, limit.unwrap_or(20))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_sessions_page(
    app: tauri::AppHandle,
    query: SessionQuery,
) -> Result<SessionPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let prices = state.effective_prices();
        query::sessions_page(&conn, &prices, &query)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_session_turns(
    app: tauri::AppHandle,
    session_id: String,
    source: Option<String>,
    filter: Filter,
) -> Result<Vec<TurnRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let prices = state.effective_prices();
        query::session_turns(&conn, &session_id, source.as_deref(), &filter, &prices)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_filter_options(app: tauri::AppHandle) -> Result<FilterOptions, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        query::filter_options(&conn)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_prices(state: tauri::State<AppState>) -> PriceTable {
    load_prices(&state.prices_path)
}

#[tauri::command]
fn save_price_table(state: tauri::State<AppState>, prices: PriceTable) -> Result<(), String> {
    save_prices(&state.prices_path, &prices)
}

/// 当前生效的 LiteLLM 价目快照元信息（内置或已刷新）。
#[tauri::command]
fn get_price_snapshot(state: tauri::State<AppState>) -> PriceSnapshotMeta {
    state.snapshot_meta()
}

/// 可选刷新：webview 拉取上游原始 JSON 后交给这里解析、落盘并热更新内存快照。
#[tauri::command]
async fn refresh_price_snapshot(
    app: tauri::AppHandle,
    raw: String,
) -> Result<PriceSnapshotMeta, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let as_of = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let snapshot = litellm::parse_litellm_raw(&raw, &as_of)?;
        if snapshot.entries.is_empty() {
            return Err("解析 LiteLLM 价目失败：未找到任何有效模型单价".to_string());
        }
        litellm::save_snapshot(&state.snapshot_path, &snapshot)?;
        let count = snapshot.entries.len();
        {
            let mut guard = state.snapshot.lock().map_err(|e| e.to_string())?;
            *guard = snapshot;
        }
        Ok(PriceSnapshotMeta {
            as_of,
            source: litellm::SOURCE_NAME.to_string(),
            count,
            bundled: false,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 恢复为内置快照：删除本地缓存并重载内置数据。
#[tauri::command]
async fn reset_price_snapshot(app: tauri::AppHandle) -> Result<PriceSnapshotMeta, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if state.snapshot_path.exists() {
            std::fs::remove_file(&state.snapshot_path).map_err(|e| e.to_string())?;
        }
        let bundled = litellm::bundled_snapshot();
        let meta = PriceSnapshotMeta {
            as_of: bundled.as_of.clone(),
            source: bundled.source.clone(),
            count: bundled.entries.len(),
            bundled: true,
        };
        {
            let mut guard = state.snapshot.lock().map_err(|e| e.to_string())?;
            *guard = bundled;
        }
        Ok(meta)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 供 webview 拉取上游价目使用的固定地址。
#[tauri::command]
fn get_price_snapshot_url() -> String {
    litellm::SOURCE_URL.to_string()
}

#[tauri::command]
async fn get_source_diagnostics(app: tauri::AppHandle) -> Result<Vec<SourceDiagnostic>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        ingest::source_diagnostics(&conn, &ingest::default_home())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn rebuild_cache(
    app: tauri::AppHandle,
    source: Option<String>,
) -> Result<IngestReport, String> {
    let source = source
        .as_deref()
        .map(|value| Source::parse(value).ok_or_else(|| format!("未知来源：{value}")))
        .transpose()?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        ingest::rebuild_cache(&conn, &ingest::default_home(), source)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_code_volume() -> Result<CodeVolumeSummary, String> {
    tauri::async_runtime::spawn_blocking(move || ingest::load_code_volume(&ingest::default_home()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn refresh_cursor_account_usage(
    app: tauri::AppHandle,
    token: Option<String>,
) -> Result<CursorAccountUsageDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let resolved = cursor_account::resolve_session_token(token)?;
        let state = app.state::<AppState>();
        let start_date_ms = {
            let conn = state.conn.lock().map_err(|e| e.to_string())?;
            cursor_account::incremental_start_ms(&conn)?
        };
        let pages = cursor_account::fetch_refresh_pages(&resolved, start_date_ms);
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        cursor_account::apply_fetched_pages(&conn, pages)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_cursor_account_usage(app: tauri::AppHandle) -> Result<CursorAccountUsageDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        cursor_account::load_summary(&conn)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_cursor_session_token(token: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cursor_account::save_token(&token))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn has_cursor_session_token() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(cursor_account::has_token)
        .await
        .map_err(|e| e.to_string())?
}

/// 弹出原生保存对话框并写入 CSV 内容；返回 `false` 表示用户取消。
#[tauri::command]
async fn export_csv(default_name: String, content: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("CSV", &["csv"])
            .save_file();
        match path {
            Some(path) => {
                // UTF-8 BOM 让 Excel 等工具正确识别中文，避免乱码。
                let mut bytes = vec![0xEF, 0xBB, 0xBF];
                bytes.extend_from_slice(content.as_bytes());
                fs::write(&path, bytes).map_err(|e| e.to_string())?;
                Ok(true)
            }
            None => Ok(false),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 弹出原生保存对话框并写入 JSON 内容；返回 `false` 表示用户取消。
#[tauri::command]
async fn export_json(default_name: String, content: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("JSON", &["json"])
            .save_file();
        match path {
            Some(path) => {
                fs::write(&path, content.as_bytes()).map_err(|e| e.to_string())?;
                Ok(true)
            }
            None => Ok(false),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn refresh_tray(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || tray::refresh(&app))
        .await
        .map_err(|e| e.to_string())?
}

/// 弹出原生保存对话框并写入图表 PNG（base64 编码）；返回 `false` 表示用户取消。
#[tauri::command]
async fn export_image(default_name: String, base64: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("PNG", &["png"])
            .save_file();
        match path {
            Some(path) => {
                let bytes = BASE64
                    .decode(base64.as_bytes())
                    .map_err(|e| e.to_string())?;
                fs::write(&path, bytes).map_err(|e| e.to_string())?;
                Ok(true)
            }
            None => Ok(false),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = cache_dir();
            let db_path = dir.join("usage.sqlite");
            let prices_path = dir.join("prices.json");
            let snapshot_path = dir.join("litellm_prices.json");
            let conn = store::open_db(db_path.to_string_lossy().as_ref())
                .map_err(std::io::Error::other)?;
            let (snapshot, _bundled) = litellm::load_snapshot(&snapshot_path);
            app.manage(AppState {
                db_path,
                prices_path,
                snapshot_path,
                conn: Mutex::new(conn),
                snapshot: Mutex::new(snapshot),
            });
            tray::setup(app.handle()).map_err(std::io::Error::other)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            ingest,
            get_overview,
            get_billing_windows,
            get_trend,
            get_application_analytics,
            get_breakdown,
            get_top_sessions,
            get_sessions_page,
            get_session_turns,
            get_filter_options,
            get_prices,
            save_price_table,
            get_price_snapshot,
            get_price_snapshot_url,
            refresh_price_snapshot,
            reset_price_snapshot,
            get_source_diagnostics,
            rebuild_cache,
            get_code_volume,
            refresh_cursor_account_usage,
            get_cursor_account_usage,
            save_cursor_session_token,
            has_cursor_session_token,
            export_csv,
            export_json,
            export_image,
            refresh_tray
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                tray::show_main(app);
            }
        });
}

#[cfg(test)]
mod tests;
