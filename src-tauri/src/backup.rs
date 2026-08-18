//! 本机缓存与用户配置的备份/恢复。不含 Cursor 钥匙串 token。

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use rusqlite::{backup::Backup, Connection};
use serde::{Deserialize, Serialize};

pub const MANIFEST_NAME: &str = "manifest.json";
pub const DB_NAME: &str = "usage.sqlite";
pub const PRICES_NAME: &str = "prices.json";
pub const SNAPSHOT_NAME: &str = "litellm_prices.json";
pub const BUDGET_NAME: &str = "budget.json";

#[derive(Debug, Clone)]
pub struct AppDataPaths {
    pub db_path: PathBuf,
    pub prices_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub budget_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupManifest {
    pub created_at: String,
    pub files: Vec<String>,
    pub note: String,
}

fn default_note() -> String {
    "不含 Cursor 钥匙串中的 WorkosCursorSessionToken；恢复会覆盖当前缓存与单价/预算配置。"
        .to_string()
}

fn copy_if_exists(
    src: &Path,
    dest: &Path,
    files: &mut Vec<String>,
    name: &str,
) -> Result<(), String> {
    if !src.exists() {
        return Ok(());
    }
    fs::copy(src, dest).map_err(|e| format!("复制 {name} 失败：{e}"))?;
    files.push(name.to_string());
    Ok(())
}

pub fn backup_sqlite(conn: &Connection, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if dest.exists() {
        fs::remove_file(dest).map_err(|e| e.to_string())?;
    }
    let mut target = Connection::open(dest).map_err(|e| e.to_string())?;
    let backup = Backup::new(conn, &mut target).map_err(|e| e.to_string())?;
    backup
        .run_to_completion(100, std::time::Duration::from_millis(0), None)
        .map_err(|e| e.to_string())
}

pub fn backup_to(
    conn: &Connection,
    dest_dir: &Path,
    paths: &AppDataPaths,
) -> Result<BackupManifest, String> {
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let mut files = Vec::new();

    backup_sqlite(conn, &dest_dir.join(DB_NAME))?;
    files.push(DB_NAME.to_string());

    copy_if_exists(
        &paths.prices_path,
        &dest_dir.join(PRICES_NAME),
        &mut files,
        PRICES_NAME,
    )?;
    copy_if_exists(
        &paths.snapshot_path,
        &dest_dir.join(SNAPSHOT_NAME),
        &mut files,
        SNAPSHOT_NAME,
    )?;
    copy_if_exists(
        &paths.budget_path,
        &dest_dir.join(BUDGET_NAME),
        &mut files,
        BUDGET_NAME,
    )?;

    let manifest = BackupManifest {
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        files,
        note: default_note(),
    };
    fs::write(
        dest_dir.join(MANIFEST_NAME),
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(manifest)
}

pub fn load_manifest(src_dir: &Path) -> Result<BackupManifest, String> {
    let text = fs::read_to_string(src_dir.join(MANIFEST_NAME))
        .map_err(|_| "不是有效的备份目录：缺少 manifest.json".to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("备份清单无效：{e}"))
}

/// 调用方必须先释放目标 sqlite 连接，否则 WAL 模式下无法安全覆盖。
pub fn restore_from(src_dir: &Path, paths: &AppDataPaths) -> Result<BackupManifest, String> {
    let manifest = load_manifest(src_dir)?;
    let src_db = src_dir.join(DB_NAME);
    if !src_db.exists() {
        return Err("备份目录缺少 usage.sqlite".to_string());
    }

    if let Some(parent) = paths.db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&src_db, &paths.db_path).map_err(|e| format!("恢复数据库失败：{e}"))?;
    remove_sidecar(&paths.db_path, "-wal");
    remove_sidecar(&paths.db_path, "-shm");

    restore_optional(src_dir, PRICES_NAME, &paths.prices_path)?;
    restore_optional(src_dir, SNAPSHOT_NAME, &paths.snapshot_path)?;
    restore_optional(src_dir, BUDGET_NAME, &paths.budget_path)?;
    Ok(manifest)
}

fn restore_optional(src_dir: &Path, name: &str, dest: &Path) -> Result<(), String> {
    let src = src_dir.join(name);
    if !src.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::copy(&src, dest).map_err(|e| format!("恢复 {name} 失败：{e}"))?;
    Ok(())
}

fn remove_sidecar(db_path: &Path, suffix: &str) {
    let sidecar = PathBuf::from(format!("{}{suffix}", db_path.to_string_lossy()));
    let _ = fs::remove_file(sidecar);
}
