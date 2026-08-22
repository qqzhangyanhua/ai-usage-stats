use std::fs;
use std::path::{Path, PathBuf};

const DIR_NAME: &str = "mabiao";
const LEGACY_DIR_NAME: &str = "ai-usage-stats";

/// 本应用自己的数据目录。sqlite、价目、官方额度捕获和指令备份都落在这里。
pub fn app_data_dir() -> PathBuf {
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")));
    app_data_dir_in(&base)
}

pub(crate) fn app_data_dir_in(base: &Path) -> PathBuf {
    let dir = base.join(DIR_NAME);
    let legacy = base.join(LEGACY_DIR_NAME);
    if !dir.exists() && legacy.is_dir() && fs::rename(&legacy, &dir).is_err() {
        let _ = fs::create_dir_all(&legacy);
        return legacy;
    }
    let _ = fs::create_dir_all(&dir);
    dir
}
