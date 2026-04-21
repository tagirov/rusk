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

/// Spawn `rusk` with `RUSK_DB` pointing at the integration harness file. Release binaries
/// do not detect "test mode", so without this they would use `~/.rusk` while tests write
/// under `debug_db_path()`.
fn rusk_command() -> Command {
    let mut cmd = Command::new(rusk_bin());
    cmd.env("RUSK_DB", debug_db_path());
    cmd
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
    let out = rusk_command().args(["del", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Delete tasks"));
    assert!(stdout.contains("--done"));
}

#[test]
fn test_binary_mark_help() {
    let out = rusk_command().args(["mark", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Toggle task completion"));
}

#[test]
fn test_binary_mark_filters_trailing_h() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    let db = r#"[
        {"id":1,"text":"Task 1","date":null,"done":false},
        {"id":2,"text":"Task 2","date":null,"done":false}
    ]"#;
    setup_test_db(db);

    let out = rusk_command().args(["mark", "1", "-h"]).output().unwrap();
    assert!(out.status.success(), "mark 1 -h should succeed (filter -h from ids)");

    let db_after: Vec<serde_json::Value> = serde_json::from_str(&read_db()).unwrap();
    let t1 = db_after.iter().find(|t| t["id"] == 1).unwrap();
    assert!(t1["done"].as_bool().unwrap(), "task 1 should be marked done");
}

#[test]
fn test_binary_add_date_flag_help_value() {
    let out = rusk_command()
        .args(["add", "x", "-d", "-h"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Add a new task"));
}

#[test]
fn test_binary_add_help_includes_relative_date_syntax() {
    let out = rusk_command().args(["add", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Relative")
            && stdout.contains("10d5w")
            && stdout.contains("interactive date entry"),
        "long help should document relative and interactive edit dates:\n{stdout}"
    );
}

#[test]
fn test_binary_root_long_help_mentions_dates() {
    let out = rusk_command().args(["--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("2w") || stdout.contains("10d5w") || stdout.contains("relative"),
        "root --help should point at date forms:\n{stdout}"
    );
}

#[test]
fn test_binary_add_rejects_invalid_relative_date() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();
    setup_test_db(r#"[{"id":1,"text":"T","date":null,"done":false}]"#);

    let out = rusk_command()
        .args(["add", "x", "-d", "0d"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Relative") || stderr.contains("positive"),
        "stderr={stderr}"
    );
}

#[test]
fn test_binary_edit_date_flag_help_value() {
    let out = rusk_command()
        .args(["edit", "1", "-d", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Edit tasks"));
}

#[test]
fn test_binary_edit_trailing_help_after_id() {
    for args in [["e", "22", "-h"], ["e", "22", "--help"]] {
        let out = rusk_command().args(args).output().unwrap();
        assert!(out.status.success(), "args={args:?}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("Edit tasks"), "args={args:?} stdout={stdout}");
    }
}

#[test]
fn test_binary_edit_help_includes_relative_date_syntax() {
    let out = rusk_command().args(["edit", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Relative")
            && stdout.contains("10d5w")
            && stdout.contains("interactive date entry"),
        "edit long help should document relative and interactive dates:\n{stdout}"
    );
}

#[test]
fn test_binary_edit_inline_date() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    let db = r#"[
        {"id":1,"text":"Original","date":null,"done":false}
    ]"#;
    setup_test_db(db);

    let out = rusk_command()
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

    let out = rusk_command().args(["mark", "--", "-"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("No valid task IDs"));
}

#[test]
fn test_binary_root_help_documents_rusk_no_colors() {
    let out = rusk_command().args(["--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("RUSK_NO_COLORS"),
        "root --help should document RUSK_NO_COLORS:\n{stdout}"
    );
}

#[test]
fn test_binary_rusk_no_colors_disables_ansi_escapes() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();
    setup_test_db(r#"[{"id":1,"text":"Task","date":null,"done":false}]"#);

    // Force colors on via CLICOLOR_FORCE; baseline run should contain ANSI escapes.
    let out_colored = rusk_command()
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .env_remove("RUSK_NO_COLORS")
        .args(["mark", "--", "-"])
        .output()
        .unwrap();
    let stderr_colored = String::from_utf8_lossy(&out_colored.stderr);
    assert!(
        stderr_colored.contains("\x1b["),
        "baseline stderr should contain ANSI escapes when CLICOLOR_FORCE=1:\n{stderr_colored:?}"
    );

    // With RUSK_NO_COLORS=1 the same run must not contain ANSI escapes.
    let out_plain = rusk_command()
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .env("RUSK_NO_COLORS", "1")
        .args(["mark", "--", "-"])
        .output()
        .unwrap();
    let stderr_plain = String::from_utf8_lossy(&out_plain.stderr);
    assert!(
        !stderr_plain.contains("\x1b["),
        "RUSK_NO_COLORS=1 stderr must not contain ANSI escapes:\n{stderr_plain:?}"
    );
    assert!(stderr_plain.contains("No valid task IDs"));
}

#[test]
fn test_binary_mark_priority_toggles_and_preserves_across_done() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    setup_test_db(r#"[{"id":1,"text":"Task","date":null,"done":false}]"#);

    // `rusk m 1 -p` → priority=true, done=false.
    let out = rusk_command().args(["mark", "1", "-p"]).output().unwrap();
    assert!(out.status.success(), "mark -p should succeed: {out:?}");
    let db: Vec<serde_json::Value> = serde_json::from_str(&read_db()).unwrap();
    assert_eq!(db[0]["priority"], true);
    assert_eq!(db[0]["done"], false);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("priority"), "stdout should mention priority:\n{stdout}");

    // `rusk m 1` → done=true, priority preserved.
    rusk_command().args(["mark", "1"]).output().unwrap();
    let db: Vec<serde_json::Value> = serde_json::from_str(&read_db()).unwrap();
    assert_eq!(db[0]["done"], true);
    assert_eq!(db[0]["priority"], true);

    // `rusk m 1` → reverts to priority (done=false, priority still true).
    rusk_command().args(["mark", "1"]).output().unwrap();
    let db: Vec<serde_json::Value> = serde_json::from_str(&read_db()).unwrap();
    assert_eq!(db[0]["done"], false);
    assert_eq!(db[0]["priority"], true);

    // `rusk m 1 -p` again → priority cleared.
    rusk_command().args(["mark", "1", "-p"]).output().unwrap();
    let db: Vec<serde_json::Value> = serde_json::from_str(&read_db()).unwrap();
    assert_eq!(db[0]["done"], false);
    // `skip_serializing_if = Not::not` omits the field when false; treat missing as false.
    let p = db[0].get("priority").and_then(|v| v.as_bool()).unwrap_or(false);
    assert!(!p, "priority should be cleared, got {:?}", db[0].get("priority"));
}

#[test]
fn test_binary_loads_legacy_db_without_priority_field() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    // Old DB schema (no `priority` field) — must still load.
    setup_test_db(r#"[{"id":1,"text":"Legacy","date":null,"done":false}]"#);

    let out = rusk_command().args(["list"]).output().unwrap();
    assert!(out.status.success(), "list should succeed on legacy DB");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Legacy"));
}

#[test]
fn test_binary_rusk_no_colors_empty_does_not_disable() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();
    setup_test_db(r#"[{"id":1,"text":"Task","date":null,"done":false}]"#);

    // Empty value is treated as "not set" (NO_COLOR semantics); colors stay forced on.
    let out = rusk_command()
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .env("RUSK_NO_COLORS", "")
        .args(["mark", "--", "-"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("\x1b["),
        "empty RUSK_NO_COLORS should not disable colors:\n{stderr:?}"
    );
}
