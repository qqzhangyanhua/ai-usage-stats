use std::fs;
use std::path::PathBuf;

/// 本应用自己的数据目录。sqlite、价目、官方额度捕获和指令备份都落在这里。
pub fn app_data_dir() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("ai-usage-stats");
    let _ = fs::create_dir_all(&dir);
    dir
}
