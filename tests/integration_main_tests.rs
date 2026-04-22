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
fn test_binary_mark_help_after_id_leaves_db_unchanged() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    let db = r#"[
        {"id":1,"text":"Task 1","date":null,"done":false,"priority":false},
        {"id":2,"text":"Task 2","date":null,"done":false,"priority":false}
    ]"#;
    setup_test_db(db);

    let out = rusk_command().args(["mark", "1", "-h"]).output().unwrap();
    assert!(out.status.success(), "mark 1 -h should print help and exit 0");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Toggle") || stdout.contains("mark"),
        "expected mark subcommand help on stdout: {stdout}"
    );

    let db_after: Vec<serde_json::Value> = serde_json::from_str(&read_db()).unwrap();
    let t1 = db_after.iter().find(|t| t["id"] == 1).unwrap();
    assert!(
        !t1["done"].as_bool().unwrap(),
        "task 1 must stay undone when -h requests help"
    );
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
        stdout.contains("Relative") && stdout.contains("10d5w"),
        "long help should document relative dates:\n{stdout}"
    );
}

#[test]
fn test_binary_root_long_help_mentions_dates() {
    let out = rusk_command().args(["--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("rusk add -d") && stdout.contains("rusk edit"),
        "root --help should point at due dates (add flag vs edit first line):\n{stdout}"
    );
}

#[test]
fn test_binary_add_rejects_invalid_relative_date() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();
    setup_test_db(r#"[{"id":1,"text":"T","date":null,"done":false,"priority":false}]"#);

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
        stdout.to_lowercase().contains("relative") && stdout.contains("first line"),
        "edit long help should document relative + absolute dates and first-line due date:\n{stdout}"
    );
}

#[test]
fn test_binary_edit_rejects_bare_date_flag() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    setup_test_db(r#"[{"id":1,"text":"T","date":null,"done":false,"priority":false}]"#);

    let out = rusk_command().args(["edit", "1", "-d"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("without a value")
            || stderr.contains("--date")
            || stderr.contains("first line of the task"),
        "stderr={stderr}"
    );
}

#[test]
fn test_binary_edit_with_date_flag_sets_date_and_text() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    let db = r#"[
        {"id":1,"text":"Original","date":null,"done":false,"priority":false}
    ]"#;
    setup_test_db(db);

    let out = rusk_command()
        .args(["edit", "1", "Updated text", "-d", "15-06-2025"])
        .output()
        .unwrap();
    assert!(out.status.success(), "edit with -d and text should succeed");

    let db_after: Vec<serde_json::Value> = serde_json::from_str(&read_db()).unwrap();
    let t = &db_after[0];
    assert_eq!(t["text"], "Updated text");
    assert_eq!(t["date"], "2025-06-15");
}

#[test]
fn test_binary_mark_error_when_only_flags() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    setup_test_db(r#"[{"id":1,"text":"Task","date":null,"done":false,"priority":false}]"#);

    let out = rusk_command().args(["mark", "--", "-"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("No valid task IDs"));
}

#[test]
fn test_binary_root_help_documents_rusk_no_color() {
    let out = rusk_command().args(["--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("RUSK_NO_COLOR"),
        "root --help should document RUSK_NO_COLOR:\n{stdout}"
    );
}

#[test]
fn test_binary_rusk_no_color_disables_ansi_escapes() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();
    setup_test_db(r#"[{"id":1,"text":"Task","date":null,"done":false,"priority":false}]"#);

    // Force colors on via CLICOLOR_FORCE; baseline run should contain ANSI escapes.
    let out_colored = rusk_command()
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .env_remove("RUSK_NO_COLOR")
        .args(["mark", "--", "-"])
        .output()
        .unwrap();
    let stderr_colored = String::from_utf8_lossy(&out_colored.stderr);
    assert!(
        stderr_colored.contains("\x1b["),
        "baseline stderr should contain ANSI escapes when CLICOLOR_FORCE=1:\n{stderr_colored:?}"
    );

    // With RUSK_NO_COLOR=1 the same run must not contain ANSI escapes.
    let out_plain = rusk_command()
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .env("RUSK_NO_COLOR", "1")
        .args(["mark", "--", "-"])
        .output()
        .unwrap();
    let stderr_plain = String::from_utf8_lossy(&out_plain.stderr);
    assert!(
        !stderr_plain.contains("\x1b["),
        "RUSK_NO_COLOR=1 stderr must not contain ANSI escapes:\n{stderr_plain:?}"
    );
    assert!(stderr_plain.contains("No valid task IDs"));
}

#[test]
fn test_binary_mark_priority_toggles_and_preserves_across_done() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();

    setup_test_db(r#"[{"id":1,"text":"Task","date":null,"done":false,"priority":false}]"#);

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
    assert_eq!(db[0]["priority"], false);
}

#[test]
fn test_binary_rusk_no_color_empty_does_not_disable() {
    let _guard = BIN_TEST_MUTEX.lock().unwrap();
    setup_test_db(r#"[{"id":1,"text":"Task","date":null,"done":false,"priority":false}]"#);

    // Empty value is treated as "not set" (NO_COLOR semantics); colors stay forced on.
    let out = rusk_command()
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .env("RUSK_NO_COLOR", "")
        .args(["mark", "--", "-"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("\x1b["),
        "empty RUSK_NO_COLOR should not disable colors:\n{stderr:?}"
    );
}
