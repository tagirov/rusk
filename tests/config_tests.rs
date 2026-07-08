// Integration tests for the configuration file: RUSK_CONFIG resolution,
// auto-creation, theme colors, no_color precedence, warnings, compact, backup.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

mod common;

// Tests below share the debug db path (RUSK_DB is ignored by debug binaries,
// which resolve to the same fixed path); serialize the ones that touch it.
static DB_MUTEX: Mutex<()> = Mutex::new(());

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

/// `rusk` with the given RUSK_CONFIG value and ANSI colors forced on
/// (`colored` would otherwise strip them from piped output).
fn rusk_with_config(cfg_path: &str) -> Command {
    let mut cmd = Command::new(common::require_rusk_bin().expect("rusk binary not found"));
    cmd.env("RUSK_DB", debug_db_path());
    cmd.env("RUSK_CONFIG", cfg_path);
    cmd.env("CLICOLOR_FORCE", "1");
    cmd.env_remove("RUSK_NO_COLOR");
    cmd.env_remove("NO_COLOR");
    cmd
}

const ONE_TASK_DB: &str =
    r#"[{"id":1,"text":"Config test task","date":null,"done":false,"priority":false}]"#;

#[test]
fn test_config_auto_created_and_parses_cleanly() {
    let _guard = DB_MUTEX.lock().unwrap();
    setup_test_db(ONE_TASK_DB);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    assert!(!cfg_path.exists());

    let out = rusk_with_config(cfg_path.to_str().unwrap())
        .arg("list")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(cfg_path.exists(), "config file should be auto-created");

    let content = fs::read_to_string(&cfg_path).unwrap();
    assert!(content.contains("rusk configuration file"));
    assert!(content.contains("priority_marker = accent"));

    // The shipped template must parse without warnings on the next run.
    let out2 = rusk_with_config(cfg_path.to_str().unwrap())
        .arg("list")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out2.stderr);
    assert!(
        !stderr.contains("Warning"),
        "template should parse cleanly, got: {stderr}"
    );
}

#[test]
fn test_theme_color_from_config_applies() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "error = green\n").unwrap();

    // `rusk edit` with no args errors out before opening the database.
    let out = rusk_with_config(cfg_path.to_str().unwrap())
        .arg("edit")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("No arguments provided"));
    assert!(
        stderr.contains("\x1b[32m"),
        "error should be green from the config, got: {stderr:?}"
    );
    assert!(!stderr.contains("\x1b[31m"));
}

#[test]
fn test_empty_rusk_config_disables_config_and_keeps_red() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "error = green\n").unwrap();

    // RUSK_CONFIG="" disables the config entirely, even though the file exists.
    let out = rusk_with_config("").arg("edit").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("\x1b[31m"),
        "default error color should be red, got: {stderr:?}"
    );
}

#[test]
fn test_env_no_color_beats_config() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "no_color = false\n").unwrap();

    let out = rusk_with_config(cfg_path.to_str().unwrap())
        .env("RUSK_NO_COLOR", "1")
        .arg("edit")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Error"));
    assert!(
        !stderr.contains("\x1b["),
        "RUSK_NO_COLOR must win over `no_color = false`, got: {stderr:?}"
    );
}

#[test]
fn test_config_no_color_disables_ansi() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "no_color = true\n").unwrap();

    let out = rusk_with_config(cfg_path.to_str().unwrap())
        .arg("edit")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Error"));
    assert!(!stderr.contains("\x1b["), "got: {stderr:?}");
}

#[test]
fn test_malformed_config_warns_with_line_numbers_and_still_works() {
    let _guard = DB_MUTEX.lock().unwrap();
    setup_test_db(ONE_TASK_DB);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "not a valid line\nerror = blu\n").unwrap();

    let out = rusk_with_config(cfg_path.to_str().unwrap())
        .arg("list")
        .output()
        .unwrap();
    assert!(out.status.success(), "config problems must never be fatal");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("cfg:1:"), "got: {stderr}");
    assert!(stderr.contains("cfg:2:"), "got: {stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Config test task"));
}

#[test]
fn test_unused_variable_warns() {
    let _guard = DB_MUTEX.lock().unwrap();
    setup_test_db(ONE_TASK_DB);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "no_colour = true\n").unwrap();

    let out = rusk_with_config(cfg_path.to_str().unwrap())
        .arg("list")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unused variable 'no_colour'"), "got: {stderr}");
}

#[test]
fn test_compact_from_config() {
    let _guard = DB_MUTEX.lock().unwrap();
    setup_test_db(
        r#"[{"id":1,"text":"First line\nSecond line","date":null,"done":false,"priority":false}]"#,
    );

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "compact = true\n").unwrap();

    // Bare `rusk` should honor `compact = true` from the config.
    let out = rusk_with_config(cfg_path.to_str().unwrap()).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("First line"));
    assert!(!stdout.contains("Second line"), "got: {stdout}");

    // Without the config the full text is shown.
    let out = rusk_with_config("").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Second line"), "got: {stdout}");
}

#[test]
fn test_backup_toggle_from_config() {
    let _guard = DB_MUTEX.lock().unwrap();
    setup_test_db(ONE_TASK_DB);
    let backup_path = debug_db_path().with_extension("json.backup");
    let _ = fs::remove_file(&backup_path);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "backup = false\n").unwrap();

    let out = rusk_with_config(cfg_path.to_str().unwrap())
        .args(["add", "no backup expected"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        !backup_path.exists(),
        "`backup = false` must skip the backup copy"
    );

    let out = rusk_with_config("")
        .args(["add", "backup expected"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(backup_path.exists(), "backups are on by default");
}

#[test]
fn test_help_mentions_rusk_config() {
    let out = rusk_with_config("").arg("--help").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("RUSK_CONFIG"));
}
