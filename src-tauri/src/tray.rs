//! 菜单栏今日花费：本地时区当天合计，关闭主窗口后继续刷新。

use std::time::Duration;

use chrono::{DateTime, Local, SecondsFormat, Utc};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::domain::{Filter, OfficialQuotaDto, OverviewDto};
use crate::{ingest, official_quota, query, AppState};

const TRAY_ID: &str = "today-cost";
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);

struct TrayItems {
    cost: MenuItem<Wry>,
    tokens: MenuItem<Wry>,
    note: MenuItem<Wry>,
    show: MenuItem<Wry>,
    refresh: MenuItem<Wry>,
    quit: MenuItem<Wry>,
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
    format_title_with_quota(cost, unpriced, None)
}

pub fn format_title_with_quota(
    cost: Option<f64>,
    unpriced: bool,
    quota: Option<&official_quota::TightestQuota>,
) -> String {
    let cost = match (cost, unpriced) {
        (None, true) => "—".to_string(),
        (None, false) => "$0.00".to_string(),
        (Some(amount), true) => format!("${amount:.2}*"),
        (Some(amount), false) => format!("${amount:.2}"),
    };
    match quota {
        Some(item) => {
            let mark = if item.stale { "*" } else { "" };
            format!(
                "{cost} · {} {} {:.0}%{mark}",
                item.provider, item.label, item.used_percent
            )
        }
        None => cost,
    }
}

/// 托盘菜单里每家一行。标题只放得下最紧的那一个窗口，但从菜单栏一眼看全各家
/// 才是这个功能的意义，所以这里按 provider 汇总。
///
/// 每行最多列 3 个窗口——Droid 一家就有 4~6 个，全列会把菜单撑得很长；
/// 多出来的用 `+N` 带过，细节去主窗口看。
pub fn quota_menu_lines(quota: &OfficialQuotaDto) -> Vec<String> {
    const MAX_WINDOWS: usize = 3;
    quota
        .rows
        .iter()
        .map(|row| {
            let shown: Vec<String> = row
                .windows
                .iter()
                .filter_map(|window| {
                    let percent = window.used_percent?;
                    Some(format!("{} {percent:.0}%", window.label))
                })
                .collect();
            if shown.is_empty() {
                // 没数字的行放一句原因，比凭空消失强。
                let reason = row.error.as_deref().unwrap_or("暂无数据");
                return format!("{}：{reason}", row.application);
            }
            let extra = shown.len().saturating_sub(MAX_WINDOWS);
            let mut text = shown
                .iter()
                .take(MAX_WINDOWS)
                .cloned()
                .collect::<Vec<_>>()
                .join(" · ");
            if extra > 0 {
                text.push_str(&format!(" +{extra}"));
            }
            let stale = matches!(row.freshness, crate::domain::OfficialQuotaFreshness::Stale);
            format!(
                "{}  {text}{}",
                row.application,
                if stale { "*" } else { "" }
            )
        })
        .collect()
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

    app.manage(TrayItems {
        cost,
        tokens,
        note,
        show,
        refresh: refresh_item,
        quit,
    });

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
        let quota = load_quota_dto(app).ok();
        apply_labels_now(app, &overview, quota.as_ref());
    }

    let handle = app.clone();
    std::thread::spawn(move || {
        // 启动只刷菜单栏缓存，全量摄取交给主窗口。两边一起扫盘会把首屏查询拖死。
        let _ = refresh(&handle);
        loop {
            std::thread::sleep(REFRESH_INTERVAL);
            let _ = refresh_if_stale(&handle);
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

/// 源文件元数据没变时只重算今日菜单栏，避免关闭主窗口后每 5 分钟全量扫盘。
pub fn refresh_if_stale(app: &AppHandle) -> Result<(), String> {
    let cache = {
        let state = app.state::<AppState>();
        let conn = state.lock_read()?;
        ingest::load_scan_cache(&conn)?
    };
    if ingest::scan_is_stale_from_cache(&cache, &ingest::default_home())? {
        refresh_with_ingest(app)
    } else {
        let _ = sync_official_quota(app);
        refresh(app)
    }
}

pub fn refresh_with_ingest(app: &AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let conn = state.lock_write()?;
        ingest::ingest_all(&conn, &ingest::default_home())?;
        let prices = state.effective_prices();
        let _ = crate::budget::check_and_notify(
            app,
            &conn,
            &prices,
            &state.budget_path,
            &state.budget_notify_path,
        );
    }
    let _ = sync_official_quota(app);
    refresh(app)
}

fn sync_official_quota(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let conn = state.lock_write()?;
    let _ = official_quota::sync_claude_capture(&conn);
    let config = official_quota::load_config(&state.official_quota_path);
    let dto = official_quota::load_dto(&conn, &config, chrono::Utc::now());
    official_quota::notify::check_and_notify_with_config(
        app,
        &dto,
        &config,
        &state.official_quota_notify_path,
    )
}

fn query_today(app: &AppHandle) -> Result<OverviewDto, String> {
    let state = app.state::<AppState>();
    let prices = state.effective_prices();
    let conn = state.lock_read()?;
    query::overview(&conn, &local_day_filter(Local::now()), &prices)
}

fn apply_labels(app: &AppHandle, overview: &OverviewDto) -> Result<(), String> {
    let quota = load_quota_dto(app).ok();
    let app = app.clone();
    let overview = overview.clone();
    app.clone()
        .run_on_main_thread(move || {
            apply_labels_now(&app, &overview, quota.as_ref());
        })
        .map_err(|e| e.to_string())
}

fn load_quota_dto(app: &AppHandle) -> Result<OfficialQuotaDto, String> {
    let state = app.state::<AppState>();
    let conn = state.lock_read()?;
    let config = official_quota::load_config(&state.official_quota_path);
    Ok(official_quota::load_dto(&conn, &config, chrono::Utc::now()))
}

/// 额度区每次重建：哪些 provider 可见由本地凭证检测决定，数量会变，
/// 固定槽位撑不住。菜单项全是禁用的纯展示行。
fn rebuild_quota_menu(app: &AppHandle, quota: &OfficialQuotaDto) -> Result<(), String> {
    let lines = quota_menu_lines(quota);
    let items = app
        .try_state::<TrayItems>()
        .ok_or_else(|| "托盘菜单尚未初始化".to_string())?;
    let mut entries: Vec<Box<dyn tauri::menu::IsMenuItem<Wry>>> = Vec::new();
    entries.push(Box::new(items.cost.clone()));
    entries.push(Box::new(items.tokens.clone()));
    entries.push(Box::new(items.note.clone()));
    for (index, line) in lines.iter().enumerate() {
        entries.push(Box::new(
            PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?,
        ));
        entries.push(Box::new(
            MenuItem::with_id(app, format!("quota_{index}"), line, false, None::<&str>)
                .map_err(|e| e.to_string())?,
        ));
    }
    entries.push(Box::new(
        PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?,
    ));
    entries.push(Box::new(items.show.clone()));
    entries.push(Box::new(items.refresh.clone()));
    entries.push(Box::new(
        PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?,
    ));
    entries.push(Box::new(items.quit.clone()));

    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> =
        entries.iter().map(|item| item.as_ref()).collect();
    let menu = Menu::with_items(app, &refs).map_err(|e| e.to_string())?;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn apply_labels_now(app: &AppHandle, overview: &OverviewDto, quota: Option<&OfficialQuotaDto>) {
    let tightest = quota.and_then(official_quota::tightest_window);
    let title = format_title_with_quota(overview.cost, overview.unpriced, tightest.as_ref());
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
    // 重建失败不该拖垮标题更新——标题已经写上去了。
    if let Some(quota) = quota {
        let _ = rebuild_quota_menu(app, quota);
    }
}
