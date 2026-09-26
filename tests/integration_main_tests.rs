// Integration tests for the rusk binary (main.rs argument parsing and flag filtering)

use std::process::Stdio;

mod common;

#[test]
fn test_binary_del_help() {
    let sb = common::Sandbox::new();
    let out = sb.cmd().args(["del", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Delete tasks"));
    assert!(stdout.contains("--done"));
}

#[test]
fn test_binary_mark_help() {
    let sb = common::Sandbox::new();
    let out = sb.cmd().args(["mark", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Toggle task completion"));
}

#[test]
fn test_binary_mark_help_after_id_leaves_db_unchanged() {
    let sb = common::Sandbox::new();
    let db = r#"[
        {"id":1,"text":"Task 1","date":null,"done":false,"priority":false},
        {"id":2,"text":"Task 2","date":null,"done":false,"priority":false}
    ]"#;
    sb.write_db(db);

    let out = sb.cmd().args(["mark", "1", "-h"]).output().unwrap();
    assert!(
        out.status.success(),
        "mark 1 -h should print help and exit 0"
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Toggle") || stdout.contains("mark"),
        "expected mark subcommand help on stdout: {stdout}"
    );

    let db_after: Vec<serde_json::Value> = serde_json::from_str(&sb.read_db()).unwrap();
    let t1 = db_after.iter().find(|t| t["id"] == 1).unwrap();
    assert!(
        !t1["done"].as_bool().unwrap(),
        "task 1 must stay undone when -h requests help"
    );
}

#[test]
fn test_binary_add_date_flag_help_value() {
    let sb = common::Sandbox::new();
    let out = sb.cmd()
        .args(["add", "x", "-d", "-h"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Add a new task"));
}

#[test]
fn test_binary_add_help_includes_relative_date_syntax() {
    let sb = common::Sandbox::new();
    let out = sb.cmd().args(["add", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Relative") && stdout.contains("10d5w"),
        "long help should document relative dates:\n{stdout}"
    );
}

#[test]
fn test_binary_root_long_help_mentions_dates() {
    let sb = common::Sandbox::new();
    let out = sb.cmd().args(["--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("rusk add -d") && stdout.contains("rusk edit"),
        "root --help should point at due dates (add flag vs edit first line):\n{stdout}"
    );
}

#[test]
fn test_binary_list_first_line_omits_body_lines() {
    let sb = common::Sandbox::new();
    let db = r#"[
        {"id":1,"text":"Title line\nBody paragraph","date":null,"done":false,"priority":false}
    ]"#;
    sb.write_db(db);

    let out = sb.cmd()
        .env("RUSK_NO_COLOR", "1")
        .args(["list", "-c"])
        .output()
        .unwrap();
    assert!(out.status.success(), "list -c should succeed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Title line"),
        "compact list should include first line:\n{stdout}"
    );
    assert!(
        !stdout.contains("Body paragraph"),
        "compact list must not print continuation lines:\n{stdout}"
    );
}

#[test]
fn test_binary_add_rejects_invalid_relative_date() {
    let sb = common::Sandbox::new();
    sb.write_db(r#"[{"id":1,"text":"T","date":null,"done":false,"priority":false}]"#);

    let out = sb.cmd()
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
fn test_binary_add_interactive_requires_tty() {
    let sb = common::Sandbox::new();
    sb.write_db("[]");

    let out = sb.cmd()
        .arg("add")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "rusk add with no pipe TTY should fail when stdout is not a terminal"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    // With `interactive` (default), non-TTY should ask for a terminal. With `--no-default-features`
    // the binary has no TUI, so `rusk add` with no text is rejected with an empty-text error.
    assert!(
        stderr.contains("terminal")
            || stderr.contains("TTY")
            || stderr.contains("tty")
            || stderr.contains("Task text cannot be empty"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_binary_add_rejects_d_clear_with_no_text() {
    let sb = common::Sandbox::new();
    let out = sb.cmd().args(["add", "-d", "_"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("clear") || stderr.contains("`_`"),
        "stderr should explain -d _ with no text: {stderr}"
    );
}

#[test]
fn test_binary_edit_date_flag_help_value() {
    let sb = common::Sandbox::new();
    let out = sb.cmd()
        .args(["edit", "1", "-d", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Edit tasks"));
}

#[test]
fn test_binary_edit_trailing_help_after_id() {
    let sb = common::Sandbox::new();
    for args in [["e", "22", "-h"], ["e", "22", "--help"]] {
        let out = sb.cmd().args(args).output().unwrap();
        assert!(out.status.success(), "args={args:?}");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Edit tasks"),
            "args={args:?} stdout={stdout}"
        );
    }
}

#[test]
fn test_binary_edit_help_includes_relative_date_syntax() {
    let sb = common::Sandbox::new();
    let out = sb.cmd().args(["edit", "--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.to_lowercase().contains("relative")
            && stdout.contains("first line")
            && stdout.contains("leading `+`")
            && stdout.contains("current due date"),
        "edit long help should document relative, + from current due date, and first line:\n{stdout}"
    );
}

#[test]
fn test_binary_edit_plus_relative_from_existing_date() {
    let sb = common::Sandbox::new();
    let db = r#"[
        {"id":1,"text":"Task","date":"2025-06-01","done":false,"priority":false}
    ]"#;
    sb.write_db(db);

    let out = sb.cmd()
        .args(["edit", "1", "-d", "+1w"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "edit -d +1w should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let db_after: Vec<serde_json::Value> = serde_json::from_str(&sb.read_db()).unwrap();
    assert_eq!(db_after[0]["date"], "2025-06-08");
}

#[test]
fn test_binary_edit_rejects_bare_date_flag() {
    let sb = common::Sandbox::new();
    sb.write_db(r#"[{"id":1,"text":"T","date":null,"done":false,"priority":false}]"#);

    let out = sb.cmd().args(["edit", "1", "-d"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("a value is required for '--date <DATE>'"),
        "stderr={stderr}"
    );
    assert!(sb.read_db().contains(r#""text":"T""#), "nothing may be written");
}

#[test]
fn test_binary_edit_with_date_flag_sets_date_and_text() {
    let sb = common::Sandbox::new();
    let db = r#"[
        {"id":1,"text":"Original","date":null,"done":false,"priority":false}
    ]"#;
    sb.write_db(db);

    let out = sb.cmd()
        .args(["edit", "1", "Updated text", "-d", "15-06-2025"])
        .output()
        .unwrap();
    assert!(out.status.success(), "edit with -d and text should succeed");

    let db_after: Vec<serde_json::Value> = serde_json::from_str(&sb.read_db()).unwrap();
    let t = &db_after[0];
    assert_eq!(t["text"], "Updated text");
    assert_eq!(t["date"], "2025-06-15");
}

#[test]
fn test_binary_mark_error_when_only_flags() {
    let sb = common::Sandbox::new();
    sb.write_db(r#"[{"id":1,"text":"Task","date":null,"done":false,"priority":false}]"#);

    let out = sb.cmd().args(["mark", "--", "-"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("'-' is not a task id"), "{stderr}");
}

#[test]
fn test_binary_root_help_documents_rusk_no_color() {
    let sb = common::Sandbox::new();
    let out = sb.cmd().args(["--help"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("RUSK_NO_COLOR"),
        "root --help should document RUSK_NO_COLOR:\n{stdout}"
    );
}

#[test]
fn test_binary_rusk_no_color_disables_ansi_escapes() {
    let sb = common::Sandbox::new();
    sb.write_db(r#"[{"id":1,"text":"Task","date":null,"done":false,"priority":false}]"#);

    // Force colors on via CLICOLOR_FORCE; baseline run should contain ANSI escapes.
    let out_colored = sb.cmd()
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
    let out_plain = sb.cmd()
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
    assert!(stderr_plain.contains("'-' is not a task id"), "{stderr_plain}");
}

#[test]
fn test_binary_mark_priority_toggles_and_preserves_across_done() {
    let sb = common::Sandbox::new();
    sb.write_db(r#"[{"id":1,"text":"Task","date":null,"done":false,"priority":false}]"#);

    // `rusk m 1 -p` → priority=true, done=false.
    let out = sb.cmd().args(["mark", "1", "-p"]).output().unwrap();
    assert!(out.status.success(), "mark -p should succeed: {out:?}");
    let db: Vec<serde_json::Value> = serde_json::from_str(&sb.read_db()).unwrap();
    assert_eq!(db[0]["priority"], true);
    assert_eq!(db[0]["done"], false);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("priority"),
        "stdout should mention priority:\n{stdout}"
    );

    // `rusk m 1` → done=true, priority preserved.
    sb.cmd().args(["mark", "1"]).output().unwrap();
    let db: Vec<serde_json::Value> = serde_json::from_str(&sb.read_db()).unwrap();
    assert_eq!(db[0]["done"], true);
    assert_eq!(db[0]["priority"], true);

    // `rusk m 1` → reverts to priority (done=false, priority still true).
    sb.cmd().args(["mark", "1"]).output().unwrap();
    let db: Vec<serde_json::Value> = serde_json::from_str(&sb.read_db()).unwrap();
    assert_eq!(db[0]["done"], false);
    assert_eq!(db[0]["priority"], true);

    // `rusk m 1 -p` again → priority cleared.
    sb.cmd().args(["mark", "1", "-p"]).output().unwrap();
    let db: Vec<serde_json::Value> = serde_json::from_str(&sb.read_db()).unwrap();
    assert_eq!(db[0]["done"], false);
    assert_eq!(db[0]["priority"], false);
}

#[test]
fn test_binary_rusk_no_color_empty_does_not_disable() {
    let sb = common::Sandbox::new();
    sb.write_db(r#"[{"id":1,"text":"Task","date":null,"done":false,"priority":false}]"#);

    // Empty value is treated as "not set" (NO_COLOR semantics); colors stay forced on.
    let out = sb.cmd()
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

#[test]
fn test_binary_add_with_after_shows_deps_and_mark_is_not_blocked() {
    let sb = common::Sandbox::new();
    sb.write_db(
        r#"[{"id":1,"text":"base","date":null,"done":false,"priority":false},
            {"id":2,"text":"other","date":null,"done":false,"priority":false}]"#,
    );

    let out = sb.cmd()
        .env("RUSK_NO_COLOR", "1")
        .args(["add", "deploy", "-a", "1,2"])
        .output()
        .unwrap();
    assert!(out.status.success(), "add -a should succeed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("deploy (1,2)"),
        "added task should show its deps:\n{stdout}"
    );
    assert!(sb.read_db().contains("\"after\""), "deps must be persisted");

    // The list appends the deps in parentheses after the text.
    let out = sb.cmd()
        .env("RUSK_NO_COLOR", "1")
        .args(["list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("deploy (1,2)"),
        "list should show the deps suffix:\n{stdout}"
    );

    // Dependencies are advisory (an ordering hint for agents/tooling):
    // manual marking works even while the deps are unfinished.
    let out = sb.cmd()
        .env("RUSK_NO_COLOR", "1")
        .args(["mark", "3"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Marked task as done"),
        "mark must not be blocked by unfinished deps:\n{stdout}"
    );
    assert!(sb.read_db().contains("\"done\": true"), "task 3 must be done");
}

#[test]
fn test_binary_add_rejects_missing_after_ids() {
    let sb = common::Sandbox::new();
    sb.write_db(r#"[{"id":1,"text":"T","date":null,"done":false,"priority":false}]"#);

    let out = sb.cmd()
        .args(["add", "x", "-a", "9"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "missing dep id must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("missing task(s): 9"),
        "error should name the missing id:\n{stderr}"
    );

    let out = sb.cmd()
        .args(["add", "x", "-a", "oops"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "non-numeric --after must fail");
}

#[test]
fn test_binary_edit_sets_and_clears_after() {
    let sb = common::Sandbox::new();
    sb.write_db(
        r#"[{"id":1,"text":"base","date":null,"done":false,"priority":false},
            {"id":2,"text":"dep","date":null,"done":false,"priority":false}]"#,
    );

    let out = sb.cmd()
        .env("RUSK_NO_COLOR", "1")
        .args(["edit", "1", "-a", "2"])
        .output()
        .unwrap();
    assert!(out.status.success(), "edit -a should succeed: {out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("- after: 2"),
        "edit should report the new deps:\n{stdout}"
    );
    assert!(sb.read_db().contains("\"after\""), "deps must be persisted");

    // Self-dependency is rejected.
    let out = sb.cmd().args(["edit", "1", "-a", "1"]).output().unwrap();
    assert!(!out.status.success(), "self-dependency must fail");

    // `_` clears the list.
    let out = sb.cmd()
        .env("RUSK_NO_COLOR", "1")
        .args(["edit", "1", "-a", "_"])
        .output()
        .unwrap();
    assert!(out.status.success(), "edit -a _ should succeed: {out:?}");
    assert!(
        !sb.read_db().contains("\"after\""),
        "cleared deps must not be persisted"
    );
}

