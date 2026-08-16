pub mod adapters;
pub mod aggregate;
pub mod cost;
pub mod domain;
pub mod ingest;
pub mod store;

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Deserialize;
use tauri::Manager;

use crate::domain::{
    CodeVolumeSummary, Filter, FilterOptions, IngestReport, NamedAmount, OverviewDto, PriceTable,
    SeriesPoint, SessionRow, TurnRow,
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
    fs::write(path, serde_json::to_string_pretty(prices).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

#[tauri::command]
fn ingest(state: tauri::State<AppState>) -> Result<IngestReport, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    ingest::ingest_all(&conn, &ingest::default_home())
}

#[tauri::command]
fn get_overview(state: tauri::State<AppState>, filter: Filter) -> Result<OverviewDto, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let records = store::load_all(&conn)?;
    let prices = load_prices(&state.prices_path);
    Ok(aggregate::overview(&records, &filter, &prices))
}

#[tauri::command]
fn get_trend(
    state: tauri::State<AppState>,
    filter: Filter,
    grain: String,
) -> Result<Vec<SeriesPoint>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let records = store::load_all(&conn)?;
    let prices = load_prices(&state.prices_path);
    Ok(aggregate::trend(&records, &filter, &prices, &grain))
}

#[derive(Deserialize)]
struct NamedQuery {
    filter: Filter,
    dimension: String,
}

#[tauri::command]
fn get_breakdown(state: tauri::State<AppState>, query: NamedQuery) -> Result<Vec<NamedAmount>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let records = store::load_all(&conn)?;
    let prices = load_prices(&state.prices_path);
    let rows = match query.dimension.as_str() {
        "model" => aggregate::by_name(&records, &query.filter, &prices, |r| r.model.clone()),
        "provider" => aggregate::by_name(&records, &query.filter, &prices, |r| r.provider.clone()),
        "project" => aggregate::by_name(&records, &query.filter, &prices, |r| r.project.clone()),
        _ => aggregate::by_name(&records, &query.filter, &prices, |r| {
            r.source.as_str().to_string()
        }),
    };
    Ok(rows)
}

#[tauri::command]
fn get_top_sessions(
    state: tauri::State<AppState>,
    filter: Filter,
    limit: Option<usize>,
) -> Result<Vec<SessionRow>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let records = store::load_all(&conn)?;
    let prices = load_prices(&state.prices_path);
    Ok(aggregate::top_sessions(
        &records,
        &filter,
        &prices,
        limit.unwrap_or(20),
    ))
}

#[tauri::command]
fn get_session_turns(
    state: tauri::State<AppState>,
    session_id: String,
    source: Option<String>,
    filter: Filter,
) -> Result<Vec<TurnRow>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let records = store::load_all(&conn)?;
    let prices = load_prices(&state.prices_path);
    Ok(aggregate::session_turns(
        &records,
        &session_id,
        source.as_deref(),
        &filter,
        &prices,
    ))
}

#[tauri::command]
fn get_filter_options(state: tauri::State<AppState>) -> Result<FilterOptions, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let records = store::load_all(&conn)?;
    Ok(aggregate::filter_options(&records))
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
fn get_code_volume() -> Result<CodeVolumeSummary, String> {
    ingest::load_code_volume(&ingest::default_home())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = cache_dir();
            let db_path = dir.join("usage.sqlite");
            let prices_path = dir.join("prices.json");
            let conn = store::open_db(db_path.to_string_lossy().as_ref())
                .map_err(|e| std::io::Error::other(e))?;
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
            get_breakdown,
            get_top_sessions,
            get_session_turns,
            get_filter_options,
            get_prices,
            save_price_table,
            get_code_volume
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests;
