// Integration tests for the rusk binary (main.rs argument parsing and flag filtering)

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

mod common;

static BIN_TEST_MUTEX: Mutex<()> = Mutex::new(());

fn rusk_bin() -> PathBuf {
    common::require_rusk_bin().expect("rusk binary not found, run cargo build")
}

fn debug_db_path() -> PathBuf {
    std::env::temp_dir().join("rusk_debug").join("tasks.json")
}

fn setup_test_db(tasks_json: &str) {
    let db_path = debug_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&db_path, tasks_json).unwrap();
}

fn read_db() -> String {
    let p = debug_db_path();
    if p.exists() {
        fs::read_to_string(&p).unwrap_or_default()
    } else {
        String::new()
    }
}

#[test]
fn test_binary_del_help() {
    let rusk = rusk_bin();
    let out = Command::new(&rusk).args(["del", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Delete tasks"));
    assert!(stdout.contains("--done"));
}

#[test]
fn test_binary_mark_help() {
    let rusk = rusk_bin();
    let out = Command::new(&rusk).args(["mark", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Mark tasks"));
}

#[test]
fn test_binary_mark_filters_trailing_h() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    let db = r#"[
        {"id":1,"text":"Task 1","date":null,"done":false},
        {"id":2,"text":"Task 2","date":null,"done":false}
    ]"#;
    setup_test_db(db);

    let rusk = rusk_bin();
    let out = Command::new(&rusk).args(["mark", "1", "-h"]).output().unwrap();
    assert!(out.status.success(), "mark 1 -h should succeed (filter -h from ids)");

    let db_after: Vec<serde_json::Value> = serde_json::from_str(&read_db()).unwrap();
    let t1 = db_after.iter().find(|t| t["id"] == 1).unwrap();
    assert!(t1["done"].as_bool().unwrap(), "task 1 should be marked done");
}

#[test]
fn test_binary_edit_inline_date() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    let db = r#"[
        {"id":1,"text":"Original","date":null,"done":false}
    ]"#;
    setup_test_db(db);

    let rusk = rusk_bin();
    let out = Command::new(&rusk)
        .args(["edit", "1", "Updated text", "-d", "15-06-2025"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Updated") || stdout.is_empty());

    let db_after: Vec<serde_json::Value> = serde_json::from_str(&read_db()).unwrap();
    let t = &db_after[0];
    assert_eq!(t["text"], "Updated text");
    assert_eq!(t["date"], "2025-06-15");
}

#[test]
fn test_binary_mark_error_when_only_flags() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    setup_test_db(r#"[{"id":1,"text":"Task","date":null,"done":false}]"#);

    let rusk = rusk_bin();
    let out = Command::new(&rusk).args(["mark", "--", "-"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("No valid task IDs"));
}
