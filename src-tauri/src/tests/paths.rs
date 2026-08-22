use std::fs;

use crate::paths::app_data_dir_in;

#[test]
fn app_data_dir_creates_mabiao() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = app_data_dir_in(tmp.path());
    assert_eq!(dir, tmp.path().join("mabiao"));
    assert!(dir.is_dir());
}

#[test]
fn app_data_dir_renames_legacy_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let legacy = tmp.path().join("ai-usage-stats");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("cache.sqlite"), b"ok").unwrap();

    let dir = app_data_dir_in(tmp.path());
    assert_eq!(dir, tmp.path().join("mabiao"));
    assert!(dir.join("cache.sqlite").is_file());
    assert!(!legacy.exists());
}

#[test]
fn app_data_dir_keeps_mabiao_when_both_exist() {
    let tmp = tempfile::tempdir().unwrap();
    let current = tmp.path().join("mabiao");
    let legacy = tmp.path().join("ai-usage-stats");
    fs::create_dir_all(&current).unwrap();
    fs::create_dir_all(&legacy).unwrap();
    fs::write(current.join("new.txt"), b"new").unwrap();
    fs::write(legacy.join("old.txt"), b"old").unwrap();

    let dir = app_data_dir_in(tmp.path());
    assert_eq!(dir, current);
    assert!(current.join("new.txt").is_file());
    assert!(legacy.join("old.txt").is_file());
}
