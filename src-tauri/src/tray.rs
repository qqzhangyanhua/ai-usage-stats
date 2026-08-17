//! 菜单栏今日花费：本地时区当天合计，关闭主窗口后继续刷新。

use std::time::Duration;

use chrono::{DateTime, Local, SecondsFormat, Utc};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::domain::{Filter, OverviewDto};
use crate::{ingest, query, AppState};

const TRAY_ID: &str = "today-cost";
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

struct TrayItems {
    cost: MenuItem<Wry>,
    tokens: MenuItem<Wry>,
    note: MenuItem<Wry>,
}

pub fn local_day_filter(now: DateTime<Local>) -> Filter {
    let date = now.date_naive();
    let start = date
        .and_hms_milli_opt(0, 0, 0, 0)
        .expect("local midnight is valid");
    let end = date
        .and_hms_milli_opt(23, 59, 59, 999)
        .expect("local end of day is valid");
    Filter {
        from: Some(to_utc_z(local_or_now(start, now))),
        to: Some(to_utc_z(local_or_now(end, now))),
        sources: Vec::new(),
        models: Vec::new(),
        projects: Vec::new(),
        providers: Vec::new(),
    }
}

fn local_or_now(naive: chrono::NaiveDateTime, now: DateTime<Local>) -> DateTime<Local> {
    naive
        .and_local_timezone(Local)
        .earliest()
        .or_else(|| naive.and_local_timezone(Local).latest())
        .unwrap_or(now)
}

fn to_utc_z(dt: DateTime<Local>) -> String {
    dt.with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn format_title(cost: Option<f64>, unpriced: bool) -> String {
    match (cost, unpriced) {
        (None, true) => "—".to_string(),
        (None, false) => "$0.00".to_string(),
        (Some(amount), true) => format!("${amount:.2}*"),
        (Some(amount), false) => format!("${amount:.2}"),
    }
}

pub fn format_compact(n: i64) -> String {
    let abs = n.unsigned_abs();
    if abs >= 1_000_000_000 {
        format!("{}B", trim_num(n as f64 / 1_000_000_000.0))
    } else if abs >= 1_000_000 {
        format!("{}M", trim_num(n as f64 / 1_000_000.0))
    } else if abs >= 10_000 {
        format!("{}K", trim_num(n as f64 / 1_000.0))
    } else {
        n.to_string()
    }
}

fn trim_num(n: f64) -> String {
    let text = format!("{n:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn setup(app: &AppHandle) -> Result<(), String> {
    let cost = MenuItem::with_id(app, "today_cost", "今日 $0.00", false, None::<&str>)
        .map_err(|e| e.to_string())?;
    let tokens = MenuItem::with_id(app, "today_tokens", "0 tokens", false, None::<&str>)
        .map_err(|e| e.to_string())?;
    let note = MenuItem::with_id(app, "today_note", "已按单价核算", false, None::<&str>)
        .map_err(|e| e.to_string())?;
    let show = MenuItem::with_id(app, "show", "打开主窗口", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let refresh_item =
        MenuItem::with_id(app, "refresh", "刷新", true, None::<&str>).map_err(|e| e.to_string())?;
    let quit =
        MenuItem::with_id(app, "quit", "退出", true, None::<&str>).map_err(|e| e.to_string())?;
    let sep1 = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let sep2 = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let menu = Menu::with_items(
        app,
        &[
            &cost,
            &tokens,
            &note,
            &sep1,
            &show,
            &refresh_item,
            &sep2,
            &quit,
        ],
    )
    .map_err(|e| e.to_string())?;

    app.manage(TrayItems { cost, tokens, note });

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("今日花费")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "refresh" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = tauri::async_runtime::spawn_blocking(move || refresh_with_ingest(&app))
                        .await;
                });
            }
            "quit" => app.exit(0),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    #[cfg(target_os = "macos")]
    {
        builder = builder.icon_as_template(true);
    }

    builder.build(app).map_err(|e| e.to_string())?;

    if let Ok(overview) = query_today(app) {
        apply_labels_now(app, &overview);
    }

    let handle = app.clone();
    std::thread::spawn(move || {
        let _ = refresh_with_ingest(&handle);
        loop {
            std::thread::sleep(REFRESH_INTERVAL);
            let _ = refresh_with_ingest(&handle);
        }
    });

    Ok(())
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn refresh(app: &AppHandle) -> Result<(), String> {
    let overview = query_today(app)?;
    apply_labels(app, &overview)
}

pub fn refresh_with_ingest(app: &AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let conn = state.conn.lock().map_err(|e| e.to_string())?;
        ingest::ingest_all(&conn, &ingest::default_home())?;
    }
    refresh(app)
}

fn query_today(app: &AppHandle) -> Result<OverviewDto, String> {
    let state = app.state::<AppState>();
    let prices = state.effective_prices();
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    query::overview(&conn, &local_day_filter(Local::now()), &prices)
}

fn apply_labels(app: &AppHandle, overview: &OverviewDto) -> Result<(), String> {
    let app = app.clone();
    let overview = overview.clone();
    app.clone()
        .run_on_main_thread(move || {
            apply_labels_now(&app, &overview);
        })
        .map_err(|e| e.to_string())
}

fn apply_labels_now(app: &AppHandle, overview: &OverviewDto) {
    let title = format_title(overview.cost, overview.unpriced);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(Some(title.as_str()));
        let _ = tray.set_tooltip(Some(format!("今日花费 {title}")));
    }
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.cost.set_text(format!("今日 {title}"));
        let _ = items
            .tokens
            .set_text(format!("{} tokens", format_compact(overview.total_tokens)));
        let _ = items.note.set_text(if overview.unpriced {
            "部分模型单价未配置"
        } else {
            "已按单价核算"
        });
    }
}
