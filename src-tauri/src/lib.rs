pub mod adapters;
pub mod aggregate;
pub mod cost;
pub mod domain;
pub mod ingest;
pub mod query;
pub mod store;

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::Connection;
use serde::Deserialize;
use tauri::Manager;

use crate::domain::{
    ApplicationAnalyticsDto, CodeVolumeSummary, Filter, FilterOptions, IngestReport, NamedAmount,
    OverviewDto, PriceTable, SeriesPoint, SessionPage, SessionQuery, SessionRow, Source,
    SourceDiagnostic, TurnRow,
};

pub struct AppState {
    pub db_path: PathBuf,
    pub prices_path: PathBuf,
    pub conn: Mutex<Connection>,
}

fn cache_dir() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("ai-usage-stats");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn load_prices(path: &PathBuf) -> PriceTable {
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
        let prices = load_prices(&state.prices_path);
        query::overview(&conn, &filter, &prices)
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
        let prices = load_prices(&state.prices_path);
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
        let prices = load_prices(&state.prices_path);
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
        let prices = load_prices(&state.prices_path);
        query::top_sessions(&conn, &filter, &prices, limit.unwrap_or(20))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_sessions_page(app: tauri::AppHandle, query: SessionQuery) -> Result<SessionPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        let prices = load_prices(&state.prices_path);
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
        let prices = load_prices(&state.prices_path);
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
                let bytes = BASE64.decode(base64.as_bytes()).map_err(|e| e.to_string())?;
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
            let conn = store::open_db(db_path.to_string_lossy().as_ref())
                .map_err(std::io::Error::other)?;
            app.manage(AppState {
                db_path,
                prices_path,
                conn: Mutex::new(conn),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            ingest,
            get_overview,
            get_trend,
            get_application_analytics,
            get_breakdown,
            get_top_sessions,
            get_sessions_page,
            get_session_turns,
            get_filter_options,
            get_prices,
            save_price_table,
            get_source_diagnostics,
            rebuild_cache,
            get_code_volume,
            export_csv,
            export_json,
            export_image
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests;
