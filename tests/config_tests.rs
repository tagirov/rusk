// Integration tests for the configuration file: RUSK_CONFIG resolution,
// auto-creation, theme colors, no_color precedence, warnings, compact, backup.

use std::fs;
use std::process::Command;

mod common;

/// `rusk` with the given RUSK_CONFIG value and ANSI colors forced on
/// (`colored` would otherwise strip them from piped output).
fn rusk_with_config(sb: &common::Sandbox, cfg_path: &str) -> Command {
    let mut cmd = sb.cmd();
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
    let sb = common::Sandbox::new();
    sb.write_db(ONE_TASK_DB);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    assert!(!cfg_path.exists());

    let out = rusk_with_config(&sb, cfg_path.to_str().unwrap())
        .arg("list")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(cfg_path.exists(), "config file should be auto-created");

    let content = fs::read_to_string(&cfg_path).unwrap();
    assert!(content.contains("rusk configuration file"));
    assert!(content.contains("priority_marker = accent"));

    // The shipped template must parse without warnings on the next run.
    let out2 = rusk_with_config(&sb, cfg_path.to_str().unwrap())
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
    let sb = common::Sandbox::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "error = green\n").unwrap();

    // `rusk edit` with no args errors out before opening the database.
    let out = rusk_with_config(&sb, cfg_path.to_str().unwrap())
        .arg("edit")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no task ids given"), "{stderr}");
    assert!(
        stderr.contains("\x1b[32m"),
        "error should be green from the config, got: {stderr:?}"
    );
    assert!(!stderr.contains("\x1b[31m"));
}

#[test]
fn test_empty_rusk_config_disables_config_and_keeps_red() {
    let sb = common::Sandbox::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "error = green\n").unwrap();

    // RUSK_CONFIG="" disables the config entirely, even though the file exists.
    let out = rusk_with_config(&sb, "").arg("edit").output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("\x1b[31m"),
        "default error color should be red, got: {stderr:?}"
    );
}

#[test]
fn test_env_no_color_beats_config() {
    let sb = common::Sandbox::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "no_color = false\n").unwrap();

    let out = rusk_with_config(&sb, cfg_path.to_str().unwrap())
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
    let sb = common::Sandbox::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "no_color = true\n").unwrap();

    let out = rusk_with_config(&sb, cfg_path.to_str().unwrap())
        .arg("edit")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Error"));
    assert!(!stderr.contains("\x1b["), "got: {stderr:?}");
}

#[test]
fn test_malformed_config_warns_with_line_numbers_and_still_works() {
    let sb = common::Sandbox::new();
    sb.write_db(ONE_TASK_DB);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "not a valid line\nerror = blu\n").unwrap();

    let out = rusk_with_config(&sb, cfg_path.to_str().unwrap())
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
    let sb = common::Sandbox::new();
    sb.write_db(ONE_TASK_DB);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "no_colour = true\n").unwrap();

    let out = rusk_with_config(&sb, cfg_path.to_str().unwrap())
        .arg("list")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unused variable 'no_colour'"), "got: {stderr}");
}

#[test]
fn test_compact_from_config() {
    let sb = common::Sandbox::new();
    sb.write_db(
        r#"[{"id":1,"text":"First line\nSecond line","date":null,"done":false,"priority":false}]"#,
    );

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "compact = true\n").unwrap();

    // Bare `rusk` should honor `compact = true` from the config.
    let out = rusk_with_config(&sb, cfg_path.to_str().unwrap()).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("First line"));
    assert!(!stdout.contains("Second line"), "got: {stdout}");

    // Without the config the full text is shown.
    let out = rusk_with_config(&sb, "").output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Second line"), "got: {stdout}");
}

#[test]
fn test_backup_toggle_from_config() {
    let sb = common::Sandbox::new();
    sb.write_db(ONE_TASK_DB);
    let backup_path = sb.db_path().with_extension("json.backup");
    let _ = fs::remove_file(&backup_path);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "backup = false\n").unwrap();

    let out = rusk_with_config(&sb, cfg_path.to_str().unwrap())
        .args(["add", "no backup expected"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        !backup_path.exists(),
        "`backup = false` must skip the backup copy"
    );

    let out = rusk_with_config(&sb, "")
        .args(["add", "backup expected"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(backup_path.exists(), "backups are on by default");
}

#[test]
fn test_help_mentions_rusk_config() {
    let sb = common::Sandbox::new();
    let out = rusk_with_config(&sb, "").arg("--help").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("RUSK_CONFIG"));
}
