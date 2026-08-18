#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().nth(1).as_deref() == Some("statusline") {
        if let Err(error) = ai_usage_stats_lib::official_quota::claude::run_statusline() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    ai_usage_stats_lib::run()
}
