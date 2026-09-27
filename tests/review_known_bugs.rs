// Executable repros of confirmed findings from REVIEW.md, grouped by the
// refactor cluster (R-number) that closes them.
//
// Every test asserts the CORRECT behaviour. For a cluster that is still open
// the tests fail today and are marked `#[ignore]` to keep `cargo test`
// green; once the cluster is closed the `#[ignore]` is removed and the tests
// stay here as its regression suite. Workflow for a cluster:
//
//   cargo test --test review_known_bugs -- --ignored r2_     # see them fail
//   ...fix...
//   remove the #[ignore] of the tests that now pass (they become regressions)
//
// `cargo test --test review_known_bugs -- --ignored` runs all open ones.
// Closed so far: R1 (errors name their cause), R2 (argv of edit/mark/del:
// clap options, one id list), R3 (one set of rules for every list that is
// read), R4 (text measured in terminal cells), R5 ("cannot read" is not
// "zero tasks"), R6 (the terminal comes back), R8 (the draft lifecycle),
// R9 (restore), R10 + R13 (one reading of a location value), R11 (quoting
// in the completion scripts), R12 (lost updates), R14 (one atomic write
// routine), R15 (the ssh protocol), R16 (web UI: one way to change a task;
// its browser half is checked black-box, see REVIEW.md), R18 (the lifecycle
// of a SQLite connection) — all of R1-R18 — and of the clusters for the
// rest of REVIEW.md: R19 (output, terminals and messages), R24 (the editor:
// keys, words, selection, undo, the first-line date), R20 (`rusk serve`:
// requests, tokens, hosts, headers), R21 (`rusk sync` and the transports),
// R22 (`git_backend`).
// R6 and R8 need a pty
// for most of their repros: what Command can drive is here, the draft rules
// are unit-tested in `src/cli/editor/draft.rs`, and the rest is driven by a
// pty script against a release binary (see REVIEW.md).

use std::fs;
use std::path::Path;

use rusk::{Backend, StaleDatabase, Task, TaskManager};

mod common;
use common::Sandbox;

fn task(id: u32, text: &str) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: None,
        done: false,
        priority: false,
        after: Vec::new(),
    }
}

fn backend(path: &Path) -> Backend {
    Backend::from_local_path(path.to_path_buf()).unwrap()
}

fn stderr_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The sandbox database parsed as JSON values.
fn db_tasks(sb: &Sandbox) -> Vec<serde_json::Value> {
    serde_json::from_str(&sb.read_db()).expect("sandbox database must stay valid JSON")
}

fn text_of(tasks: &[serde_json::Value], id: u64) -> String {
    tasks
        .iter()
        .find(|t| t["id"] == id)
        .unwrap_or_else(|| panic!("task {id} is missing"))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

const THREE_TASKS_DB: &str = r#"[
    {"id":1,"text":"first task","date":"2027-01-01","done":false,"priority":false},
    {"id":2,"text":"second task","date":null,"done":false,"priority":false},
    {"id":3,"text":"third task","date":null,"done":false,"priority":false}
]"#;

// ---------------------------------------------------------------------------
// R1 — an error names its cause, not just the step that failed
// ---------------------------------------------------------------------------
//
// `main` printed `{err}`: of an anyhow chain that is the outermost context
// only ("Failed to read the database file"), and the OS error, curl's or
// ssh's stderr, SQLite's message under it never reached the user.

/// The whole chain, the way `main` and the web API print it.
fn chain_of(err: &anyhow::Error) -> String {
    format!("{err:#}")
}

/// REVIEW №22: the binary prints the cause. A directory in place of the
/// database cannot be read by any user, root included. (A debug binary is
/// held to its test database; a release one takes a directory in `RUSK_DB`
/// for `<dir>/tasks.json`.)
#[test]
#[cfg(debug_assertions)]
fn r1_the_binary_prints_the_cause_of_an_error() {
    let sb = Sandbox::new();
    fs::create_dir(sb.db_path()).unwrap();

    let out = sb.cmd().arg("list").output().unwrap();
    assert!(!out.status.success());
    let stderr = stderr_of(&out);
    assert!(stderr.contains("Failed to read the database file"), "{stderr}");
    assert!(
        stderr.contains(sb.db_path().to_str().unwrap()),
        "the error does not name the file: {stderr}"
    );
    assert!(stderr.contains("os error"), "the OS error is missing: {stderr}");
}

/// REVIEW №22 (repro 2): a CSV database that is not UTF-8.
#[test]
fn r1_an_unreadable_encoding_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("latin1.csv");
    fs::write(&path, b"id,text,date,done,priority,after\n1,caf\xe9,,false,false,\n").unwrap();

    let err = chain_of(&backend(&path).load().unwrap_err());
    assert!(err.contains("latin1.csv"), "{err}");
    assert!(err.contains("UTF-8"), "{err}");
}

/// A decoder names the place ("NDJSON line 2") and the cause under it says
/// what is wrong there; the flattened message kept only the place. NDJSON
/// also got the JSON corruption report, whose line and column are counted
/// within the whole file, not within one line.
#[test]
#[cfg(feature = "fmt-ndjson")]
fn r1_a_bad_ndjson_line_is_named_with_its_cause() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.ndjson");
    fs::write(
        &path,
        "{\"id\":1,\"text\":\"ok\",\"date\":null,\"done\":false,\"priority\":false}\nnot json\n",
    )
    .unwrap();

    let err = backend(&path).load().unwrap_err();
    // Already in the outermost message: that is all a caller printing
    // `{err}` shows.
    let message = err.to_string();
    assert!(message.contains("NDJSON line 2"), "{message}");
    assert!(message.contains("expected"), "serde's reason is missing: {message}");
    assert!(!message.contains("rm '"), "NDJSON got the JSON corruption report: {message}");
}

/// The JSON corruption report stays what it was for JSON.
#[test]
fn r1_json_keeps_its_corruption_report() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    fs::write(&path, "[{\"id\":1,\"text\":\"x\",").unwrap();

    let err = chain_of(&backend(&path).load().unwrap_err());
    assert!(err.contains("appears to be corrupted"), "{err}");
    assert!(err.contains("JSON parsing error"), "{err}");
}

/// A backup that does not parse must not be answered with the report for a
/// corrupted database: "delete the file ... or restore from backup" - while
/// restoring, about the backup itself.
#[test]
fn r1_a_broken_backup_is_reported_as_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "one")]).unwrap();
    fs::write(dir.path().join("tasks.json.backup"), "[{\"id\":1,\"text\":\"one\",").unwrap();

    let err = chain_of(&backend(&path).restore_from_backup().unwrap_err());
    assert!(err.contains("Failed to parse the JSON backup"), "{err}");
    assert!(err.contains("tasks.json.backup"), "{err}");
    assert!(err.contains("EOF"), "serde's reason is missing: {err}");
    assert!(!err.contains("rm '") && !err.contains("restore from backup"), "{err}");
}

/// REVIEW №22: a save that cannot create the database directory. (A file
/// in place of the directory fails for every user, root included.)
#[test]
fn r1_a_failed_save_names_the_directory_and_the_os_error() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("not-a-dir");
    fs::write(&blocker, "").unwrap();

    let err = chain_of(&backend(&blocker.join("tasks.json")).save(&[task(1, "x")]).unwrap_err());
    assert!(err.contains("not-a-dir"), "{err}");
    assert!(err.contains("os error"), "{err}");
}

/// REVIEW №22 (repro 3): nobody listens. curl's own words are the cause.
#[test]
#[cfg(feature = "backend-http")]
fn r1_an_unreachable_http_database_shows_what_curl_said() {
    if std::process::Command::new("curl").arg("--version").output().is_err() {
        eprintln!("skipping r1_an_unreachable_http_database_shows_what_curl_said: curl not found");
        return;
    }
    // Port 9 (discard) on loopback: closed everywhere that matters.
    let remote = rusk::backend::http::HttpBackend::new("http://127.0.0.1:9", None);
    let err = chain_of(&remote.load().unwrap_err());
    assert!(err.contains("failed to load tasks from http://127.0.0.1:9"), "{err}");
    assert!(err.contains("curl"), "curl's message is missing: {err}");
}

/// REVIEW №22: the ssh path showed ssh's stderr but not what rusk was
/// doing; now it reads like the http one.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r1_a_failed_ssh_read_names_the_location_and_the_cause() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let remote_dir = sb.path().join("remote");
    // A directory in place of the remote file: `cat` fails.
    fs::create_dir_all(remote_dir.join("tasks.json")).unwrap();
    let remote = format!("user@host:{}", remote_dir.join("tasks.json").display());

    let out = sb
        .cmd_with_fake_ssh()
        .env("RUSK_SYNC_REMOTE", &remote)
        .arg("sync")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = stderr_of(&out);
    assert!(stderr.contains(&format!("failed to load tasks from {remote}")), "{stderr}");
    assert!(stderr.contains("ssh failed"), "{stderr}");
    assert!(stderr.to_lowercase().contains("directory"), "cat's message is missing: {stderr}");
}

/// Clarification to REVIEW №22: whatever is wrong with a SQLite file came
/// out as "Failed to create the tasks table", with no path and no cause.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r1_sqlite_says_that_a_file_is_not_a_database_and_what_to_do() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    fs::write(&path, "garbage garbage garbage garbage garbage garbage").unwrap();

    let err = chain_of(&backend(&path).load().unwrap_err());
    assert!(err.contains("tasks.db"), "{err}");
    assert!(err.contains("not a valid SQLite database"), "{err}");
    assert!(err.contains("rusk restore"), "{err}");
    assert!(!err.contains("create the tasks table"), "{err}");
}

/// ...and a row that is not a task names the column.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r1_sqlite_names_the_column_that_does_not_parse() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE tasks (pos INTEGER PRIMARY KEY, id INTEGER, text TEXT, date TEXT,
                             done INTEGER, priority INTEGER, \"after\" TEXT);
         INSERT INTO tasks (pos, id, text, done, priority) VALUES (1, 1, NULL, 0, 0);",
    )
    .unwrap();
    drop(conn);

    let err = chain_of(&backend(&path).load().unwrap_err());
    assert!(err.contains("tasks.db"), "{err}");
    assert!(err.contains("text"), "the column is not named: {err}");

    // A date that is not one names the row and the task (R3: every cause
    // under the row it is in).
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("UPDATE tasks SET text = 'x', date = 'soon'", []).unwrap();
    drop(conn);
    let err = chain_of(&backend(&path).load().unwrap_err());
    assert!(err.contains("tasks.db"), "{err}");
    assert!(err.contains("row 1 (id 1): invalid date 'soon'"), "{err}");
}

/// Anything SQLite refuses for a reason of its own names the step, the
/// database and that reason (here: the schema's `CHECK (id > 0)`).
#[test]
#[cfg(feature = "backend-sqlite")]
fn r1_sqlite_names_the_step_the_database_and_the_reason() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");

    let err = chain_of(&backend(&path).save(&[task(0, "zero")]).unwrap_err());
    assert!(err.contains("Failed to insert task 0"), "{err}");
    assert!(err.contains("tasks.db"), "{err}");
    assert!(err.contains("CHECK constraint failed"), "{err}");
    // SQLite's message once, not once more as a bare result code.
    assert_eq!(err.matches("constraint failed").count(), 1, "{err}");
}

/// A read-only SQLite file: "Failed to clear the tasks table" said nothing.
#[test]
#[cfg(all(unix, feature = "backend-sqlite"))]
fn r1_sqlite_says_that_the_database_is_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    backend(&path).save(&[task(1, "one")]).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    if fs::OpenOptions::new().write(true).open(&path).is_ok() {
        eprintln!("skipping: running as a user that ignores file modes (root?)");
        return;
    }

    let err = chain_of(&backend(&path).save(&[task(1, "two")]).unwrap_err());
    assert!(err.contains("read-only"), "{err}");
    assert!(err.contains("nothing was saved"), "{err}");
    assert!(err.contains("tasks.db"), "{err}");
}

// ---------------------------------------------------------------------------
// R2 — argv parsing of `rusk edit` / id lists
// ---------------------------------------------------------------------------

fn date_of(tasks: &[serde_json::Value], id: u64) -> serde_json::Value {
    tasks
        .iter()
        .find(|t| t["id"] == id)
        .unwrap_or_else(|| panic!("task {id} is missing"))["date"]
        .clone()
}

fn done_of(tasks: &[serde_json::Value], id: u64) -> bool {
    tasks
        .iter()
        .find(|t| t["id"] == id)
        .unwrap_or_else(|| panic!("task {id} is missing"))["done"]
        .as_bool()
        .unwrap()
}

/// REVIEW №4: `--date=VALUE` was not recognised as a flag and became the
/// task text. Now `-d` / `--date` is a clap option: every spelling sets the
/// date and the text survives.
#[test]
fn r2_edit_long_date_flag_with_equals_does_not_replace_the_text() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb.cmd().args(["edit", "1", "--date=5-5-2027"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "first task");
    assert_eq!(date_of(&tasks, 1), "2027-05-05");
}

/// REVIEW №4: attached short form `-d2d`.
#[test]
fn r2_edit_attached_short_date_flag_does_not_replace_the_text() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb.cmd().args(["edit", "1", "-d5-5-2027"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "first task");
    assert_eq!(date_of(&tasks, 1), "2027-05-05");
}

/// REVIEW №4: `--after=VALUE`.
#[test]
fn r2_edit_long_after_flag_with_equals_does_not_replace_the_text() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb.cmd().args(["edit", "1", "--after=2"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "first task");
    assert_eq!(tasks[0]["after"], serde_json::json!([2]));
}

/// REVIEW №77: the flags were only recognised after the first positional;
/// `rusk edit -d 2w 1` was a clap error.
#[test]
fn r2_edit_flag_before_the_ids() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb
        .cmd()
        .args(["edit", "-d", "5-5-2027", "2", "new", "text"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 2), "new text");
    assert_eq!(date_of(&tasks, 2), "2027-05-05");
}

/// REVIEW №4: `--` must end flag parsing (as it does for `rusk add`), so a
/// text starting with a dash can be stored.
#[test]
fn r2_edit_double_dash_ends_flag_parsing() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb
        .cmd()
        .args(["edit", "1", "--", "-x", "text"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(text_of(&db_tasks(&sb), 1), "-x text");
}

/// REVIEW №4 (addendum): a text ending in `-h` printed the help (exit 0)
/// and dropped the edit, even after `--`.
#[test]
fn r2_edit_text_ending_with_help_after_double_dash_is_stored() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb
        .cmd()
        .args(["edit", "3", "--", "run", "tool", "with", "-h"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(text_of(&db_tasks(&sb), 3), "run tool with -h");
}

/// REVIEW №77: `rusk edit -d --help` (no id) was a clap error while
/// `rusk edit 1 -d --help` printed the help; `--date=-h` was text. All the
/// help spellings print the help and touch nothing.
#[test]
fn r2_edit_date_help_forms_print_the_help() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    for argv in [
        vec!["edit", "-d", "--help"],
        vec!["edit", "1", "--date=-h"],
        vec!["edit", "1", "--date", "--help"],
        vec!["edit", "1", "remove", "the", "-h"],
    ] {
        let out = sb.cmd().args(&argv).output().unwrap();
        assert!(out.status.success(), "{argv:?}: {}", stderr_of(&out));
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("Edit tasks by ID"), "{argv:?}: {stdout}");
    }
    assert_eq!(text_of(&db_tasks(&sb), 1), "first task");
    assert_eq!(sb.read_db().trim(), THREE_TASKS_DB.trim(), "help must not write");
}

/// REVIEW №77: `-d -1d` was reported as "-d without a value"; the value is
/// there, and the error is about it.
#[test]
fn r2_edit_names_a_bad_date_value_that_starts_with_a_dash() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb.cmd().args(["edit", "1", "-d", "-1d"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = stderr_of(&out);
    assert!(stderr.contains("'-1d'"), "{stderr}");
    assert!(!stderr.contains("without a value"), "{stderr}");
    assert_eq!(date_of(&db_tasks(&sb), 1), "2027-01-01");
}

/// REVIEW №5: a word with a comma after the id is parsed as more ids:
/// `edit 3 1,000 units sold` rewrites task 1.
#[test]
fn r2_edit_text_with_a_comma_number_is_not_an_id_list() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let _ = sb
        .cmd()
        .args(["edit", "3", "1,000", "units", "sold"])
        .output()
        .unwrap();
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "first task", "task 1 must not be touched");
    assert_eq!(text_of(&tasks, 3), "1,000 units sold");
}

/// REVIEW №76/№5 (addendum): a space after the comma (`1, 2, 3`) made the
/// trailing id the new text of the preceding tasks. The words are glued
/// back across the comma: one list, three tasks.
#[test]
fn r2_edit_id_list_with_spaces_does_not_clobber_texts() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb
        .cmd()
        .args(["edit", "1,", "2,", "3", "-d", "5-5-2027"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    for id in 1..=3 {
        assert_eq!(text_of(&tasks, id), ["first", "second", "third"][id as usize - 1].to_string() + " task");
        assert_eq!(date_of(&tasks, id), "2027-05-05");
    }
}

/// REVIEW №76: a second word after the id list was dropped without a word
/// (`mark 1 2` marked task 1 only; `mark 3 1,2` dropped the 3). It is an
/// error now, and nothing is marked.
#[test]
fn r2_mark_extra_word_after_the_ids_is_an_error() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    for argv in [vec!["mark", "1", "2"], vec!["mark", "3", "1,2"]] {
        let out = sb.cmd().args(&argv).output().unwrap();
        assert!(!out.status.success(), "{argv:?} must fail");
        let stderr = stderr_of(&out);
        assert!(stderr.contains(&format!("'{}'", argv[2])), "{argv:?}: {stderr}");
        assert!(stderr.contains("1,2,3"), "the error shows the form to use: {stderr}");
    }
    let tasks = db_tasks(&sb);
    assert!((1..=3).all(|id| !done_of(&tasks, id)), "nothing may be marked");
}

/// REVIEW №73: a part of a list that is not an id was dropped silently
/// (`mark 1,abc` marked 1; `add x -a 2,abc` stored `(2)`).
#[test]
fn r2_invalid_list_part_is_an_error() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb.cmd().args(["mark", "1,abc"]).output().unwrap();
    assert!(!out.status.success());
    assert!(stderr_of(&out).contains("'abc'"), "{}", stderr_of(&out));
    assert!(!done_of(&db_tasks(&sb), 1));

    let out = sb.cmd().args(["add", "x", "-a", "2,abc"]).output().unwrap();
    assert!(!out.status.success());
    assert!(stderr_of(&out).contains("'abc'"), "{}", stderr_of(&out));
    assert_eq!(db_tasks(&sb).len(), 3, "nothing may be added");

    let out = sb.cmd().args(["edit", "1,4294967296", "zzz"]).output().unwrap();
    assert!(!out.status.success());
    assert_eq!(text_of(&db_tasks(&sb), 1), "first task");
}

/// `mark 1, 2, 3` (a space after each comma) is the same list.
#[test]
fn r2_mark_glues_a_list_split_by_the_shell() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb.cmd().args(["mark", "1,", "2,", "3"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    assert!((1..=3).all(|id| done_of(&tasks, id)), "{tasks:?}");
}

/// REVIEW №44: duplicate ids are applied once per occurrence — `mark 1,1`
/// toggles twice and leaves the task undone.
#[test]
fn r2_duplicate_ids_are_applied_once() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb.cmd().args(["mark", "1,1"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    let t1 = tasks.iter().find(|t| t["id"] == 1).unwrap();
    assert_eq!(t1["done"], true, "`mark 1,1` must mark task 1 done exactly once");
}

/// REVIEW №44: `edit 1,1 -d +1w` shifted the date twice.
#[test]
fn r2_duplicate_ids_shift_a_relative_date_once() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb.cmd().args(["edit", "1,1", "-d", "+1w"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(date_of(&db_tasks(&sb), 1), "2027-01-08");
}

/// REVIEW №77: a repeated `-d` silently takes the last value; `rusk add`
/// rejects the same input.
#[test]
fn r2_edit_rejects_a_repeated_date_flag() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb
        .cmd()
        .args(["edit", "2", "-d", "1d", "-d", "2d"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "two -d values are ambiguous and must be rejected like `rusk add` does"
    );
    assert!(db_tasks(&sb)[1]["date"].is_null(), "nothing may be written");
}

/// REVIEW №71: `del 1 -h` opened the delete prompt (`-h` was swallowed
/// as an id word); it prints the help.
#[test]
fn r2_del_help_after_an_id_prints_the_help() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = sb.cmd().args(["del", "1", "-h"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Delete tasks"));
    assert_eq!(db_tasks(&sb).len(), 3);
}

/// REVIEW №72: `--done` with ids did whatever the argument order made of
/// it; the two are exclusive.
#[test]
fn r2_del_done_and_ids_are_exclusive() {
    let sb = Sandbox::with_db(
        r#"[{"id":1,"text":"open","date":null,"done":false,"priority":false},
            {"id":2,"text":"closed","date":null,"done":true,"priority":false}]"#,
    );
    for argv in [vec!["del", "1", "--done"], vec!["del", "--done", "1"]] {
        let out = sb.cmd().args(&argv).output().unwrap();
        assert!(!out.status.success(), "{argv:?} must fail");
        assert!(stderr_of(&out).contains("--done"), "{}", stderr_of(&out));
    }
    assert_eq!(db_tasks(&sb).len(), 2, "nothing may be deleted");
}

/// REVIEW №75: `del abc` / `del` printed a notice to stdout and exited 0,
/// unlike `mark` / `edit`. One convention: stderr, exit 1.
#[test]
fn r2_del_without_valid_ids_fails_like_mark() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    for argv in [vec!["del"], vec!["del", "abc"], vec!["mark", "abc"], vec!["edit", "abc", "t"]] {
        let out = sb.cmd().args(&argv).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{argv:?}");
        assert!(out.stdout.is_empty(), "{argv:?}: {}", String::from_utf8_lossy(&out.stdout));
        assert!(!out.stderr.is_empty(), "{argv:?}");
    }
    assert_eq!(db_tasks(&sb).len(), 3);
}

/// REVIEW №76/№5 (addendum): `edit 1,2 3 -d 2w` — the user forgot a comma —
/// set "3" as the text of tasks 1 and 2. Numbers alone after the ids are
/// not taken as the text; after `--` they are.
#[test]
fn r2_edit_numbers_after_the_ids_are_not_the_text() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    for argv in [
        vec!["edit", "1,2", "3", "-d", "5-5-2027"],
        vec!["edit", "1", "2", "3", "-d", "5-5-2027"],
        vec!["edit", "1", "2"],
    ] {
        let out = sb.cmd().args(&argv).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{argv:?}");
        let stderr = stderr_of(&out);
        assert!(stderr.contains("more task ids"), "{argv:?}: {stderr}");
        assert!(stderr.contains(" -- "), "the way to set such a text is named: {stderr}");
    }
    assert_eq!(sb.read_db().trim(), THREE_TASKS_DB.trim(), "nothing may be written");

    let out = sb.cmd().args(["edit", "1", "--", "42"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(text_of(&db_tasks(&sb), 1), "42");
}

/// Argument errors come before the database is opened: an unreadable (or
/// remote) database must not hide them. The same errors as with a good
/// database, and the broken file is left alone.
#[test]
fn r2_arguments_are_checked_before_the_database_is_opened() {
    let sb = Sandbox::new();
    sb.write_db("garbage{");
    for (argv, expected) in [
        (vec!["mark", "1", "2"], "unexpected argument '2'"),
        (vec!["mark", "abc"], "'abc' is not a task id"),
        (vec!["mark"], "no task ids given"),
        (vec!["del", "1", "2"], "unexpected argument '2'"),
        (vec!["edit", "abc", "t"], "'abc' is not a task id"),
        (vec!["edit", "1,2", "3"], "more task ids"),
        (vec!["edit", "1", "-a", "abc"], "`--after` expects"),
        (vec!["add", "x", "-a", "abc"], "`--after` expects"),
    ] {
        let out = sb.cmd().args(&argv).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{argv:?}");
        let stderr = stderr_of(&out);
        assert!(stderr.contains(expected), "{argv:?}: {stderr}");
        assert!(!stderr.contains("database"), "{argv:?} opened the database: {stderr}");
    }
    assert_eq!(sb.read_db(), "garbage{");
}

/// REVIEW №43: `edit 1 ''` stored an empty text, which `add` refuses.
#[test]
fn r2_edit_refuses_an_empty_text() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    for argv in [vec!["edit", "1", ""], vec!["edit", "1", "   "], vec!["edit", "1", "-d", "2w", ""]] {
        let out = sb.cmd().args(&argv).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{argv:?}");
        assert!(stderr_of(&out).contains("cannot be empty"), "{argv:?}: {}", stderr_of(&out));
    }
    assert_eq!(sb.read_db().trim(), THREE_TASKS_DB.trim());
}

/// REVIEW №77: `-a` takes values that start with `-` like `-d` does: `-a -h`
/// is the help (it was "a value is required"), `-a -5` names the value
/// (clap's tip `-- -5` led to the same "a value is required").
#[test]
fn r2_after_option_takes_dash_values_like_date() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    for (argv, about) in [
        (vec!["edit", "1", "-a", "-h"], "Edit tasks by ID"),
        (vec!["add", "x", "--after", "--help"], "Add a new task"),
    ] {
        let out = sb.cmd().args(&argv).output().unwrap();
        assert!(out.status.success(), "{argv:?}: {}", stderr_of(&out));
        assert!(String::from_utf8_lossy(&out.stdout).contains(about), "{argv:?}");
    }
    let out = sb.cmd().args(["edit", "1", "-a", "-5"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr_of(&out).contains("'-5' is not a task id"), "{}", stderr_of(&out));
    assert_eq!(sb.read_db().trim(), THREE_TASKS_DB.trim());
}

// Shell completion after R2. `-d` / `-a` are real options now, so a task
// text that `rusk edit <id><TAB>` inserts and that starts with `-` would be
// read as options (`-d 2d` moved the date); it goes after `--`. And
// `del --done` takes no ids, so it is not offered after one. Each test
// drives one shell (without its user config) with a fake `rusk` first on
// PATH, and is skipped when that shell is not installed.

#[cfg(unix)]
const COMPLETIONS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/completions");

/// A directory with a fake `rusk` that lists task 1 `-x means exclude` and
/// task 2 `plain text` for completion, and the PATH to use with it.
#[cfg(unix)]
fn fake_rusk_on_path() -> (tempfile::TempDir, String) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let rusk = dir.path().join("rusk");
    fs::write(
        &rusk,
        "#!/bin/sh\n\
         [ \"$1 $2\" = 'list --for-completion-lines' ] && printf '1\\t-x means exclude\\n2\\tplain text\\n'\n\
         exit 0\n",
    )
    .unwrap();
    fs::set_permissions(&rusk, fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", dir.path().display(), std::env::var("PATH").unwrap_or_default());
    (dir, path)
}

/// Runs `shell` with `args`; `None` when the shell is not installed.
#[cfg(unix)]
fn run_shell(shell: &str, args: &[&str], path: &str) -> Option<String> {
    match std::process::Command::new(shell).args(args).env("PATH", path).output() {
        Ok(out) => {
            assert!(out.status.success(), "{shell} failed: {}", stderr_of(&out));
            Some(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("{shell} is not installed: skipped");
            None
        }
        Err(e) => panic!("{shell}: {e}"),
    }
}

/// Four lines from a shell: what `rusk edit 1<TAB>` and `rusk edit 2<TAB>`
/// put on the command line after `rusk edit `, then the candidates for
/// `rusk del <TAB>` and `rusk del 1 <TAB>` joined with `|`.
#[cfg(unix)]
fn assert_r2_completions(shell: &str, out: Option<String>) {
    let Some(out) = out else { return };
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 4, "{shell}: {out}");
    assert_eq!(lines[0], "1 -- '-x means exclude'", "{shell}");
    assert_eq!(lines[1], "2 plain text", "{shell}");
    assert!(lines[2].split('|').any(|c| c == "--done"), "{shell}: {}", lines[2]);
    assert!(!lines[3].split('|').any(|c| c == "--done"), "{shell}: {}", lines[3]);
}

#[test]
#[cfg(unix)]
fn r2_bash_completion_puts_a_dash_text_after_double_dash() {
    let (_dir, path) = fake_rusk_on_path();
    let script = format!(
        r#"source {COMPLETIONS}/rusk.bash
c() {{ COMP_WORDS=("$@"); COMP_CWORD=$((${{#COMP_WORDS[@]}} - 1)); COMP_LINE="${{COMP_WORDS[*]}}"; COMPREPLY=(); _rusk_completion; local IFS='|'; echo "${{COMPREPLY[*]}}"; }}
c rusk edit 1; c rusk edit 2; c rusk del ""; c rusk del 1 ""
"#
    );
    let out = run_shell("bash", &["--norc", "--noprofile", "-c", &script], &path);
    assert_r2_completions("bash", out);
}

#[test]
#[cfg(unix)]
fn r2_zsh_completion_puts_a_dash_text_after_double_dash() {
    let (_dir, path) = fake_rusk_on_path();
    // `compadd` works only inside the completion system: collect what the
    // script hands to it.
    let script = format!(
        r#"_RUSK_ZSH_SKIP_ENTRY=1 source {COMPLETIONS}/rusk.zsh
compadd() {{ local -a a=("$@"); reply+=("${{(@)a[${{a[(i)--]}}+1,-1]}}"); }}
c() {{ words=("$@"); CURRENT=${{#words}}; LBUFFER="${{words[*]}}"; reply=(); _rusk; print -r -- "${{(j:|:)reply}}"; }}
c rusk edit 1; c rusk edit 2; c rusk del ""; c rusk del 1 ""
"#
    );
    let out = run_shell("zsh", &["-f", "-c", &script], &path);
    assert_r2_completions("zsh", out);
}

/// fish inserts one token per Tab, escaped as it inserts it (REVIEW №63:
/// the Tab wrapper it had left a text that starts with `-` unquoted): after
/// the id comes `--`, then the text.
#[test]
#[cfg(unix)]
fn r2_fish_completion_puts_a_dash_text_after_double_dash() {
    let (_dir, path) = fake_rusk_on_path();
    let script = format!(
        r#"source {COMPLETIONS}/rusk.fish
for line in 'rusk edit 1 ' 'rusk edit 1 -- ' 'rusk edit 2 ' 'rusk del ' 'rusk del 1 '
    complete --escape -C $line | string replace -r '\t.*' '' | string join '|'
end
"#
    );
    let Some(out) = run_shell("fish", &["-N", "-c", &script], &path) else { return };
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[..3], ["--", "-x\\ means\\ exclude", "plain\\ text"], "{out}");
    assert!(lines[3].split('|').any(|c| c == "--done"), "{}", lines[3]);
    assert!(!lines[4].split('|').any(|c| c == "--done"), "{}", lines[4]);
}

#[test]
#[cfg(unix)]
fn r2_nu_completion_puts_a_dash_text_after_double_dash() {
    let (_dir, path) = fake_rusk_on_path();
    let script = format!(
        r#"use {COMPLETIONS}/rusk.nu *
print (rusk-completions-main [rusk edit 1] | get value | str join "|")
print (rusk-completions-main [rusk edit 2] | get value | str join "|")
print (rusk-completions-main [rusk del ""] | get value | str join "|")
print (rusk-completions-main [rusk del 1 ""] | get value | str join "|")
"#
    );
    let out = run_shell("nu", &["-n", "-c", &script], &path);
    assert_r2_completions("nu", out);
}

#[test]
#[cfg(unix)]
fn r2_powershell_completion_puts_a_dash_text_after_double_dash() {
    let (_dir, path) = fake_rusk_on_path();
    let script = format!(
        r#". {COMPLETIONS}/rusk.ps1
foreach ($line in @('rusk edit 1', 'rusk edit 2', 'rusk del ', 'rusk del 1 ')) {{
    $r = TabExpansion2 -inputScript $line -cursorColumn $line.Length
    (@($r.CompletionMatches | ForEach-Object {{ $_.CompletionText }})) -join '|'
}}
"#
    );
    let out = run_shell("pwsh", &["-NoProfile", "-NonInteractive", "-Command", &script], &path);
    assert_r2_completions("pwsh", out);
}

// ---------------------------------------------------------------------------
// R3 — one set of rules for every task list that is read
// ---------------------------------------------------------------------------
//
// Whatever a backend reads — a hand-edited file, a spreadsheet, a foreign
// calendar, a table another tool wrote — is made into a list rusk can hold
// before anything uses it: every task has a text, an id of its own, and
// dependencies only on other tasks that exist. `PUT /api/tasks` refuses a
// list that is not one already.

fn ids_and_texts(tasks: &[Task]) -> Vec<(u32, &str)> {
    tasks.iter().map(|t| (t.id, t.text.as_str())).collect()
}

/// REVIEW №14: tasks that share an id (a copied spreadsheet row, a merge)
/// or have id 0 were loaded as they were: `add` handed out an id that was
/// taken, and commands by id hit only the first task that had it.
#[test]
fn r3_tasks_that_share_an_id_get_ids_of_their_own() {
    let dir = tempfile::tempdir().unwrap();

    let json = dir.path().join("dup.json");
    fs::write(
        &json,
        r#"[{"id":1,"text":"a","date":null,"done":false},{"id":1,"text":"b","date":null,"done":false},{"id":2,"text":"c","date":null,"done":false}]"#,
    )
    .unwrap();
    let loaded = backend(&json).load().unwrap();
    assert_eq!(ids_and_texts(&loaded), [(1, "a"), (3, "b"), (2, "c")]);
    assert_eq!(backend(&json).load().unwrap(), loaded, "the same file, the same ids");

    let mut tm = TaskManager::open_at(json.clone()).unwrap();
    tm.add_task(vec!["brand-new".into()], None).unwrap();
    assert_eq!(tm.mark_tasks(vec![3]).unwrap(), (vec![(3, true)], vec![]));
    let on_disk = backend(&json).load().unwrap();
    assert_eq!(
        ids_and_texts(&on_disk),
        [(1, "a"), (3, "b"), (2, "c"), (4, "brand-new")]
    );
    assert!(on_disk[1].done && !on_disk[0].done, "mark 3 must hit b, and only b");

    let csv = dir.path().join("dup.csv");
    fs::write(
        &csv,
        "id,text,date,done,priority,after\n1,a,,false,false,\n1,b,,false,false,\n2,c,,false,false,\n",
    )
    .unwrap();
    TaskManager::open_at(csv.clone())
        .unwrap()
        .add_task(vec!["brand-new".into()], None)
        .unwrap();
    let on_disk = backend(&csv).load().unwrap();
    assert_eq!(
        ids_and_texts(&on_disk),
        [(1, "a"), (3, "b"), (2, "c"), (4, "brand-new")]
    );

    let zero = dir.path().join("zero.csv");
    fs::write(&zero, "id,text,date,done,priority,after\n0,z,,false,false,\n1,a,,false,false,\n")
        .unwrap();
    let mut tm = TaskManager::open_at(zero.clone()).unwrap();
    assert_eq!(ids_and_texts(tm.tasks()), [(2, "z"), (1, "a")]);
    tm.add_task(vec!["newtask".into()], None).unwrap();
    assert_eq!(
        ids_and_texts(&backend(&zero).load().unwrap()),
        [(2, "z"), (1, "a"), (3, "newtask")]
    );
}

/// What the user sees of it: the list shows the ids the next save writes,
/// stderr says which task got a new id, which has no text, what was
/// skipped and which dependency leads nowhere; once the file has been saved
/// there is nothing more to say.
#[test]
fn r3_a_repaired_list_is_announced_until_it_is_saved() {
    let sb = Sandbox::with_db(
        r#"[{"id":1,"text":"alpha"},{"id":1,"text":"beta"},{"id":2,"text":""},
            {"text":"  "},{"id":5,"text":"epsilon","after":[9]}]"#,
    );
    let out = sb.cmd().args(["list", "--for-completion-lines"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "1\talpha\n3\tbeta\n2\t(no text)\n5\tepsilon\n"
    );
    let stderr = stderr_of(&out);
    assert!(stderr.contains(r#""beta" (id 1) is task 3 now"#), "{stderr}");
    assert!(stderr.contains("task 2 in '") && stderr.contains("has no text"), "{stderr}");
    assert!(stderr.contains("an item with neither text nor id"), "{stderr}");
    assert!(stderr.contains("task 5 depends on id 9, which no task there was saved with"), "{stderr}");
    assert!(stderr.contains(&sb.db_path().display().to_string()), "{stderr}");
    assert_eq!(stderr.lines().count(), 4, "one line per repair: {stderr}");

    // The task without text is there to be given one.
    let out = sb.cmd().args(["edit", "2", "buy presents"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let out = sb.cmd().args(["add", "gamma"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Added task: 4"), "{out:?}");
    let tasks = db_tasks(&sb);
    let ids: Vec<u64> = tasks.iter().map(|t| t["id"].as_u64().unwrap()).collect();
    // Not in id order (the repair put 3 before 2): the new task goes at the
    // end, as in any list with an order of its own (REVIEW №133).
    assert_eq!(ids, [1, 3, 2, 5, 4]);
    assert_eq!(text_of(&tasks, 3), "beta");
    assert_eq!(text_of(&tasks, 2), "buy presents");

    let out = sb.cmd().arg("list").output().unwrap();
    assert!(out.status.success());
    assert_eq!(stderr_of(&out), "", "a saved list needs no repairs");
}

/// REVIEW №15: a dependency on a task that is not there (deleted by hand, a
/// typo) survived the load and came to mean whichever task got that id
/// next; one on the task itself, or listed twice, survived too.
#[test]
fn r3_dependencies_are_only_on_other_tasks_that_exist() {
    let dir = tempfile::tempdir().unwrap();

    #[cfg(feature = "fmt-markdown")]
    {
        let md = dir.path().join("m.md");
        fs::write(&md, "- [ ] blocked <!-- id:2 after:1 -->\n").unwrap();
        let mut tm = TaskManager::open_at(md.clone()).unwrap();
        assert!(tm.tasks()[0].after.is_empty());
        tm.add_task(vec!["unrelated new task".into()], None).unwrap();
        let on_disk = backend(&md).load().unwrap();
        // The new task takes id 1, in front of task 2 (REVIEW №133).
        assert_eq!(ids_and_texts(&on_disk), [(1, "unrelated new task"), (2, "blocked")]);
        assert!(on_disk[1].after.is_empty(), "the old dependency now means the new task");
        assert!(!fs::read_to_string(&md).unwrap().contains("after:"));

        let md = dir.path().join("self.md");
        fs::write(&md, "- [ ] a <!-- id:1 after:1,1,2 -->\n- [ ] b <!-- id:2 -->\n").unwrap();
        assert_eq!(backend(&md).load().unwrap()[0].after, [2]);
    }

    #[cfg(feature = "fmt-todotxt")]
    {
        let txt = dir.path().join("todo.txt");
        fs::write(&txt, "blocked after:1 id:2\n").unwrap();
        assert!(backend(&txt).load().unwrap()[0].after.is_empty());
    }

    let json = dir.path().join("deps.json");
    fs::write(
        &json,
        r#"[{"id":1,"text":"a","after":[1,2,2,7]},{"id":2,"text":"b","after":[5]}]"#,
    )
    .unwrap();
    let loaded = backend(&json).load().unwrap();
    assert_eq!(loaded[0].after, [2]);
    assert!(loaded[1].after.is_empty());

    let csv = dir.path().join("deps.csv");
    fs::write(&csv, "id,text,date,done,priority,after\n2,blocked,,false,false,1\n").unwrap();
    assert!(backend(&csv).load().unwrap()[0].after.is_empty());
}

/// A task whose id is handed out on load (no id, or a shared one) never
/// picks up a dependency the file meant for another task.
#[test]
#[cfg(feature = "fmt-markdown")]
fn r3_a_new_id_never_inherits_a_dangling_dependency() {
    let dir = tempfile::tempdir().unwrap();
    let md = dir.path().join("m.md");
    // Task 1 is gone; "typed by hand" has no id and gets the lowest free
    // one — 1 — in the same load.
    fs::write(&md, "- [ ] blocked <!-- id:2 after:1 -->\n- [ ] typed by hand\n").unwrap();
    let loaded = backend(&md).load().unwrap();
    assert_eq!(ids_and_texts(&loaded), [(2, "blocked"), (1, "typed by hand")]);
    assert!(loaded[0].after.is_empty(), "{loaded:?}");
}

/// REVIEW №80: a row typed into the spreadsheet without an id, a row of
/// empty cells, or a line of spaces stopped the whole CSV database from
/// loading.
#[test]
fn r3_csv_rows_without_an_id_are_new_tasks_and_blank_rows_are_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b.csv");
    fs::write(
        &path,
        "id,text,date,done,priority,after\r\n1,x,,false,false,\r\n,new row typed in sheet,,false,false,\r\n,,,,,\r\n \r\n",
    )
    .unwrap();
    let loaded = backend(&path).load().unwrap();
    assert_eq!(ids_and_texts(&loaded), [(1, "x"), (2, "new row typed in sheet")]);
}

/// REVIEW №82, №83: a bad id cell was reported as outside "(1-255)" — ids
/// go up to 4294967295 — at a row number that skipped blank lines and
/// counted a multi-line cell as one line.
#[test]
fn r3_csv_errors_name_the_valid_ids_and_the_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.csv");
    let error = |content: &str| {
        fs::write(&path, content).unwrap();
        format!("{:#}", backend(&path).load().unwrap_err())
    };

    let err = error("id,text,date,done,priority,after\r\nabc,x,,false,false,\r\n");
    assert!(err.contains("row 2: invalid id 'abc'"), "{err}");
    assert!(err.contains("4294967295") && !err.contains("1-255"), "{err}");

    let err = error("id,text,date,done,priority,after\n\n\nBAD,x,,false,false,\n");
    assert!(err.contains("row 4:"), "{err}");

    let err = error("id,text,date,done,priority,after\n1,\"two\nlines\",,false,false,\nBAD,x,,false,false,\n");
    assert!(err.contains("row 3 (line 4):"), "{err}");
}

/// REVIEW №81: the `after` cell took only spaces between ids, while the
/// CLI, the list and every other format use commas.
#[test]
fn r3_csv_after_cells_take_commas_too() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.csv");
    fs::write(
        &path,
        "id,text,date,done,priority,after\n1,a,,false,false,\n2,b,,false,false,\n3,c,,false,false,\"1,2\"\n4,d,,false,false,\"2, 1 3\"\n",
    )
    .unwrap();
    let loaded = backend(&path).load().unwrap();
    assert_eq!(loaded[2].after, [1, 2]);
    assert_eq!(loaded[3].after, [2, 1, 3]);
}

/// REVIEW №82: CSV and JSON loaded id 0 as it was (`mark 0` marked it),
/// while SQLite and `PUT /api/tasks` refuse it — such a database could not
/// be pushed to `rusk serve`. Id 0 is "no id" now: the task gets a free one.
#[test]
fn r3_id_zero_means_no_id() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("zero.json");
    fs::write(&path, r#"[{"id":0,"text":"zero"},{"id":1,"text":"one"}]"#).unwrap();
    let loaded = backend(&path).load().unwrap();
    assert_eq!(ids_and_texts(&loaded), [(2, "zero"), (1, "one")]);

    #[cfg(feature = "web")]
    {
        let mut server = TaskManager::new_empty_with_path(dir.path().join("server.json"));
        let body = serde_json::to_string(&loaded).unwrap();
        let res = rusk::web::api::replace_tasks(&mut server, &body, None);
        assert_eq!(res.status, 200, "{}", res.body);
    }
}

/// REVIEW №86: hand-written JSON needed every field — `"after": null` or a
/// missing `done` made the database "corrupted". Only the text is required
/// now; everything else may be left out or null, and a task without an id
/// gets one.
#[test]
fn r3_hand_written_json_needs_only_the_text() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    for (json, what) in [
        (
            r#"[{"id":1,"text":"a","date":null,"done":false,"priority":false,"after":null}]"#,
            "after: null",
        ),
        (r#"[{"id":1,"text":"a"}]"#, "no done"),
        (r#"[{"id":1,"text":"a","done":null,"priority":null}]"#, "null flags"),
        (r#"[{"text":"a"}]"#, "no id"),
        (r#"[{"id":null,"text":"a"}]"#, "null id"),
    ] {
        fs::write(&path, json).unwrap();
        let loaded = backend(&path)
            .load()
            .unwrap_or_else(|e| panic!("{what}: {e:#}"));
        assert_eq!(loaded, [task(1, "a")], "{what}");
    }

    #[cfg(feature = "fmt-ndjson")]
    {
        let path = dir.path().join("tasks.ndjson");
        fs::write(&path, "{\"text\":\"a\",\"after\":null}\n{\"id\":2,\"text\":\"b\"}\n").unwrap();
        assert_eq!(backend(&path).load().unwrap(), [task(1, "a"), task(2, "b")]);
    }

    // The text is what a task is: without the field the file does not load.
    fs::write(&path, r#"[{"id":1}]"#).unwrap();
    let err = format!("{:#}", backend(&path).load().unwrap_err());
    assert!(err.contains("missing field `text`"), "{err}");
}

/// REVIEW №87: items without text — an empty `- [ ]`, a todo.txt line of
/// metadata only, a VTODO without SUMMARY, `"text": ""` — were loaded as
/// tasks `rusk add` refuses to create, shown as empty rows and saved back.
/// An item with neither text nor id is markup left behind: no task.
#[test]
fn r3_items_with_neither_text_nor_id_are_not_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let loads = |name: &str, content: &str| {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        backend(&path).load().unwrap()
    };

    #[cfg(feature = "fmt-markdown")]
    {
        let md = "- [ ]\n- [x]   \n- [ ] real <!-- id:1 -->\n";
        assert_eq!(ids_and_texts(&loads("e.md", md)), [(1, "real")]);
        let path = dir.path().join("e.md");
        TaskManager::open_at(path.clone())
            .unwrap()
            .add_task(vec!["z".into()], None)
            .unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert_eq!(saved, "- [ ] real <!-- id:1 -->\n- [ ] z <!-- id:2 -->\n");
    }
    #[cfg(feature = "fmt-todotxt")]
    assert_eq!(
        ids_and_texts(&loads("todo.txt", "x (A)\nx 2026-01-01\n(B)\nreal id:2\n")),
        [(2, "real")]
    );
    #[cfg(feature = "fmt-ics")]
    assert_eq!(
        ids_and_texts(&loads(
            "t.ics",
            "BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:abc@elsewhere\nDUE;VALUE=DATE:20261001\nEND:VTODO\n\
             BEGIN:VTODO\nSUMMARY:real\nEND:VTODO\nEND:VCALENDAR\n"
        )),
        [(1, "real")]
    );
    assert_eq!(
        ids_and_texts(&loads("e.json", r#"[{"text":""},{"id":3,"text":"real"}]"#)),
        [(3, "real")]
    );
    assert_eq!(
        ids_and_texts(&loads(
            "e.csv",
            "id,text,date,done,priority,after\n,,,true,false,\n2,real,,false,false,\n"
        )),
        [(2, "real")]
    );
}

/// ...but one that has an id was stored as a task (an old `rusk edit 2 ''`,
/// REVIEW №43, a cleared cell, an item rusk wrote for a task without text):
/// it keeps its id, date, flags and dependents, shows as "(no text)" and
/// can be given a text or deleted like any other task.
#[test]
fn r3_a_task_stored_without_text_is_kept_where_commands_reach_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    fs::write(
        &path,
        r#"[{"id":1,"text":"alpha"},{"id":2,"text":"","date":"2026-12-24","priority":true},
            {"id":3,"text":"gift wrap","after":[2]}]"#,
    )
    .unwrap();
    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    let kept = &tm.tasks()[1];
    assert_eq!((kept.id, kept.text.as_str()), (2, "(no text)"));
    assert!(kept.priority && kept.date.is_some(), "{kept:?}");
    assert_eq!(tm.tasks()[2].after, [2], "its dependents keep depending on it");
    let (edited, _, not_found) = tm
        .edit_tasks(vec![2], Some(vec!["buy presents".into()]), None)
        .unwrap();
    assert_eq!((edited, not_found), (vec![2], vec![]));
    let on_disk = backend(&path).load().unwrap();
    assert_eq!(on_disk[1].text, "buy presents");
    assert!(on_disk[1].priority && on_disk[1].date.is_some());

    let loads = |name: &str, content: &str| {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        backend(&path).load().unwrap()
    };
    #[cfg(feature = "fmt-markdown")]
    assert_eq!(ids_and_texts(&loads("e.md", "- [x] <!-- id:3 -->\n")), [(3, "(no text)")]);
    #[cfg(feature = "fmt-todotxt")]
    {
        let tasks = loads("todo.txt", "x id:1\n(A) id:7\n");
        assert_eq!(ids_and_texts(&tasks), [(1, "(no text)"), (7, "(no text)")]);
        assert!(tasks[0].done && tasks[1].priority);
    }
    #[cfg(feature = "fmt-ics")]
    assert_eq!(
        ids_and_texts(&loads(
            "t.ics",
            "BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:rusk-4@rusk\nDUE;VALUE=DATE:20261001\nEND:VTODO\nEND:VCALENDAR\n"
        )),
        [(4, "(no text)")]
    );
    assert_eq!(
        ids_and_texts(&loads(
            "e.csv",
            "id,text,date,done,priority,after\n1,,,true,false,\n2,real,,false,false,\n"
        )),
        [(1, "(no text)"), (2, "real")]
    );
}

/// Review of R3: a todo.txt or Markdown text that looks like the markup
/// around it was written as it was and read back as something else — with
/// R3, as an item without text: `rusk add x` on a todo.txt database stored
/// `x id:1`, which reads as a done task without text. What rusk writes it
/// reads back, whatever the text (REVIEW №30 for todo.txt).
#[test]
fn r3_what_rusk_writes_it_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let formats = [
        "tasks.json",
        "tasks.csv",
        #[cfg(feature = "fmt-todotxt")]
        "todo.txt",
        #[cfg(feature = "fmt-markdown")]
        "tasks.md",
        #[cfg(feature = "fmt-ndjson")]
        "tasks.ndjson",
        #[cfg(feature = "fmt-ics")]
        "tasks.ics",
    ];
    let texts = [
        "x",
        "(A)",
        "2026-10-01",
        "due:2026-10-01",
        "id:5",
        "after:1",
        "x marks the spot",
        "ticket id:42 needs review",
        "@2026-01-01",
        " @2026-01-01",
    ];
    // Saved as they are: `rusk add` would trim " @2026-01-01" (REVIEW
    // №115), and it is the formats that are checked here.
    let tasks: Vec<Task> = (1..).zip(texts).map(|(id, text)| task(id, text)).collect();
    for name in formats {
        let path = dir.path().join(name);
        backend(&path).save(&tasks).unwrap();
        let back = backend(&path).load().unwrap();
        let expected: Vec<(u32, &str)> = (1..).zip(texts).collect();
        assert_eq!(ids_and_texts(&back), expected, "{name}: {}", fs::read_to_string(&path).unwrap());
    }
}

/// Review of R3: a CSV of blank rows only (a cleared sheet) decodes to no
/// tasks, and was taken for an empty database — `rusk restore` put such a
/// `.backup` in place of a real one.
#[test]
fn r3_a_csv_of_blank_rows_is_not_an_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r.csv");
    backend(&path).save(&[task(1, "keep me")]).unwrap();
    fs::write(dir.path().join("r.csv.backup"), ",,,,,\r\n,,,,,\r\n").unwrap();
    let err = format!("{:#}", backend(&path).restore_from_backup().unwrap_err());
    assert!(err.contains("no tasks were recognized in the backup"), "{err}");
    assert_eq!(ids_and_texts(&backend(&path).load().unwrap()), [(1, "keep me")]);
}

/// Over ssh the same rules hold: `rusk sync` takes a remote Markdown list
/// with an empty item and a dangling dependency as a clean list, and says
/// where the item was skipped.
#[test]
#[cfg(all(unix, feature = "sync", feature = "fmt-markdown"))]
fn r3_a_remote_list_is_held_to_the_same_rules() {
    let sb = Sandbox::new();
    let remote = sb.path().join("remote.md");
    fs::write(&remote, "- [ ]\n- [ ] blocked <!-- id:2 after:1 -->\n").unwrap();
    let out = sb
        .cmd_with_fake_ssh()
        .env("RUSK_SYNC_REMOTE", format!("u@h:{}", remote.display()))
        .args(["sync", "pull", "--force"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    assert_eq!(tasks.len(), 1, "{tasks:?}");
    assert_eq!(text_of(&tasks, 2), "blocked");
    assert!(tasks[0].get("after").is_none(), "{tasks:?}");
    let stderr = stderr_of(&out);
    assert!(stderr.contains("neither text nor id") && stderr.contains("remote.md"), "{stderr}");
    assert!(stderr.contains("task 2 depends on id 1, which no task there was saved with"), "{stderr}");
}

/// REVIEW №108: `PUT /api/tasks` checked only that ids were unique and
/// nonzero, so a list with an empty text or with dependencies on missing
/// tasks or on the task itself was stored as sent. PUT stores a list
/// exactly as sent (sync compares what it pushed with what it reads back),
/// so it refuses whatever a load would have to repair, and says what.
#[test]
#[cfg(feature = "web")]
fn r3_put_refuses_a_list_that_a_load_would_repair() {
    use rusk::web::api::replace_tasks;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let mut tm = TaskManager::new_empty_with_path(path.clone());
    for (body, says) in [
        (r#"[{"id":1,"text":"   "}]"#, "task 1 has no text"),
        (r#"[{"text":""}]"#, "a task has neither text nor id"),
        (r#"[{"id":1,"text":"a","after":[1]}]"#, "task 1 depends on itself"),
        (r#"[{"id":1,"text":"a","after":[99]}]"#, "task 1 depends on task 99, which does not exist"),
        (
            r#"[{"id":1,"text":"a","after":[2,2]},{"id":2,"text":"b"}]"#,
            "task 1 lists task 2 more than once",
        ),
        (r#"[{"id":0,"text":"a"}]"#, "without an id"),
        (r#"[{"text":"a"}]"#, "without an id"),
        (r#"[{"id":1,"text":"a"},{"id":1,"text":"b"}]"#, "unique"),
    ] {
        let res = replace_tasks(&mut tm, body, None);
        assert_eq!(res.status, 400, "{body}: {}", res.body);
        assert!(res.body.contains(says), "{body}: {}", res.body);
    }
    assert!(!path.exists(), "a refused list was written");

    let valid = r#"[{"id":2,"text":"b"},{"id":1,"text":"a","after":[2]}]"#;
    assert_eq!(replace_tasks(&mut tm, valid, None).status, 200);
    assert_eq!(ids_and_texts(&backend(&path).load().unwrap()), [(2, "b"), (1, "a")]);
}

/// REVIEW section 4 (SQLite): a table written by another tool, without
/// rusk's UNIQUE and CHECK constraints, loaded shared ids and id 0 as they
/// were, and a NULL in `done` — or in `id` — made the whole database
/// unreadable; a value that is no task named neither row nor task.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r3_a_foreign_sqlite_table_is_held_to_the_same_rules() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "CREATE TABLE tasks (pos INTEGER PRIMARY KEY, id INTEGER, text TEXT, date TEXT,
                             done INTEGER, priority INTEGER, \"after\" TEXT);
         INSERT INTO tasks (pos, id, text, done, priority, \"after\") VALUES
             (1, 1, 'a', 0, 0, NULL),
             (2, 1, 'b', NULL, NULL, '9,1'),
             (3, 0, 'z', 1, 0, '1'),
             (4, 5, '', 0, 0, NULL),
             (5, NULL, 'inserted without an id', 0, 0, NULL);",
    )
    .unwrap();
    let loaded = backend(&path).load().unwrap();
    // "b" shares id 1, "z" and the last row have none; row 4 is task 5
    // without text.
    assert_eq!(
        ids_and_texts(&loaded),
        [(1, "a"), (2, "b"), (3, "z"), (5, "(no text)"), (4, "inserted without an id")]
    );
    assert!(!loaded[1].done && !loaded[1].priority && loaded[2].done);
    // 9 does not exist and 1 is the id "b" was written with (its own).
    assert!(loaded[1].after.is_empty(), "{loaded:?}");
    assert_eq!(loaded[2].after, [1]);

    // What is saved is the repaired list.
    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    tm.add_task(vec!["new".into()], None).unwrap();
    assert_eq!(
        ids_and_texts(&backend(&path).load().unwrap()),
        [(1, "a"), (2, "b"), (3, "z"), (5, "(no text)"), (4, "inserted without an id"), (6, "new")]
    );

    // A value that is no task at all names the row it is in.
    conn.execute("UPDATE tasks SET done = 'yes' WHERE pos = 2", []).unwrap();
    let err = format!("{:#}", backend(&path).load().unwrap_err());
    assert!(err.contains("row 2 (id 2): Invalid column type Text"), "{err}");
}

// ---------------------------------------------------------------------------
// R5 — "cannot read" is not "zero tasks" (closed: regressions)
// ---------------------------------------------------------------------------
//
// Three answers, never folded into one: nothing is there (a database yet to
// be made), something is there that cannot be read (an error that stops the
// command), or a file that holds nothing (zero tasks, and a warning unless
// that is how the format writes an empty database).

/// REVIEW №17: a UTF-8 BOM (Windows editors) makes a valid JSON database
/// "corrupted".
#[test]
fn r5_json_with_a_bom_loads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    fs::write(
        &path,
        "\u{feff}[{\"id\":1,\"text\":\"bom\",\"date\":null,\"done\":false,\"priority\":false}]",
    )
    .unwrap();
    let tasks = backend(&path).load().expect("a BOM is not corruption");
    assert_eq!(tasks.len(), 1);
}

/// REVIEW №171: an inaccessible database directory (EACCES) reads as an
/// empty database — `No tasks`, exit 0 — and the next save fails or, worse,
/// succeeds somewhere else.
#[test]
#[cfg(unix)]
fn r5_unreadable_database_directory_is_an_error_not_an_empty_database() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let db_dir = dir.path().join("locked");
    fs::create_dir(&db_dir).unwrap();
    let path = db_dir.join("tasks.json");
    backend(&path).save(&[task(1, "precious")]).unwrap();

    fs::set_permissions(&db_dir, fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = fs::read(&path).is_ok();
    let loaded = backend(&path).load();
    fs::set_permissions(&db_dir, fs::Permissions::from_mode(0o755)).unwrap();

    if readable_anyway {
        eprintln!("skipping: running as a user that ignores file modes (root?)");
        return;
    }
    assert!(
        loaded.is_err(),
        "an unreadable database must be an error, got {loaded:?}"
    );
}

/// REVIEW №171, its `rusk restore` half: the backup was looked for with
/// the same `exists()`, so a directory nobody may enter answered "No
/// backup file found" about a backup that is right there.
#[test]
#[cfg(unix)]
fn r5_an_unreadable_directory_is_not_a_missing_backup() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let db_dir = dir.path().join("locked");
    fs::create_dir(&db_dir).unwrap();
    let path = db_dir.join("tasks.json");
    let b = backend(&path);
    b.save(&[task(1, "precious")]).unwrap();
    // A second save leaves the first state as `tasks.json.backup`.
    b.save(&[task(1, "precious"), task(2, "and more")]).unwrap();

    fs::set_permissions(&db_dir, fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = fs::read(&path).is_ok();
    let restored = backend(&path).restore_from_backup();
    fs::set_permissions(&db_dir, fs::Permissions::from_mode(0o755)).unwrap();

    if readable_anyway {
        eprintln!("skipping: running as a user that ignores file modes (root?)");
        return;
    }
    let err = chain_of(&restored.unwrap_err());
    assert!(
        !err.contains("No backup file found"),
        "the backup is there, it just cannot be reached: {err}"
    );
    assert!(err.contains("locked"), "{err}");
}

/// REVIEW №19, the read half of the dangling link: a link whose target is
/// gone (an unmounted disk) read as "no database yet", and the next save
/// then created the target — a database the user could not see.
#[test]
#[cfg(unix)]
fn r5_a_dangling_symlink_is_not_an_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("tasks.json");
    std::os::unix::fs::symlink(dir.path().join("unmounted/tasks.json"), &link).unwrap();

    let err = chain_of(&backend(&link).load().unwrap_err());

    assert!(err.contains("symbolic link"), "{err}");
    assert!(err.contains("tasks.json"), "{err}");
}

/// The same for a SQLite database: `try_exists` follows the link and says
/// "nothing there" about a link that is very much there.
#[test]
#[cfg(all(unix, feature = "backend-sqlite"))]
fn r5_a_dangling_symlink_is_not_an_empty_sqlite_database() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("tasks.db");
    std::os::unix::fs::symlink(dir.path().join("unmounted/tasks.db"), &link).unwrap();

    let err = chain_of(&backend(&link).load().unwrap_err());

    assert!(err.contains("symbolic link"), "{err}");
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
}

/// REVIEW №17, the second half: whether an empty file is a database was
/// answered differently by every format (JSON: exit 1 "corrupted"; CSV:
/// "No tasks") and again differently over ssh. One answer now — no tasks,
/// and a warning that the file is not the empty database its format writes
/// — so nothing silently treats a half-finished write as "all deleted".
#[test]
fn r5_an_empty_file_reads_as_no_tasks_whatever_the_format() {
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in [
        ("tasks.json", ""),
        ("tasks.json", "  \n"),
        ("tasks.csv", ""),
        #[cfg(feature = "fmt-ndjson")]
        ("tasks.ndjson", ""),
        #[cfg(feature = "fmt-ics")]
        ("tasks.ics", ""),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        let tasks = backend(&path)
            .load()
            .unwrap_or_else(|e| panic!("{name} ({content:?}): {e:#}"));
        assert!(tasks.is_empty(), "{name}");
    }
}

/// Content that is not UTF-8 is unreadable, not "readable, slightly
/// different": the ssh backend used to decode it lossily, so a save wrote
/// U+FFFD over the bytes it could not read. The local backend always
/// refused; both do now.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r5_remote_content_that_is_not_utf8_is_not_repaired_into_the_file() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let remote_dir = sb.path().join("remote");
    fs::create_dir_all(&remote_dir).unwrap();
    let remote_path = remote_dir.join("tasks.json");
    let latin1 = b"[{\"id\":1,\"text\":\"caf\xe9\",\"date\":null,\"done\":false,\"priority\":false}]";
    fs::write(&remote_path, latin1).unwrap();

    let out = sb
        .cmd_with_fake_ssh()
        .env(
            "RUSK_SYNC_REMOTE",
            format!("user@host:{}", remote_path.display()),
        )
        .arg("sync")
        .output()
        .unwrap();

    let stderr = stderr_of(&out);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("UTF-8"), "{stderr}");
    assert_eq!(fs::read(&remote_path).unwrap(), latin1, "the bytes changed");
}

/// REVIEW №171 on the remote side: `test -e` answers "no" for a file in a
/// directory nobody may search, which would have made a remote database
/// that is right there read as one that was never created.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r5_an_unreadable_remote_directory_is_not_a_missing_database() {
    use std::os::unix::fs::PermissionsExt;
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let locked = sb.path().join("locked");
    fs::create_dir_all(&locked).unwrap();
    let remote_path = locked.join("tasks.json");
    fs::write(&remote_path, THREE_TASKS_DB).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = fs::read(&remote_path).is_ok();

    let out = sb
        .cmd_with_fake_ssh()
        .env(
            "RUSK_SYNC_REMOTE",
            format!("user@host:{}", remote_path.display()),
        )
        .arg("sync")
        .output()
        .unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

    if readable_anyway {
        eprintln!("skipping: running as a user that ignores file modes (root?)");
        return;
    }
    let stderr = stderr_of(&out);
    assert!(!out.status.success(), "{stderr}");
    assert!(!stderr.contains("does not exist"), "{stderr}");
    assert!(stderr.contains("failed to load tasks from"), "{stderr}");
}

/// A calendar of appointments holds no task, and rusk's own save would
/// replace all of it: that is a warning, not "an empty iCalendar database".
/// So is a write that stopped after the calendar header.
#[test]
#[cfg(feature = "fmt-ics")]
fn r5_a_foreign_calendar_is_no_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in [
        (
            "appointments.ics",
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:e1\r\n\
             SUMMARY:Standup\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
        ),
        ("truncated.ics", "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n"),
    ] {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        let tasks = backend(&path).load().unwrap();
        assert!(tasks.is_empty(), "{name}");
        assert!(
            !rusk::codec::DbFormat::Ics.is_empty_database(content),
            "{name} passed as an empty database, so nothing warns before it is replaced"
        );
    }
    // What rusk writes for an empty database still is one.
    let empty = rusk::codec::DbFormat::Ics.encode(&[]).unwrap();
    assert!(rusk::codec::DbFormat::Ics.is_empty_database(&empty));
}

/// The database becomes unreadable between the load and the save: the
/// compare-and-swap must fail, not take the file for missing and replace
/// it. (The load-bearing property of the new read.)
#[test]
#[cfg(unix)]
fn r5_a_database_that_turns_unreadable_fails_the_save() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let db_dir = dir.path().join("locked");
    fs::create_dir(&db_dir).unwrap();
    let path = db_dir.join("tasks.json");
    let b = backend(&path);
    b.save(&[task(1, "precious")]).unwrap();
    let before = fs::read(&path).unwrap();

    b.load().unwrap();
    fs::set_permissions(&db_dir, fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = fs::read(&path).is_ok();
    let saved = b.save(&[task(1, "precious"), task(2, "new")]);
    fs::set_permissions(&db_dir, fs::Permissions::from_mode(0o755)).unwrap();

    if readable_anyway {
        eprintln!("skipping: running as a user that ignores file modes (root?)");
        return;
    }
    assert!(saved.is_err(), "the save went through blind");
    assert_eq!(fs::read(&path).unwrap(), before);
}

/// REVIEW №6: after one successful sync the local file was removed (a lost
/// disk, a botched restore). The remote was emptied to match, without a
/// question. It is refused now — and the message says which of the two
/// reasons for "no tasks" it is.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r5_sync_says_whether_a_side_is_missing_or_empty() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let remote_path = sb.path().join("remote").join("tasks.json");
    let remote = format!("user@host:{}", remote_path.display());
    let sync = || {
        sb.cmd_with_fake_ssh()
            .env("RUSK_SYNC_REMOTE", &remote)
            .arg("sync")
            .output()
            .unwrap()
    };

    // First sync seeds a remote that does not exist yet.
    let out = sync();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(remote_path.exists(), "the remote file was not created");

    // The local database is gone: not an instruction to empty the remote.
    fs::remove_file(sb.db_path()).unwrap();
    let out = sync();
    let stderr = stderr_of(&out);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("does not exist"), "{stderr}");
    assert!(stderr.contains("has 3 task(s)"), "{stderr}");
    assert!(
        fs::read_to_string(&remote_path).unwrap().contains("first task"),
        "the remote was emptied"
    );

    // The user really deleted every task: same refusal, different reason.
    sb.write_db("[]");
    let out = sync();
    let stderr = stderr_of(&out);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("has no tasks"), "{stderr}");
}

/// REVIEW №6/№7: over ssh, "there is no file" and "the file cannot be
/// read" both came back as empty output. A remote file nobody may read
/// must not load as an empty database — the next save would replace it.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r5_an_unreadable_remote_file_is_not_a_missing_one() {
    use std::os::unix::fs::PermissionsExt;
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let remote_dir = sb.path().join("remote");
    fs::create_dir_all(&remote_dir).unwrap();
    let remote_path = remote_dir.join("tasks.json");
    fs::write(&remote_path, THREE_TASKS_DB).unwrap();
    fs::set_permissions(&remote_path, fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = fs::read(&remote_path).is_ok();

    let out = sb
        .cmd_with_fake_ssh()
        .env(
            "RUSK_SYNC_REMOTE",
            format!("user@host:{}", remote_path.display()),
        )
        .arg("sync")
        .output()
        .unwrap();
    fs::set_permissions(&remote_path, fs::Permissions::from_mode(0o644)).unwrap();

    if readable_anyway {
        eprintln!("skipping: running as a user that ignores file modes (root?)");
        return;
    }
    let stderr = stderr_of(&out);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("failed to load tasks from"), "{stderr}");
    // Unchanged: nothing was pushed over what could not be read.
    assert_eq!(fs::read_to_string(&remote_path).unwrap(), THREE_TASKS_DB);

    // A link whose target is gone is not "no file yet" either: `-e` says
    // no about the target while the link itself is there to be replaced.
    let link = remote_dir.join("dangling.json");
    std::os::unix::fs::symlink(remote_dir.join("unmounted/tasks.json"), &link).unwrap();
    let out = sb
        .cmd_with_fake_ssh()
        .env("RUSK_SYNC_REMOTE", format!("user@host:{}", link.display()))
        .arg("sync")
        .output()
        .unwrap();
    let stderr = stderr_of(&out);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("failed to load tasks from"), "{stderr}");
    assert!(
        fs::symlink_metadata(&link).unwrap().file_type().is_symlink(),
        "the link was replaced"
    );
}

// ---------------------------------------------------------------------------
// R9 — restore (closed: regressions)
// ---------------------------------------------------------------------------

/// Names of the files in `dir`, sorted.
/// What a directory holds, to catch leftovers (temp files, stray copies).
/// The writer lock (`<db>.lock`, R12) is no leftover: it is an empty file
/// that stays next to a database once it has been written.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| !name.ends_with(".lock"))
        .collect();
    names.sort();
    names
}

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// REVIEW №55: the second `restore` overwrites the only `.before_restore`,
/// so the state saved by the first restore is gone for good.
#[test]
fn r9_second_restore_keeps_the_first_before_restore_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let b = backend(&path);
    b.save(&[task(1, "state-one")]).unwrap();
    b.save(&[task(1, "state-two-UNIQUE")]).unwrap();
    b.restore_from_backup().unwrap(); // before_restore = state-two
    b.save(&[task(1, "state-three")]).unwrap();
    b.restore_from_backup().unwrap(); // must not destroy state-two

    let survives = fs::read_dir(dir.path()).unwrap().any(|entry| {
        fs::read_to_string(entry.unwrap().path())
            .map(|content| content.contains("state-two-UNIQUE"))
            .unwrap_or(false)
    });
    assert!(survives, "the state saved by the first restore was overwritten");
}

/// REVIEW №56: for the text formats a garbage `.backup` decodes to zero
/// tasks and `restore` wipes a healthy database with it.
#[test]
#[cfg(feature = "fmt-markdown")]
fn r9_restore_refuses_a_garbage_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.md");
    let b = backend(&path);
    b.save(&[task(1, "healthy one")]).unwrap();
    b.save(&[task(1, "healthy one"), task(2, "healthy two")]).unwrap();
    fs::write(dir.path().join("tasks.md.backup"), b"\x01\x02 definitely not a task list").unwrap();

    let _ = b.restore_from_backup();

    let after = backend(&path).load().unwrap();
    assert_eq!(after.len(), 2, "a healthy database was replaced by garbage");
}

/// REVIEW №157: restore copies the backup over the live database in place
/// (`fs::copy` = truncate + write); a crash in between leaves a truncated
/// database. An atomic replace creates a new inode, so a hard link to the
/// old file keeps the old content.
#[test]
#[cfg(unix)]
fn r9_restore_replaces_the_database_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let b = backend(&path);
    b.save(&[task(1, "state-one")]).unwrap();
    b.save(&[task(1, "state-two")]).unwrap();

    let witness = dir.path().join("witness");
    fs::hard_link(&path, &witness).unwrap();
    b.restore_from_backup().unwrap();

    assert!(fs::read_to_string(&path).unwrap().contains("state-one"));
    assert!(
        fs::read_to_string(&witness).unwrap().contains("state-two"),
        "the live file was rewritten in place instead of being replaced by rename"
    );
}

/// REVIEW №55: the copies are numbered, the newest gets the highest number,
/// and `restore` says where the current state went.
#[test]
fn r9_before_restore_copies_are_numbered() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let b = backend(&path);
    for round in 0..3 {
        b.save(&[task(1, &format!("kept-{round}"))]).unwrap();
        b.save(&[task(1, &format!("replaced-{round}"))]).unwrap();
        b.restore_from_backup().unwrap();
    }

    assert_eq!(
        names_in(dir.path()),
        [
            "tasks.json",
            "tasks.json.backup",
            "tasks.json.before_restore",
            "tasks.json.before_restore.1",
            "tasks.json.before_restore.2",
        ]
    );
    for (round, name) in ["", ".1", ".2"].iter().enumerate() {
        let kept = fs::read_to_string(dir.path().join(format!("tasks.json.before_restore{name}")))
            .unwrap();
        assert!(kept.contains(&format!("replaced-{round}")), "{name}: {kept}");
    }
}

/// A restore that would change nothing keeps no copy (repeated `rusk
/// restore` must not pile up identical files) and leaves `.backup` alone.
#[test]
fn r9_restore_that_changes_nothing_keeps_no_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let b = backend(&path);
    b.save(&[task(1, "state-one")]).unwrap();
    b.save(&[task(1, "state-two")]).unwrap();
    let backup_before = fs::read(dir.path().join("tasks.json.backup")).unwrap();

    b.restore_from_backup().unwrap();
    let restored = b.restore_from_backup().unwrap();

    assert_eq!(restored[0].text, "state-one");
    assert_eq!(
        names_in(dir.path()),
        ["tasks.json", "tasks.json.backup", "tasks.json.before_restore"]
    );
    assert_eq!(
        fs::read(dir.path().join("tasks.json.backup")).unwrap(),
        backup_before,
        "restore must not rewrite the backup it restores from"
    );
}

/// REVIEW №56: a zero-length backup is never a state rusk saved (an empty
/// database is not backed up at all, CSV and iCalendar are never empty
/// files) — it is what an interrupted copy leaves behind, and for Markdown,
/// todo.txt and NDJSON it would decode to "zero tasks" without complaint.
#[test]
fn r9_restore_refuses_a_zero_length_backup_in_every_format() {
    #[allow(unused_mut)] // no optional format may be compiled in
    let mut names = vec!["tasks.json", "tasks.csv"];
    #[cfg(feature = "fmt-markdown")]
    names.push("tasks.md");
    #[cfg(feature = "fmt-todotxt")]
    names.push("tasks.txt");
    #[cfg(feature = "fmt-ndjson")]
    names.push("tasks.ndjson");
    #[cfg(feature = "fmt-ics")]
    names.push("tasks.ics");

    for name in names {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        let b = backend(&path);
        b.save(&[task(1, "healthy one"), task(2, "healthy two")]).unwrap();
        fs::write(dir.path().join(format!("{name}.backup")), b"").unwrap();

        assert!(b.restore_from_backup().is_err(), "{name}: an empty file was restored");
        assert_eq!(backend(&path).load().unwrap().len(), 2, "{name}");
        assert_eq!(names_in(dir.path()), [name.to_string(), format!("{name}.backup")]);
    }
}

/// The other side of №56: a backup that holds a real empty database (a CSV
/// header without rows here) is a state like any other.
#[test]
fn r9_restore_accepts_a_real_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.csv");
    let b = backend(&path);
    b.save(&[]).unwrap();
    b.save(&[task(1, "added by mistake")]).unwrap();

    let restored = b.restore_from_backup().unwrap();

    assert!(restored.is_empty());
    assert!(backend(&path).load().unwrap().is_empty());
}

/// `restore` puts the backup file itself back, not a re-encoding of the
/// tasks in it: what the format does not model (prose around a Markdown
/// list) exists only in the backup once rusk has saved over it.
#[test]
#[cfg(feature = "fmt-markdown")]
fn r9_restore_brings_the_backup_back_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.md");
    let original = "# Groceries\n\nbuy before friday:\n\n- [ ] milk <!-- id:1 -->\n";
    fs::write(&path, original).unwrap();
    let b = backend(&path);
    let mut tasks = b.load().unwrap();
    tasks.push(task(2, "bread"));
    b.save(&tasks).unwrap(); // the prose is dropped; .backup still has it

    b.restore_from_backup().unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}

/// REVIEW №56, iCalendar: text that is not a calendar decodes to zero
/// VTODOs; a calendar without VTODOs is a real empty database.
#[test]
#[cfg(feature = "fmt-ics")]
fn r9_restore_tells_a_garbage_ics_backup_from_an_empty_calendar() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.ics");
    let backup = dir.path().join("tasks.ics.backup");
    let b = backend(&path);
    b.save(&[task(1, "healthy")]).unwrap();

    fs::write(&backup, "this is not a task list\n").unwrap();
    let err = b.restore_from_backup().unwrap_err().to_string();
    assert!(err.contains("no tasks were recognized"), "{err}");
    assert_eq!(backend(&path).load().unwrap().len(), 1);

    fs::write(&backup, "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n").unwrap();
    assert!(b.restore_from_backup().unwrap().is_empty());
}

/// The current database is never replaced unless a copy of it could be
/// kept first.
#[test]
#[cfg(unix)]
fn r9_restore_does_nothing_when_the_current_database_cannot_be_kept() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let b = backend(&path);
    b.save(&[task(1, "state-one")]).unwrap();
    b.save(&[task(1, "state-two")]).unwrap();

    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read(&path).is_ok() {
        eprintln!("skipping: running as a user that ignores file modes (root?)");
        return;
    }
    let result = b.restore_from_backup();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

    let err = result.unwrap_err().to_string();
    assert!(err.contains("nothing was restored"), "{err}");
    assert!(fs::read_to_string(&path).unwrap().contains("state-two"));
    assert_eq!(names_in(dir.path()), ["tasks.json", "tasks.json.backup"]);
}

#[cfg(feature = "backend-git")]
fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// REVIEW №68: with `git_backend` a restore is a commit of its own instead
/// of an uncommitted change that melts into the next save.
#[test]
#[cfg(feature = "backend-git")]
fn r9_restore_is_committed_with_git_backend() {
    if !git_available() {
        eprintln!("skipping: git is not installed");
        return;
    }
    let sb = Sandbox::new();
    let config = sb.path().join("cfg");
    fs::write(&config, "git_backend = true\n").unwrap();
    let rusk = |args: &[&str]| {
        let out = sb.cmd().env("RUSK_CONFIG", &config).args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", stderr_of(&out));
    };
    rusk(&["add", "first"]);
    rusk(&["add", "second"]);
    rusk(&["restore"]);

    let db_dir = sb.db_path().parent().unwrap().to_path_buf();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&db_dir)
            .args(args)
            .output()
            .unwrap();
        stdout_of(&out)
    };
    assert_eq!(
        git(&["status", "--short", "--", "tasks.json"]),
        "",
        "the restored database was left uncommitted"
    );
    assert_eq!(
        git(&["log", "-1", "--format=%s"]).trim(),
        "rusk: restore tasks.json from backup (1 tasks)"
    );
}

/// REVIEW №173: with `backup = false` the `.backup` is whatever an old save
/// left behind; `restore` says so and shows when the backup was saved.
#[test]
fn r9_restore_warns_when_backups_are_disabled() {
    let sb = Sandbox::new();
    let run = |cmd: &mut std::process::Command| {
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
        out
    };
    run(sb.cmd().args(["add", "old one"]));
    run(sb.cmd().args(["add", "old two"])); // .backup = [old one]

    let config = sb.path().join("cfg");
    fs::write(&config, "backup = false\n").unwrap();
    run(sb.cmd().env("RUSK_CONFIG", &config).args(["add", "new work"]));
    let out = run(sb.cmd().env("RUSK_CONFIG", &config).args(["restore"]));

    assert!(stderr_of(&out).contains("backup = false"), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("(saved "), "{}", stdout_of(&out));
    // Still an explicit user action: it is carried out.
    assert_eq!(db_tasks(&sb).len(), 1);
}

/// REVIEW №173: a backup clearly older than the database (it was changed
/// without the backup being refreshed) is called out as well.
#[test]
fn r9_restore_warns_about_a_backup_much_older_than_the_database() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let backup = sb.db_path().with_extension("json.backup");
    fs::write(&backup, "[]").unwrap();
    // A bit more than three days before the database was written just now.
    let saved = std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 86_400 + 3_600);
    fs::File::options()
        .write(true)
        .open(&backup)
        .unwrap()
        .set_modified(saved)
        .unwrap();

    let out = sb.cmd().arg("restore").output().unwrap();

    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(
        stderr_of(&out).contains("3 days older than the database"),
        "{}",
        stderr_of(&out)
    );
}

/// REVIEW №56 (SQLite addendum): a zero-length `.backup` is a valid empty
/// SQLite file; read the usual way it got the schema written into it and
/// then replaced a healthy database with zero tasks.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r9_sqlite_restore_refuses_a_zero_length_backup_and_leaves_it_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    let backup = dir.path().join("tasks.db.backup");
    let b = backend(&path);
    b.save(&[task(1, "one"), task(2, "two")]).unwrap();
    fs::write(&backup, b"").unwrap();

    let err = b.restore_from_backup().unwrap_err().to_string();

    assert!(err.contains("zero bytes"), "{err}");
    assert_eq!(fs::metadata(&backup).unwrap().len(), 0, "the backup was modified");
    assert_eq!(backend(&path).load().unwrap().len(), 2);

    // A SQLite file that is somebody else's database is not a backup either,
    // and reading it must not create the tasks table in it.
    fs::remove_file(&backup).unwrap();
    rusqlite::Connection::open(&backup)
        .unwrap()
        .execute_batch("CREATE TABLE notes (body TEXT); INSERT INTO notes VALUES ('x');")
        .unwrap();
    let before = fs::read(&backup).unwrap();

    let err = b.restore_from_backup().unwrap_err().to_string();

    assert!(err.contains("no tasks table"), "{err}");
    assert_eq!(fs::read(&backup).unwrap(), before, "the backup was modified");
    assert_eq!(backend(&path).load().unwrap().len(), 2);
}

/// REVIEW №157 for SQLite: a healthy database is replaced through a
/// transaction (no `fs::copy` over the live file), and the state it had is
/// kept as a loadable copy.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r9_sqlite_restore_goes_through_a_transaction_and_keeps_the_old_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    let b = backend(&path);
    b.save(&[task(1, "state-one")]).unwrap();
    b.save(&[task(1, "state-two"), task(2, "more")]).unwrap();

    // A writer that loaded before the restore must be told its snapshot is
    // stale afterwards — a byte copy under its feet would not do that.
    let other = backend(&path);
    let mut stale = other.load().unwrap();

    let restored = b.restore_from_backup().unwrap();

    assert_eq!(restored.len(), 1);
    assert_eq!(backend(&path).load().unwrap()[0].text, "state-one");
    let kept = backend(&dir.path().join("tasks.db.before_restore")).load().unwrap();
    assert_eq!(kept.len(), 2, "the replaced state must stay loadable");
    stale.push(task(3, "from a stale snapshot"));
    assert!(other.save(&stale).is_err(), "the stale writer overwrote the restore");
}

/// REVIEW №157/№1 for SQLite: a database SQLite itself rejects cannot be
/// written through a connection; restore keeps its raw bytes and starts a
/// fresh file.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r9_sqlite_restore_replaces_a_database_that_is_not_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    let b = backend(&path);
    b.save(&[task(1, "state-one")]).unwrap();
    b.save(&[task(1, "state-two")]).unwrap(); // .backup = state-one
    let garbage = b"this is not a sqlite file, but it may hold the user's only copy";
    fs::write(&path, garbage).unwrap();

    let restored = backend(&path).restore_from_backup().unwrap();

    assert_eq!(restored[0].text, "state-one");
    assert_eq!(backend(&path).load().unwrap()[0].text, "state-one");
    assert_eq!(
        fs::read(dir.path().join("tasks.db.before_restore")).unwrap(),
        garbage
    );
}

/// The bytes of a SQLite file differ after every save even when the tasks
/// do not: "already restored" is decided by the tasks, or every repeated
/// `rusk restore` would pile up another `.before_restore.N`.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r9_sqlite_repeated_restore_keeps_no_more_copies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    let b = backend(&path);
    b.save(&[task(1, "state-one")]).unwrap();
    b.save(&[task(1, "state-two")]).unwrap();

    b.restore_from_backup().unwrap();
    backend(&path).restore_from_backup().unwrap();
    backend(&path).restore_from_backup().unwrap();

    assert_eq!(
        names_in(dir.path()),
        ["tasks.db", "tasks.db.backup", "tasks.db.before_restore"]
    );
}

/// Replacing a file SQLite rejects keeps what a regular save keeps: the
/// mode of a private database and a symlinked database path.
#[test]
#[cfg(all(unix, feature = "backend-sqlite"))]
fn r9_sqlite_restore_over_garbage_keeps_the_mode_and_the_symlink() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let synced = dir.path().join("synced");
    fs::create_dir(&synced).unwrap();
    let real = synced.join("real.db");
    let link = dir.path().join("tasks.db");
    backend(&real).save(&[task(1, "state-one")]).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let b = backend(&link);
    b.load().unwrap();
    b.save(&[task(1, "state-two")]).unwrap(); // tasks.db.backup = state-one
    fs::write(&real, b"garbage where the database used to be").unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();

    backend(&link).restore_from_backup().unwrap();

    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(backend(&real).load().unwrap()[0].text, "state-one");
    let mode = fs::metadata(&real).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "mode after restore is {mode:o}");
}

/// A backup with the pre-`after` schema is read as it is: restore must not
/// migrate (write to) the file it restores from.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r9_sqlite_restore_reads_a_legacy_backup_without_migrating_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    let backup = dir.path().join("tasks.db.backup");
    {
        let conn = rusqlite::Connection::open(&backup).unwrap();
        conn.execute_batch(
            "CREATE TABLE tasks (
                pos INTEGER PRIMARY KEY,
                id INTEGER NOT NULL UNIQUE,
                text TEXT NOT NULL,
                date TEXT,
                done INTEGER NOT NULL DEFAULT 0,
                priority INTEGER NOT NULL DEFAULT 0
            );
            INSERT INTO tasks (pos, id, text) VALUES (1, 1, 'legacy');",
        )
        .unwrap();
    }
    let before = fs::read(&backup).unwrap();

    let restored = backend(&path).restore_from_backup().unwrap();

    assert_eq!(restored[0].text, "legacy");
    assert_eq!(fs::read(&backup).unwrap(), before, "the backup was modified");
    assert_eq!(backend(&path).load().unwrap()[0].text, "legacy");
}

// ---------------------------------------------------------------------------
// R12 — lost updates (closed: regressions)
//
// A save used to replace the whole database with the snapshot the process
// had loaded, however old, so whatever another writer had stored in between
// vanished without a word. Now a backend remembers what it loaded: a plain
// `save` of a stale list is refused, and `TaskManager::update` — what every
// command uses — applies its change to the database as it is now.
// ---------------------------------------------------------------------------

fn texts(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|t| t.text.as_str()).collect()
}

/// REVIEW №155: two writers that both loaded before either saved — the
/// second save silently dropped the first writer's task. It is refused now,
/// with an error the caller can tell from an I/O failure, and it leaves the
/// database and the backup alone.
#[test]
fn r12_stale_save_does_not_silently_drop_another_writers_task() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "seed")]).unwrap();

    let (a, b) = (backend(&path), backend(&path));
    let mut tasks_a = a.load().unwrap();
    let mut tasks_b = b.load().unwrap();
    tasks_a.push(task(2, "written by a"));
    tasks_b.push(task(2, "written by b"));

    a.save(&tasks_a).unwrap();
    let backup_after_a = fs::read(dir.path().join("tasks.json.backup")).unwrap();
    let second = b.save(&tasks_b);

    let err = second.expect_err("a save based on a stale read must be refused");
    assert!(err.is::<StaleDatabase>(), "{err}");
    assert!(err.to_string().contains("changed by another process"), "{err}");
    assert_eq!(texts(&backend(&path).load().unwrap()), ["seed", "written by a"]);
    assert_eq!(
        fs::read(dir.path().join("tasks.json.backup")).unwrap(),
        backup_after_a,
        "the refused save rotated the backup anyway"
    );

    // Once it has read the current state, the same writer succeeds.
    let mut fresh = b.load().unwrap();
    fresh.push(task(3, "written by b"));
    b.save(&fresh).unwrap();
    assert_eq!(backend(&path).load().unwrap().len(), 3);
}

/// A database that appears (or disappears) between load and save counts as
/// changed, too.
#[test]
fn r12_stale_save_notices_a_database_that_appeared_or_vanished() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");

    let (a, b) = (backend(&path), backend(&path));
    assert!(a.load().unwrap().is_empty());
    assert!(b.load().unwrap().is_empty());
    a.save(&[task(1, "from a")]).unwrap();
    assert!(b.save(&[task(1, "from b")]).unwrap_err().is::<StaleDatabase>());
    assert_eq!(texts(&backend(&path).load().unwrap()), ["from a"]);

    let c = backend(&path);
    c.load().unwrap();
    fs::remove_file(&path).unwrap();
    assert!(c.save(&[task(1, "from c")]).unwrap_err().is::<StaleDatabase>());
    assert!(!path.exists());
}

/// A backend that never loaded anything has nothing to be stale against:
/// tools (and most tests) write a fresh list this way.
#[test]
fn r12_a_backend_that_never_loaded_replaces_unconditionally() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "first")]).unwrap();
    backend(&path).save(&[task(1, "second")]).unwrap();
    assert_eq!(texts(&backend(&path).load().unwrap()), ["second"]);

    // Its own writes keep it current.
    let b = backend(&path);
    for round in 0..3 {
        b.save(&[task(1, &format!("round {round}"))]).unwrap();
    }
    assert_eq!(texts(&b.load().unwrap()), ["round 2"]);
}

/// REVIEW №155, the fix proper: a command whose snapshot went stale applies
/// its change to the current database instead of writing the snapshot back.
#[test]
fn r12_update_applies_the_change_to_the_current_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path)
        .save(&[task(1, "alpha"), task(2, "beta")])
        .unwrap();

    // The editor / the prompt of this "process" is open...
    let mut slow = TaskManager::open_at(path.clone()).unwrap();
    // ...while another terminal adds a task and marks one done.
    let mut other = TaskManager::open_at(path.clone()).unwrap();
    other.add_task(vec!["gamma".into()], None).unwrap();
    other.mark_tasks(vec![2]).unwrap();

    slow.edit_tasks(vec![1], Some(vec!["alpha".into(), "edited".into()]), None)
        .unwrap();

    let on_disk = backend(&path).load().unwrap();
    assert_eq!(texts(&on_disk), ["alpha edited", "beta", "gamma"]);
    assert!(on_disk[1].done, "the other terminal's `mark 2` was lost");
    assert_eq!(slow.tasks(), on_disk.as_slice(), "the manager holds what it saved");

    // The backup holds what this save replaced — the other writer's state.
    let backup = backend(&dir.path().join("tasks.json.backup")).load().unwrap();
    assert_eq!(texts(&backup), ["alpha", "beta", "gamma"]);
}

/// Ids are handed out against the current database, so two processes that
/// both saw "next id is 2" do not both use it.
#[test]
fn r12_concurrent_adds_get_different_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "seed")]).unwrap();

    let mut a = TaskManager::open_at(path.clone()).unwrap();
    let mut b = TaskManager::open_at(path.clone()).unwrap();
    a.add_task(vec!["from a".into()], None).unwrap();
    b.add_task(vec!["from b".into()], None).unwrap();

    assert_eq!(b.tasks().last().unwrap().id, 3, "what the CLI reports as added");
    let on_disk = backend(&path).load().unwrap();
    assert_eq!(texts(&on_disk), ["seed", "from a", "from b"]);
    assert_eq!(on_disk.iter().map(|t| t.id).collect::<Vec<_>>(), [1, 2, 3]);
}

/// Many writers at once, in one process: nothing is lost, no id is used
/// twice. (`r12_parallel_adds_lose_nothing` does the same with processes.)
#[test]
fn r12_parallel_updates_all_take_effect() {
    const WRITERS: u32 = 12;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "seed")]).unwrap();

    std::thread::scope(|scope| {
        for n in 0..WRITERS {
            let path = path.clone();
            scope.spawn(move || {
                let mut tm = TaskManager::open_at(path).unwrap();
                tm.add_task(vec![format!("writer-{n}")], None).unwrap();
            });
        }
    });

    let on_disk = backend(&path).load().unwrap();
    assert_eq!(on_disk.len() as u32, WRITERS + 1);
    let mut ids: Vec<u32> = on_disk.iter().map(|t| t.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len() as u32, WRITERS + 1, "an id was handed out twice");
}

/// Tasks edited in memory and not saved cannot be carried over to a fresh
/// read; dropping them silently would be the same bug in a new place.
#[test]
fn r12_update_does_not_drop_unsaved_in_memory_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "one"), task(2, "two")]).unwrap();

    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    tm.tasks_mut()[0].text = "edited in memory".into();
    TaskManager::open_at(path.clone())
        .unwrap()
        .add_task(vec!["from elsewhere".into()], None)
        .unwrap();

    let err = tm.mark_tasks(vec![2]).unwrap_err();
    assert!(err.is::<StaleDatabase>(), "{err}");
    assert_eq!(texts(&backend(&path).load().unwrap()), ["one", "two", "from elsewhere"]);

    // Without a concurrent change the in-memory edit is simply part of the
    // save, as it always was.
    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    tm.tasks_mut()[0].text = "edited in memory".into();
    tm.mark_tasks(vec![2]).unwrap();
    let on_disk = backend(&path).load().unwrap();
    assert_eq!(on_disk[0].text, "edited in memory");
    assert!(on_disk[1].done);
}

/// The same through a no-op in between: a change that gave no reason to
/// write must not make the in-memory edit pass for a saved one.
#[test]
fn r12_a_noop_does_not_pass_unsaved_edits_for_saved_ones() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "one"), task(2, "two")]).unwrap();

    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    tm.tasks_mut()[0].text = "edited in memory".into();
    let (marked, not_found) = tm.mark_tasks(vec![9]).unwrap();
    assert!(marked.is_empty() && not_found == [9]);
    assert_eq!(texts(&backend(&path).load().unwrap()), ["one", "two"], "a no-op wrote");

    TaskManager::open_at(path.clone())
        .unwrap()
        .add_task(vec!["from elsewhere".into()], None)
        .unwrap();
    let err = tm.mark_tasks(vec![2]).unwrap_err();
    assert!(err.is::<StaleDatabase>(), "the in-memory edit was dropped silently: {err}");
    assert_eq!(tm.tasks()[0].text, "edited in memory");
}

/// A command that changes nothing writes nothing: no rewrite, no backup
/// rotation — and no directory or lock file where there was no database.
#[test]
fn r12_a_change_that_changes_nothing_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let nowhere = dir.path().join("not-created").join("tasks.json");
    let mut tm = TaskManager::open_at(nowhere.clone()).unwrap();
    let (marked, not_found) = tm.mark_tasks(vec![7]).unwrap();
    assert!(marked.is_empty());
    assert_eq!(not_found, [7]);
    assert!(!nowhere.parent().unwrap().exists(), "a no-op created the database directory");

    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "one")]).unwrap();
    let before = fs::read(&path).unwrap();
    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    tm.edit_tasks(vec![1], Some(vec!["one".into()]), None).unwrap();
    tm.mark_tasks(vec![9]).unwrap();
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!dir.path().join("tasks.json.backup").exists(), "a no-op rotated the backup");
}

/// REVIEW №155, the editor: a session that lasted minutes stores its text
/// when the task still is what the editor showed; everything else in the
/// database — other tasks, the task's own flags — stays as it is now.
#[test]
fn r12_editor_result_lands_on_the_current_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "alpha"), task(2, "beta")]).unwrap();

    let mut editing = TaskManager::open_at(path.clone()).unwrap();
    let mut other = TaskManager::open_at(path.clone()).unwrap();
    other.add_task(vec!["gamma".into()], None).unwrap();
    other.mark_tasks(vec![1, 2]).unwrap();

    editing
        .edit_task_as_seen(1, ("alpha", None), ("Xalpha", None))
        .unwrap();

    let on_disk = backend(&path).load().unwrap();
    assert_eq!(texts(&on_disk), ["Xalpha", "beta", "gamma"]);
    assert!(on_disk[0].done && on_disk[1].done, "flags set meanwhile were reset");
}

/// ...and it is refused when the task was rewritten, deleted, or deleted
/// and its id handed to a new task while the editor was open.
#[test]
fn r12_editor_result_is_refused_when_the_task_changed_underneath() {
    use rusk::storage::TaskChanged;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let seed = [task(1, "alpha"), task(2, "beta"), task(3, "gamma")];
    backend(&path).save(&seed).unwrap();

    let mut editing = TaskManager::open_at(path.clone()).unwrap();
    let mut other = TaskManager::open_at(path.clone()).unwrap();
    other.edit_tasks(vec![1], Some(vec!["alpha, reworded".into()]), None).unwrap();
    other.delete_tasks(vec![2]).unwrap();
    other.delete_tasks(vec![3]).unwrap();
    other.add_task(vec!["brand new".into()], None).unwrap(); // takes id 2
    let expected = backend(&path).load().unwrap();

    let refused = |tm: &mut TaskManager, id: u32, seen: &str| {
        let err = tm
            .edit_task_as_seen(id, (seen, None), ("typed in the editor", None))
            .unwrap_err();
        err.downcast::<TaskChanged>().expect("a TaskChanged error")
    };
    assert_eq!(refused(&mut editing, 1, "alpha"), TaskChanged { id: 1, deleted: false });
    assert_eq!(refused(&mut editing, 2, "beta"), TaskChanged { id: 2, deleted: false });
    assert_eq!(refused(&mut editing, 3, "gamma"), TaskChanged { id: 3, deleted: true });
    assert_eq!(backend(&path).load().unwrap(), expected, "a refused edit wrote something");
}

/// REVIEW №155, the `del` prompt: what was confirmed is a task, not an id.
/// A task that another process changed — or deleted, the id going to a new
/// task — while the prompt waited is reported, not deleted.
#[test]
fn r12_confirmed_deletion_spares_tasks_that_are_no_longer_what_was_confirmed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let seed = [task(1, "one"), task(2, "two"), task(3, "three"), task(4, "four")];
    backend(&path).save(&seed).unwrap();

    let mut prompting = TaskManager::open_at(path.clone()).unwrap();
    let confirmed: Vec<Task> = prompting.tasks().to_vec();

    let mut other = TaskManager::open_at(path.clone()).unwrap();
    other.delete_tasks(vec![2]).unwrap();
    other.add_task(vec!["added concurrently".into()], None).unwrap(); // takes id 2
    other.delete_tasks(vec![3]).unwrap();
    other.mark_tasks(vec![4]).unwrap();

    let outcome = prompting
        .delete_confirmed(&confirmed, |shown, now| shown.text == now.text)
        .unwrap();
    assert_eq!(outcome.deleted, [1, 4], "a flag flipped meanwhile does not make it another task");
    assert_eq!(outcome.changed, [2]);
    assert_eq!(outcome.gone, [3]);
    assert_eq!(texts(&backend(&path).load().unwrap()), ["added concurrently"]);
}

/// Dependencies are validated against the database as it is at the save.
#[test]
fn r12_dependencies_are_checked_against_the_current_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path).save(&[task(1, "one"), task(2, "two")]).unwrap();

    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    TaskManager::open_at(path.clone()).unwrap().delete_tasks(vec![2]).unwrap();

    let err = tm
        .add_task_with_after(vec!["needs two".into()], None, vec![2])
        .unwrap_err()
        .to_string();
    assert!(err.contains("missing task(s): 2"), "{err}");
    assert_eq!(texts(&backend(&path).load().unwrap()), ["one"]);
}

/// `rusk restore` runs under the same writer lock and the same staleness
/// check as a save; afterwards the backend is current again.
#[test]
fn r12_restore_and_later_saves_stay_consistent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    tm.add_task(vec!["kept".into()], None).unwrap();
    tm.add_task(vec!["to be undone".into()], None).unwrap();

    tm.restore_from_backup().unwrap();
    assert_eq!(texts(tm.tasks()), ["kept"]);
    tm.add_task(vec!["after restore".into()], None).unwrap();
    assert_eq!(texts(&backend(&path).load().unwrap()), ["kept", "after restore"]);
}

/// REVIEW №154: SQLite writers serialize on the write lock; the change of
/// each is applied to what the one before left, inside one transaction.
#[cfg(feature = "backend-sqlite")]
#[test]
fn r12_sqlite_parallel_updates_all_take_effect() {
    const WRITERS: u32 = 12;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    backend(&path).save(&[task(1, "seed")]).unwrap();

    std::thread::scope(|scope| {
        for n in 0..WRITERS {
            let path = path.clone();
            scope.spawn(move || {
                let mut tm = TaskManager::open_at(path).unwrap();
                tm.add_task(vec![format!("writer-{n}")], None).unwrap();
            });
        }
    });

    let on_disk = backend(&path).load().unwrap();
    assert_eq!(on_disk.len() as u32, WRITERS + 1);
    let mut ids: Vec<u32> = on_disk.iter().map(|t| t.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len() as u32, WRITERS + 1, "an id was handed out twice");
}

/// A sync tool (or a copy from another machine) replaces the SQLite file by
/// rename while a command waits in the editor. The kept connection still
/// has the old, unlinked file open, where `data_version` never moves: the
/// change must go to the file the path names now, on top of its content.
#[cfg(all(feature = "backend-sqlite", unix))]
#[test]
fn r12_sqlite_database_replaced_by_rename_is_noticed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    backend(&path).save(&[task(1, "one"), task(2, "two")]).unwrap();

    let mut waiting = TaskManager::open_at(path.clone()).unwrap();
    let stale_writer = backend(&path);
    let stale_snapshot = stale_writer.load().unwrap();

    let incoming = dir.path().join("incoming.db");
    backend(&incoming)
        .save(&[task(1, "one"), task(2, "two"), task(3, "synced from the other machine")])
        .unwrap();
    fs::rename(&incoming, &path).unwrap();

    waiting.mark_tasks(vec![1]).unwrap();
    let on_disk = backend(&path).load().unwrap();
    assert_eq!(texts(&on_disk), ["one", "two", "synced from the other machine"]);
    assert!(on_disk[0].done, "the change went to the unlinked file");

    let err = stale_writer.save(&stale_snapshot).unwrap_err();
    assert!(err.is::<StaleDatabase>(), "{err}");
}

/// Worker half of `r12_sqlite_parallel_processes_lose_nothing`: a no-op
/// unless that test started this binary as a child process.
#[cfg(feature = "backend-sqlite")]
#[test]
fn r12_sqlite_worker() {
    let Some(path) = std::env::var_os("RUSK_R12_WORKER_DB") else {
        return;
    };
    let text = std::env::var("RUSK_R12_WORKER_TEXT").unwrap();
    let mut tm = TaskManager::open_at(std::path::PathBuf::from(path)).unwrap();
    tm.add_task(vec![text], None).unwrap();
}

/// REVIEW №154 across processes (threads of one process cannot show it:
/// SQLite arbitrates those in memory). The `.backup` copy is taken inside
/// the write transaction, and closing the handle it was read through drops
/// the POSIX locks of the process — SQLite's write lock with them — so the
/// other processes wrote in the middle of the transaction: adds exited 0
/// with ids already taken and vanished. The handle now outlives the
/// transaction.
#[cfg(feature = "backend-sqlite")]
#[test]
fn r12_sqlite_parallel_processes_lose_nothing() {
    const WRITERS: usize = 16;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    backend(&path).save(&[task(1, "seed")]).unwrap();

    let children: Vec<_> = (0..WRITERS)
        .map(|n| {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["r12_sqlite_worker", "--exact", "--test-threads=1"])
                .env("RUSK_R12_WORKER_DB", &path)
                .env("RUSK_R12_WORKER_TEXT", format!("process-{n}"))
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for (n, child) in children.into_iter().enumerate() {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "writer {n} failed: {}{}",
            stdout_of(&out),
            stderr_of(&out)
        );
    }

    let on_disk = backend(&path).load().unwrap();
    let mut found = texts(&on_disk);
    found.sort_unstable();
    let mut expected: Vec<String> = (0..WRITERS).map(|n| format!("process-{n}")).collect();
    expected.push("seed".into());
    expected.sort_unstable();
    assert_eq!(found, expected, "a writer that exited 0 lost its task");
}

/// REVIEW №154 (status note of R14/R9): a refused SQLite save used to
/// rotate `.backup` anyway, and a save that failed early left the backend
/// able to pass the staleness check the next time.
#[cfg(feature = "backend-sqlite")]
#[test]
fn r12_sqlite_refused_save_keeps_the_backup_and_stays_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.db");
    backend(&path).save(&[task(1, "seed")]).unwrap();

    let (a, b) = (backend(&path), backend(&path));
    let mut tasks_a = a.load().unwrap();
    let mut tasks_b = b.load().unwrap();
    tasks_a.push(task(2, "written by a"));
    tasks_b.push(task(2, "written by b"));
    a.save(&tasks_a).unwrap();
    let backup_after_a = fs::read(dir.path().join("tasks.db.backup")).unwrap();

    for _ in 0..2 {
        let err = b.save(&tasks_b).unwrap_err();
        assert!(err.is::<StaleDatabase>(), "{err}");
    }
    assert_eq!(fs::read(dir.path().join("tasks.db.backup")).unwrap(), backup_after_a);
    assert_eq!(texts(&backend(&path).load().unwrap()), ["seed", "written by a"]);

    // The update path re-reads inside its transaction instead of failing.
    let mut slow = TaskManager::open_at(path.clone()).unwrap();
    TaskManager::open_at(path.clone())
        .unwrap()
        .add_task(vec!["from elsewhere".into()], None)
        .unwrap();
    slow.mark_tasks(vec![1]).unwrap();
    let on_disk = backend(&path).load().unwrap();
    assert_eq!(texts(&on_disk), ["seed", "written by a", "from elsewhere"]);
    assert!(on_disk[0].done);
}

/// REVIEW №155 (with №13): parallel `rusk add` processes — every process
/// that exits 0 must find its task in the database afterwards. The №13 half
/// (writers tripping over the shared `tasks.json.tmp`) was closed by R14,
/// see `r14_parallel_writers_and_readers_never_see_a_broken_database`.
#[test]
fn r12_parallel_adds_lose_nothing() {
    const N: usize = 16;
    let sb = Sandbox::with_db("[]");
    let children: Vec<_> = (0..N)
        .map(|i| {
            sb.cmd()
                .args(["add", &format!("parallel-{i}")])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let outputs: Vec<_> = children
        .into_iter()
        .map(|c| c.wait_with_output().unwrap())
        .collect();

    let db = sb.read_db();
    for (i, out) in outputs.iter().enumerate() {
        assert!(out.status.success(), "add #{i} failed: {}", stderr_of(out));
        assert_eq!(stderr_of(out), "", "add #{i} had something to complain about");
        assert!(
            db.contains(&format!("parallel-{i}")),
            "add #{i} exited 0 but its task is not in the database"
        );
    }
    let mut ids: Vec<u64> = db_tasks(&sb).iter().map(|t| t["id"].as_u64().unwrap()).collect();
    ids.sort_unstable();
    assert_eq!(ids, (1..=N as u64).collect::<Vec<_>>(), "ids must be 1..=N, each once");
}

/// A debug build seeds sample tasks into an empty database. Several first
/// runs at once all find it empty; the seeding used to be a plain save, so
/// all but one of them failed with "changed by another process".
#[test]
fn r12_parallel_first_runs_of_a_debug_build_all_succeed() {
    let sb = Sandbox::new();
    let children: Vec<_> = (0..6)
        .map(|_| {
            // Not in test mode: that is what turns the seeding off.
            sb.cmd()
                .env_remove("RUST_TEST_THREADS")
                .env_remove("CARGO_TEST")
                .env_remove("__CARGO_TEST_CHANNEL")
                .arg("list")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "`rusk list` failed: {}", stderr_of(&out));
    }
    // A release build seeds nothing; a debug build must have seeded once.
    let db = sb.read_db();
    if !db.is_empty() {
        let ids: Vec<u64> = db_tasks(&sb).iter().map(|t| t["id"].as_u64().unwrap()).collect();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(ids.len(), unique.len(), "the samples were seeded more than once");
    }
}

/// Different commands at once: each one's effect is in the final database.
#[test]
fn r12_parallel_mixed_commands_all_take_effect() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let commands: [&[&str]; 5] = [
        &["add", "fourth task"],
        &["mark", "1"],
        &["mark", "2", "-p"],
        &["edit", "3", "third", "task,", "reworded"],
        &["add", "fifth task"],
    ];
    let children: Vec<_> = commands
        .iter()
        .map(|args| {
            sb.cmd()
                .args(*args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for (child, args) in children.into_iter().zip(commands) {
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "`rusk {args:?}` failed: {}", stderr_of(&out));
    }

    let tasks = db_tasks(&sb);
    assert_eq!(tasks.len(), 5);
    assert_eq!(tasks[0]["done"], true, "`mark 1` was lost");
    assert_eq!(tasks[1]["priority"], true, "`mark 2 -p` was lost");
    assert_eq!(text_of(&tasks, 3), "third task, reworded");
    let all = sb.read_db();
    assert!(all.contains("fourth task") && all.contains("fifth task"), "{all}");
}

// ---------------------------------------------------------------------------
// R14 — one atomic write routine (closed: regressions)
// ---------------------------------------------------------------------------

/// REVIEW №13: writers shared one `tasks.json.tmp`, so parallel saves fell
/// back to truncating the live database and readers saw an empty file.
/// (Lost updates between the writers are a different finding: №155, R12.)
#[test]
fn r14_parallel_writers_and_readers_never_see_a_broken_database() {
    const WRITERS: usize = 12;
    const READERS: usize = 6;
    let sb = Sandbox::with_db("[]");
    let spawn = |args: &[&str]| {
        sb.cmd()
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap()
    };
    let mut children = Vec::new();
    for i in 0..WRITERS.max(READERS) {
        if i < WRITERS {
            children.push(("add", spawn(&["add", &format!("parallel-{i}")])));
        }
        if i < READERS {
            children.push(("list", spawn(&["list"])));
        }
    }

    for (what, child) in children {
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "`rusk {what}` failed: {}", stderr_of(&out));
        assert_eq!(stderr_of(&out), "", "`rusk {what}` had something to complain about");
    }
    assert!(!db_tasks(&sb).is_empty(), "the database must hold complete JSON");
    assert_eq!(
        names_in(sb.db_path().parent().unwrap()),
        ["tasks.json", "tasks.json.backup"],
        "a temp file was left behind"
    );
}

/// REVIEW №20/№176 (H1 of the security audit): the backup of a private
/// database is just as private.
#[test]
#[cfg(unix)]
fn r14_backup_is_as_private_as_the_database() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let b = backend(&path);
    b.save(&[task(1, "private")]).unwrap();
    let backup = dir.path().join("tasks.json.backup");
    // 0640 cannot come out by accident: it is neither the umask default
    // nor the 0600 the temp file is created with.
    for wanted in [0o600, 0o640] {
        fs::set_permissions(&path, fs::Permissions::from_mode(wanted)).unwrap();

        b.save(&[task(1, "still private")]).unwrap();

        let mode = fs::metadata(&backup).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, wanted, "backup mode is {mode:o}");
    }
}

/// A zero-length database (what a crashed in-place write or a full disk
/// leaves behind; for Markdown, todo.txt and NDJSON it loads as "no tasks"
/// without complaint) must not rotate the last good backup away.
#[test]
#[cfg(feature = "fmt-markdown")]
fn r14_a_zero_length_database_does_not_replace_the_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.md");
    let b = backend(&path);
    b.save(&[task(1, "precious")]).unwrap();
    b.save(&[task(1, "precious"), task(2, "more")]).unwrap(); // .backup = [precious]
    fs::write(&path, b"").unwrap(); // the crash

    let survivor = backend(&path);
    assert!(survivor.load().unwrap().is_empty());
    survivor.save(&[task(1, "typed after the crash")]).unwrap();

    let backup = fs::read_to_string(dir.path().join("tasks.md.backup")).unwrap();
    assert!(backup.contains("precious"), "the last good copy is gone: {backup:?}");
}

/// REVIEW №175 (H1), the `.backup` half: the predictable backup name is
/// not written through a planted symlink either.
#[test]
#[cfg(unix)]
fn r14_backup_does_not_write_through_a_planted_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let victim = dir.path().join("victim.txt");
    let b = backend(&path);
    b.save(&[task(1, "payload")]).unwrap();
    fs::write(&victim, "do not touch").unwrap();
    std::os::unix::fs::symlink(&victim, dir.path().join("tasks.json.backup")).unwrap();

    b.save(&[task(1, "payload"), task(2, "more")]).unwrap();

    assert_eq!(fs::read_to_string(&victim).unwrap(), "do not touch");
    let backup = fs::read_to_string(dir.path().join("tasks.json.backup")).unwrap();
    assert!(backup.contains("payload") && !backup.contains("more"), "{backup}");
}

/// REVIEW №19, the save half of the dangling link: the link is not
/// silently replaced by a regular file any more.
#[test]
#[cfg(unix)]
fn r14_save_through_a_dangling_symlink_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("tasks.json");
    std::os::unix::fs::symlink(dir.path().join("unmounted/tasks.json"), &link).unwrap();

    let err = backend(&link).save(&[task(1, "lost?")]).unwrap_err().to_string();

    assert!(err.contains("symbolic link"), "{err}");
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
}

/// REVIEW №20: the atomic write resets a private 0600 database to 0644.
#[test]
#[cfg(unix)]
fn r14_save_preserves_the_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let b = backend(&path);
    b.save(&[task(1, "private")]).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    b.save(&[task(1, "private"), task(2, "still private")]).unwrap();

    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "mode after save is {mode:o}");

    // 0600 is also what the temp file is created with; a mode that has to
    // be carried over explicitly must survive as well.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    b.save(&[task(1, "shared with the group")]).unwrap();
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640, "mode after save is {mode:o}");
}

/// REVIEW №175: the predictable `tasks.json.tmp` is opened without
/// O_EXCL/O_NOFOLLOW — a planted symlink gets its target overwritten.
#[test]
#[cfg(unix)]
fn r14_save_does_not_write_through_a_planted_temp_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let victim = dir.path().join("victim.txt");
    fs::write(&victim, "do not touch").unwrap();
    std::os::unix::fs::symlink(&victim, dir.path().join("tasks.json.tmp")).unwrap();

    let _ = backend(&path).save(&[task(1, "payload")]);

    assert_eq!(fs::read_to_string(&victim).unwrap(), "do not touch");
    assert!(
        !fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false),
        "the database itself became a symlink to the victim"
    );
}

/// REVIEW №176: `.backup` inherits 0444 from a read-only database and can
/// never be refreshed again — restore silently returns an old state.
#[test]
#[cfg(unix)]
fn r14_backup_of_a_read_only_database_keeps_updating() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    let b = backend(&path);
    b.save(&[task(1, "state-one")]).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    b.save(&[task(1, "state-two")]).unwrap(); // backup = state-one, mode 0444
    fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
    b.save(&[task(1, "state-three")]).unwrap(); // backup must become state-two

    let backup = fs::read_to_string(dir.path().join("tasks.json.backup")).unwrap();
    assert!(backup.contains("state-two"), "stale backup: {backup}");
}

/// REVIEW №19: saving a symlinked database replaces the link with a regular
/// file; the real file (e.g. in a synced folder) silently stops updating.
#[test]
#[cfg(unix)]
fn r14_save_keeps_a_symlinked_database_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real.json");
    let link = dir.path().join("tasks.json");
    backend(&real).save(&[task(1, "old")]).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    backend(&link).save(&[task(1, "new")]).unwrap();

    assert!(
        fs::symlink_metadata(&link).unwrap().file_type().is_symlink(),
        "the symlink was replaced by a regular file"
    );
    assert!(fs::read_to_string(&real).unwrap().contains("new"));
}

/// REVIEW №172: an extensionless database (`tasks`) gets the auxiliary
/// names of `tasks.json`, so the two share `.backup` and `restore` brings
/// back the other database.
#[test]
fn r14_extensionless_database_has_its_own_backup() {
    let dir = tempfile::tempdir().unwrap();
    let plain = backend(&dir.path().join("tasks"));
    let json = backend(&dir.path().join("tasks.json"));
    plain.save(&[task(1, "plain-one")]).unwrap();
    plain.save(&[task(1, "plain-two")]).unwrap();
    json.save(&[task(1, "json-one")]).unwrap();
    json.save(&[task(1, "json-two")]).unwrap();

    let restored = plain.restore_from_backup().unwrap();
    assert_eq!(restored[0].text, "plain-one", "restored the other database's backup");
}

/// REVIEW №178: `rusk gen -o FILE` truncates and rewrites the page in place;
/// a failed write leaves a truncated page instead of the previous version.
#[test]
#[cfg(all(unix, feature = "web"))]
fn r14_gen_replaces_the_page_atomically() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let page = sb.path().join("index.html");
    fs::write(&page, "previous version").unwrap();
    let witness = sb.path().join("witness");
    fs::hard_link(&page, &witness).unwrap();

    let out = sb
        .cmd()
        .args(["gen", "-o", page.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));

    assert!(fs::read_to_string(&page).unwrap().contains("first task"));
    assert_eq!(
        fs::read_to_string(&witness).unwrap(),
        "previous version",
        "the page was rewritten in place instead of being replaced by rename"
    );
}

// ---------------------------------------------------------------------------
// R16 — web UI: one way to change a task
// ---------------------------------------------------------------------------
//
// The cluster lives in the page script (`src/web/template.html`), which has
// no harness here; it was checked black-box in headless Chromium (REVIEW.md,
// status of R16). What the page relies on from the server is pinned below
// and in the unit tests of `src/web/api.rs`.

/// REVIEW №196: Save in the edit dialog without a change sent a PATCH with
/// the values the task already had, and the server saved it like any other:
/// the one-level `.backup` was overwritten with a copy of the current
/// database, so the state before the last real change was gone. The page no
/// longer sends such a PATCH; the server does not write one either way.
#[test]
#[cfg(feature = "web")]
fn r16_a_patch_that_changes_nothing_leaves_the_backup_alone() {
    use rusk::web::api::{delete_task, update_task};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tasks.json");
    backend(&path)
        .save(&[task(1, "one"), task(2, "two"), task(3, "three")])
        .unwrap();

    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    assert_eq!(delete_task(&mut tm, 3, None).status, 204);
    let backup = dir.path().join("tasks.json.backup");
    let kept = fs::read_to_string(&backup).unwrap();
    assert!(kept.contains("three"), "the backup holds the state before the delete");
    let database = fs::read(&path).unwrap();

    // What the old page sent on an untouched Save, and what a script may.
    let mut tm = TaskManager::open_at(path.clone()).unwrap();
    let unchanged = r#"{"text":"one","date":null,"priority":false}"#;
    assert_eq!(update_task(&mut tm, 1, unchanged, None).status, 200);
    assert_eq!(update_task(&mut tm, 1, "{}", None).status, 200);

    assert_eq!(fs::read_to_string(&backup).unwrap(), kept, "the backup was rotated");
    assert_eq!(fs::read(&path).unwrap(), database, "the database was rewritten");
}

// ---------------------------------------------------------------------------
// R4 — text measured in terminal cells
// ---------------------------------------------------------------------------
//
// `unicode-width` plus grapheme segmentation (`src/width.rs`) behind every
// layout decision: the wrapped list, the delete dialog and the editor's soft
// wrap, cursor and mouse mapping. The editor half is covered by the unit
// tests of `src/cli/editor/view.rs` and checked in a pty (REVIEW.md, status
// of R4); what a terminal actually receives is pinned below.

/// Width of one printed line in terminal cells.
fn cells(line: &str) -> usize {
    rusk::width::width(line)
}

/// Lines of `rusk list` output that carry something (header rule and blanks
/// dropped), in order.
fn list_lines(out: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.contains('─'))
        .map(|l| l.to_string())
        .collect()
}

/// REVIEW №34: wrapping counted `char`s, so a CJK or emoji task ran 120-133
/// columns wide on an 80-column terminal and the terminal wrapped it again,
/// without the indent. The budget is cells now.
#[test]
fn r4_wide_text_wraps_by_cells_not_by_chars() {
    let sb = Sandbox::new();
    let cjk = "日本語のテキスト".repeat(9);
    let apples = vec!["🍎".repeat(10); 6].join(" ");
    for text in [&cjk, &apples] {
        let out = sb.cmd().args(["add", text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    }

    let out = sb.cmd().arg("list").output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    for line in list_lines(&out) {
        assert!(
            cells(&line) <= 80,
            "{} cells on an 80-column terminal: {line}",
            cells(&line)
        );
    }
    // The text is still all there, just spread over rows.
    let printed: String = list_lines(&out)
        .iter()
        .skip(1)
        .map(|l| l.trim().to_string())
        .collect();
    assert!(printed.contains(&cjk[..cjk.len() / 2]), "{printed}");
}

/// REVIEW №34: a long word was cut every `width` chars, which split ZWJ
/// sequences (a row ended with a bare joiner), broke flag pairs apart and
/// left a combining accent to start the next row. Cuts land between
/// grapheme clusters now.
#[test]
fn r4_wrapping_never_cuts_inside_a_cluster() {
    let family = "👨\u{200d}👩\u{200d}👧";
    let sb = Sandbox::new();
    for text in [family.repeat(30), "e\u{301}".repeat(70), "🇺🇸".repeat(40)] {
        let out = sb.cmd().args(["add", &text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    }

    let out = sb.cmd().arg("list").output().unwrap();
    for line in list_lines(&out).iter().skip(1) {
        let trimmed = line.trim_start();
        assert!(
            !trimmed.starts_with('\u{200d}') && !trimmed.ends_with('\u{200d}'),
            "a joiner is left at a row edge: {line:?}"
        );
        assert!(
            !trimmed.starts_with('\u{301}'),
            "an accent was moved away from its letter: {line:?}"
        );
    }
    // Flags stay paired: every row holds whole regional-indicator pairs.
    let flag_row = list_lines(&out)
        .into_iter()
        .find(|l| l.contains('🇺'))
        .expect("the flags task is listed");
    let indicators = flag_row.chars().filter(|c| ('🇦'..='🇿').contains(c)).count();
    assert_eq!(indicators % 2, 0, "a flag was split: {flag_row:?}");
}

/// REVIEW №34: the `(1,2)` dependency note was appended to the last wrapped
/// line without counting its width, so a full-width task ran past the
/// terminal. It gets a line of its own when it does not fit, and in compact
/// mode its room is taken out of the wrap budget.
#[test]
fn r4_the_dependency_note_stays_inside_the_width() {
    let sb = Sandbox::new();
    for text in ["dep one", "dep two"] {
        sb.cmd().args(["add", text]).output().unwrap();
    }
    let out = sb
        .cmd()
        .args(["add", &"x".repeat(57), "-a", "1,2"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));

    for args in [vec!["list"], vec!["list", "--compact"]] {
        let out = sb.cmd().args(&args).output().unwrap();
        for line in list_lines(&out) {
            assert!(cells(&line) <= 80, "{args:?}: {} cells: {line}", cells(&line));
        }
        let printed = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(printed.contains("(1,2)"), "{args:?}: the note is gone:\n{printed}");
    }

    // Compact mode still shows one line per task.
    let out = sb.cmd().args(["list", "--compact"]).output().unwrap();
    assert_eq!(list_lines(&out).len(), 1 + 3, "one line per task plus the header");
}

/// REVIEW №35: ids of three digits and more pushed the date and the text to
/// the right while wrapped lines kept a fixed 19-column indent. The id
/// column is as wide as the widest id on screen, and everything lines up
/// with it - including the header.
#[test]
fn r4_the_id_column_is_as_wide_as_the_widest_id() {
    let sb = Sandbox::with_db(
        r#"[{"id":10,"text":"ten"},
            {"id":100,"text":"three digit id and a long text that has to wrap onto a second line because it is long","date":"2026-09-23"},
            {"id":1000,"text":"four digit","done":true,"date":"2026-09-14"},
            {"id":4294967295,"text":"u32 max","priority":true}]"#,
    );
    let out = sb.cmd().arg("list").output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let lines = list_lines(&out);

    // Where a word starts on screen: cells before it, not bytes.
    let text_col = |needle: &str| -> usize {
        lines
            .iter()
            .find(|l| l.contains(needle))
            .map(|l| cells(&l[..l.find(needle).unwrap()]))
            .unwrap_or_else(|| panic!("no line with {needle:?} in {lines:#?}"))
    };
    let first = text_col("ten");
    for needle in ["three digit id", "four digit", "u32 max"] {
        assert_eq!(text_col(needle), first, "{needle:?} starts in another column");
    }
    // The wrapped remainder of the long task keeps that same column.
    assert_eq!(text_col("onto a second line"), first);
    // Dates share their column too.
    assert_eq!(text_col("23-sep-26"), text_col("14-sep-26"));
    for line in &lines {
        assert!(cells(line) <= 80, "{} cells: {line}", cells(line));
    }

    // Single-digit ids keep the column the header was drawn for; the wide
    // one moves the whole line right by the eight digits it added, header
    // included.
    let narrow = Sandbox::with_db(r#"[{"id":1,"text":"ten"}]"#);
    let out = narrow.cmd().arg("list").output().unwrap();
    let narrow_lines = list_lines(&out);
    let col_in = |ls: &[String], needle: &str| -> usize {
        ls.iter()
            .find(|l| l.contains(needle))
            .map(|l| cells(&l[..l.find(needle).unwrap()]))
            .unwrap_or_else(|| panic!("no line with {needle:?} in {ls:#?}"))
    };
    let narrow_text = col_in(&narrow_lines, "ten");
    assert_eq!(first, narrow_text + 8);
    assert_eq!(
        col_in(&lines, "task") - first,
        col_in(&narrow_lines, "task") - narrow_text,
        "the header did not move with the column"
    );
}

/// REVIEW №10: a tab reached the editor buffer as a tab - drawn by the
/// terminal at its own tab stop, counted as one char by everything else. It
/// is expanded once on the way in (prefill, paste), so the text the editor
/// draws, measures and saves is the same string.
#[test]
fn r4_tabs_are_expanded_on_the_way_into_the_editor() {
    use rusk::cli::HandlerCLI;
    assert_eq!(
        HandlerCLI::split_multi_line_prefill("a\tb\n\tindented"),
        vec!["a    b".to_string(), "    indented".to_string()]
    );
}

/// REVIEW №34: the same wrapping serves `rusk add`, `mark`, `edit` and the
/// delete dialog. What `add` prints for a wide text fits the terminal too.
#[test]
fn r4_wide_text_fits_everywhere_a_task_is_printed() {
    let sb = Sandbox::new();
    let text = "日本語のテキスト".repeat(9);
    let out = sb.cmd().args(["add", &text]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        assert!(cells(line) <= 80, "add: {} cells: {line}", cells(line));
    }
    let out = sb.cmd().args(["mark", "1"]).output().unwrap();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        assert!(cells(line) <= 80, "mark: {} cells: {line}", cells(line));
    }
}

/// REVIEW №34 (the other half of the note): a dependency list too long for a
/// line of its own is wrapped like any other text instead of running off the
/// screen, and compact mode does not shave the task text down to a stub to
/// make room for it.
#[test]
fn r4_a_long_dependency_list_is_wrapped_too() {
    let mut db = String::from("[");
    let ids: Vec<u64> = (4294967280..4294967286).collect();
    for id in &ids {
        db.push_str(&format!(r#"{{"id":{id},"text":"dep {id}"}},"#));
    }
    let after = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
    db.push_str(&format!(
        r#"{{"id":4294967295,"text":"blocked by the big ids","after":[{after}]}}]"#
    ));
    let sb = Sandbox::with_db(&db);

    for args in [vec!["list"], vec!["list", "--compact"]] {
        let out = sb.cmd().args(&args).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
        let lines = list_lines(&out);
        for line in &lines {
            assert!(cells(line) <= 80, "{args:?}: {} cells: {line}", cells(line));
        }
        // The whole list of ids is there, and the task text is not chopped.
        let printed = lines.join(" ");
        assert!(printed.contains("blocked by the big ids"), "{args:?}: {printed}");
        for id in &ids {
            assert!(printed.contains(&id.to_string()), "{args:?}: {id} is missing");
        }
    }
}

/// REVIEW №35: the columns were aligned with `{:>2}` / `{:>9}` over styled
/// cells, and styling made those width specifiers no-ops - the fix is only
/// visible with colors on, which every other test here turns off.
#[test]
fn r4_colored_output_lines_up_like_the_plain_one() {
    let db = r#"[{"id":10,"text":"ten","date":"2026-09-23"},
                 {"id":1000,"text":"four digit","done":true,"date":"2026-09-14"},
                 {"id":4294967295,"text":"u32 max","priority":true}]"#;
    let sb = Sandbox::with_db(db);
    let plain = sb.cmd().arg("list").output().unwrap();
    let colored = sb
        .cmd()
        .env_remove("RUSK_NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .arg("list")
        .output()
        .unwrap();
    assert!(colored.status.success(), "{}", stderr_of(&colored));
    let strip = |out: &std::process::Output| -> Vec<String> {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(rusk::cli::HandlerCLI::strip_ansi_codes)
            .filter(|l| !l.trim().is_empty())
            .collect()
    };
    let colored_lines = strip(&colored);
    assert!(
        String::from_utf8_lossy(&colored.stdout).contains('\u{1b}'),
        "the run was not colored after all"
    );
    assert_eq!(strip(&plain), colored_lines, "colors moved the columns");
}

// ── R6: the terminal comes back, whatever ends the editor ─────────────────

/// REVIEW №50: `rusk edit 1` never checked for a terminal, the way
/// `rusk add` does. Into a pipe it died of a broken one after leaving the
/// shell in raw mode; into `/dev/null` it painted an invisible editor and
/// exited 0 on Esc, as though the task had been looked at.
#[test]
#[cfg(feature = "interactive")]
fn r6_interactive_edit_without_a_terminal_is_refused() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"buy milk","date":null,"done":false,"priority":false}]"#);
    let out = sb.cmd().args(["edit", "1"]).output().unwrap();

    assert!(!out.status.success(), "{}", stderr_of(&out));
    let stderr = stderr_of(&out);
    assert!(stderr.contains("requires a terminal"), "{stderr}");
    assert!(stderr.contains("rusk edit 1 buy oat milk"), "{stderr}");
    // The control: the same check `rusk add` has made all along.
    let add = sb.cmd().arg("add").output().unwrap();
    assert!(!add.status.success());
    assert!(stderr_of(&add).contains("requires a terminal"), "{}", stderr_of(&add));
}

// ── R7: task text does not drive the terminal ─────────────────────────────

/// Bytes a terminal acts on instead of printing: ESC, BEL, NUL and the
/// UTF-8 encoding of C1 (U+0080..U+009F, lead byte 0xC2).
fn terminal_controls(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    text.chars()
        .filter(|&c| c.is_control() && c != '\n')
        .map(|c| format!("U+{:04X}", c as u32))
        .collect()
}

const R7_HOSTILE: [&str; 4] = [
    "before \x1b[?25l after the escape",
    "\x1b]0;pwned title\x07 visible text",
    "csi \u{9b}2J one char, soh \x01 and bel \x07",
    "\x1b]0;evil\x07 hello mom",
];

/// REVIEW №38: control characters in a task went to the terminal as they
/// were — `list` hid the cursor and retitled the window, and the wrapper
/// that measured styled text swallowed everything up to the next `m`
/// (`Added task:` showed "before" and nothing else).
#[test]
fn r7_task_text_reaches_the_terminal_escaped() {
    let sb = Sandbox::new();
    for text in R7_HOSTILE {
        let out = sb.cmd().args(["add", text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
        assert_eq!(terminal_controls(&out.stdout), Vec::<String>::new(), "add {text:?}");
    }
    let first_added = sb.cmd().args(["add", R7_HOSTILE[0]]).output().unwrap();
    assert!(
        stdout_of(&first_added).contains(r"before \x1b[?25l after the escape"),
        "{}",
        stdout_of(&first_added)
    );

    let runs: [&[&str]; 6] = [
        &["list"],
        &["list", "--compact"],
        &["search", "visible"],
        &["mark", "2"],
        &["mark", "-p", "3"],
        &["edit", "4", "\x1b]0;evil\x07 hello again"],
    ];
    for args in runs {
        let out = sb.cmd().args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", stderr_of(&out));
        assert_eq!(terminal_controls(&out.stdout), Vec::<String>::new(), "{args:?}: {}", stdout_of(&out));
    }

    let list = stdout_of(&sb.cmd().arg("list").output().unwrap());
    for shown in [
        r"before \x1b[?25l after the escape",
        r"\x1b]0;pwned title\x07 visible text",
        r"csi \x9b2J one char, soh \x01 and bel \x07",
        r"\x1b]0;evil\x07 hello again",
    ] {
        assert!(list.contains(shown), "{shown:?} not in:\n{list}");
    }
    // The stored text is the text: only its printing is escaped.
    let db = std::fs::read_to_string(sb.db_path()).unwrap();
    assert!(db.contains(r"\u001b[?25l"), "{db}");
}

/// REVIEW №38, colored: styling is applied to the escaped text, so the
/// only sequences on the wire are rusk's own — and taking them out leaves
/// the whole text, not text cut at the first `m` after an ESC.
#[test]
fn r7_colored_output_carries_only_its_own_sequences() {
    let sb = Sandbox::new();
    for text in R7_HOSTILE {
        let out = sb.cmd().args(["add", text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    }
    for args in [&["list"][..], &["add", R7_HOSTILE[3]]] {
        let out = sb
            .cmd()
            .env_remove("RUSK_NO_COLOR")
            .env("CLICOLOR_FORCE", "1")
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
        let raw = stdout_of(&out);
        assert!(raw.contains('\x1b'), "the run was not colored after all");
        let plain: String = raw.lines().map(rusk::cli::HandlerCLI::strip_ansi_codes).collect();
        assert_eq!(terminal_controls(plain.as_bytes()), Vec::<String>::new(), "{args:?}: {raw:?}");
        assert!(plain.contains(r"\x1b]0;evil\x07 hello mom"), "{args:?}: {plain}");
    }
}

/// REVIEW №38: the renumbering warning quotes the task — through the same
/// escaping.
#[test]
fn r7_warnings_that_quote_a_task_escape_it() {
    let sb = Sandbox::with_db(
        r#"[{"id":1,"text":"one"},{"id":1,"text":"\u001b]0;evil\u0007 two"}]"#,
    );
    let out = sb.cmd().arg("list").output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let stderr = stderr_of(&out);
    assert!(stderr.contains(r"\x1b]0;evil\x07 two"), "{stderr}");
    assert_eq!(terminal_controls(&out.stderr), Vec::<String>::new(), "{stderr:?}");
}

/// Found by the review of R7: an error quotes the database — the line the
/// JSON parser stopped at (a raw ESC in a string is exactly what stops it),
/// a CSV cell — and went to stderr raw.
#[test]
fn r7_errors_that_quote_the_database_escape_it() {
    let sb = Sandbox::new();
    std::fs::write(sb.db_path(), "[{\"id\":1,\"text\":\"a\x1b]0;pwn\x07 b\",\"done\":false}]").unwrap();
    let out = sb.cmd().arg("list").output().unwrap();
    assert!(!out.status.success());
    let stderr = stderr_of(&out);
    assert!(stderr.contains(r"\x1b]0;pwn\x07"), "{stderr}");
    assert_eq!(terminal_controls(&out.stderr), Vec::<String>::new(), "{stderr:?}");
}

/// Found by the review of R7: search matched the raw text but showed the
/// escaped one, so `after tab` did not find the task shown as "after tab".
#[test]
fn r7_search_finds_what_it_shows() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"after\ttab"},{"id":2,"text":"esc \u001b here"}]"#);
    for (query, id) in [("after tab", "1"), (r"\x1b", "2")] {
        let out = sb.cmd().args(["search", query]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
        assert!(
            list_lines(&out).iter().any(|l| l.split_whitespace().nth(1) == Some(id)),
            "{query:?}: {}",
            stdout_of(&out)
        );
    }
}

// ── R15: the ssh protocol ─────────────────────────────────────────────────

/// A database that does not fit in one 64 KB pipe buffer — the size at
/// which writing to a tool before reading from it starts to matter.
#[cfg(all(unix, feature = "sync"))]
fn big_database() -> String {
    let many: Vec<String> = (1..=900)
        .map(|id| format!(r#"{{"id":{id},"text":"{}","date":null,"done":false,"priority":false}}"#, "x".repeat(240)))
        .collect();
    format!("[{}]", many.join(","))
}

/// REVIEW №181: everything the remote login shell printed was taken for
/// the file. In todo.txt a banner became a task and was written back (and
/// multiplied on every save); in JSON it turned a readable database into
/// "not a valid JSON task list".
#[test]
#[cfg(all(unix, feature = "sync", feature = "fmt-todotxt"))]
fn r15_a_banner_from_the_remote_shell_is_not_a_task() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"real task","date":null,"done":false,"priority":false}]"#);
    let remote = sb.path().join("remote.txt");
    // A login shell with an `echo` in its rc file: the classic thing that
    // breaks scp, and it talks on the way out as well as on the way in.
    let chatty = "#!/bin/sh\necho 'Welcome to vps! Last login: Sun Sep 20'\nshift\nsh -c \"$1\"\nstatus=$?\necho 'Have a nice day'\nexit $status\n";
    let location = format!("u@h:{}", remote.display());

    let push = sb
        .cmd_with_ssh_shim(chatty)
        .env("RUSK_SYNC_REMOTE", &location)
        .args(["sync", "push", "--force"])
        .output()
        .unwrap();
    assert!(push.status.success(), "{}", stderr_of(&push));
    let written = fs::read_to_string(&remote).unwrap();
    assert!(!written.contains("Welcome"), "the banner was stored: {written:?}");
    assert!(written.contains("real task"), "{written:?}");

    // ...and reading it back sees one task, not three.
    let pull = sb
        .cmd_with_ssh_shim(chatty)
        .env("RUSK_SYNC_REMOTE", &location)
        .args(["sync", "pull", "--force"])
        .output()
        .unwrap();
    assert!(pull.status.success(), "{}", stderr_of(&pull));
    let tasks = db_tasks(&sb);
    assert_eq!(tasks.len(), 1, "{tasks:?}");
    assert_eq!(text_of(&tasks, 1), "real task");
}

/// A remote that never ran the script — no `sh`, a login shell that
/// rejected the line — is an error showing what did come back, not an
/// empty database that the next save would write over the real one.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r15_a_remote_that_answers_with_something_else_is_an_error() {
    let sb = Sandbox::new();
    let mute = "#!/bin/sh\necho 'This account is currently not available.'\nexit 0\n";
    let out = sb
        .cmd_with_ssh_shim(mute)
        .env("RUSK_SYNC_REMOTE", "u@h:/srv/tasks.json")
        .args(["sync", "pull", "--force"])
        .output()
        .unwrap();

    assert!(!out.status.success(), "{}", stderr_of(&out));
    let stderr = stderr_of(&out);
    assert!(stderr.contains("not the file"), "{stderr}");
    assert!(stderr.contains("This account is currently not available"), "{stderr}");
}

/// REVIEW №25: a location ending in `/` is a directory, exactly as it is
/// locally. It used to produce `mv: '.../.tmp' and '.../.tmp' are the same
/// file` and leave the data in a `.tmp` nobody reads.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r15_a_trailing_slash_is_the_directory_not_a_nameless_file() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"hello","date":null,"done":false,"priority":false}]"#);
    let remote_dir = sb.path().join("remote/dir");
    let out = sb
        .cmd_with_fake_ssh()
        .env("RUSK_SYNC_REMOTE", format!("u@h:{}/", remote_dir.display()))
        .args(["sync", "push", "--force"])
        .output()
        .unwrap();

    assert!(out.status.success(), "{}", stderr_of(&out));
    let written = fs::read_to_string(remote_dir.join("tasks.json")).unwrap();
    assert!(written.contains("hello"), "{written}");
    let left: Vec<_> = fs::read_dir(&remote_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

/// REVIEW №180: README promises a backup on every save, and the ssh side
/// made none — so `rusk restore`'s advice to "restore from a backup on the
/// remote side" pointed at something that did not exist.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r15_the_remote_keeps_the_content_it_replaces() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"first","date":null,"done":false,"priority":false}]"#);
    let remote = sb.path().join("bk/t.json");
    let location = format!("u@h:{}", remote.display());
    let push = |sb: &Sandbox| {
        let out = sb
            .cmd_with_fake_ssh()
            .env("RUSK_SYNC_REMOTE", &location)
            .args(["sync", "push", "--force"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    };

    push(&sb);
    assert!(!remote.with_extension("json.backup").exists(), "nothing to keep yet");
    sb.write_db(r#"[{"id":1,"text":"second","date":null,"done":false,"priority":false}]"#);
    push(&sb);

    assert!(fs::read_to_string(&remote).unwrap().contains("second"));
    let backup = fs::read_to_string(remote.with_extension("json.backup")).unwrap();
    assert!(backup.contains("first"), "{backup}");
}

/// REVIEW №183: rusk wrote the whole database into ssh's stdin before it
/// read a word of what ssh said. Past the 64 KB pipe buffer, an ssh that
/// died early left the write with a broken pipe and that was all the user
/// got — "failed to stream data to ssh", with ssh's own explanation
/// thrown away.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r15_a_failing_ssh_still_gets_to_say_why_on_a_large_database() {
    let sb = Sandbox::with_db(&big_database());
    assert!(sb.read_db().len() > 64 * 1024, "the database fits in one pipe buffer");
    let remote = sb.path().join("remote.json");
    fs::write(&remote, "[]").unwrap();

    // Reads go through; the save is the one that cannot connect, and it
    // never reads its stdin. (A read-only remote whose shell cannot create
    // the temp file behaves the same way.)
    let dies_on_write = "#!/bin/sh\ncase \"$2\" in *mkdir*) echo 'ssh: connect to host vps port 22: Connection refused' >&2; exit 255 ;; esac\nshift\nexec sh -c \"$1\"\n";
    let out = sb
        .cmd_with_ssh_shim(dies_on_write)
        .env("RUSK_SYNC_REMOTE", format!("u@h:{}", remote.display()))
        .args(["sync", "push", "--force"])
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = stderr_of(&out);
    assert!(stderr.contains("Connection refused"), "{stderr}");
    assert!(!stderr.contains("failed to stream data"), "{stderr}");
}

/// The other half of №183: an ssh that talks more than one pipe buffer
/// full before it reads its stdin used to deadlock — rusk waiting to
/// finish writing, ssh waiting to finish complaining.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r15_a_very_chatty_ssh_does_not_hang_rusk() {
    let sb = Sandbox::with_db(&big_database());
    // 200 KB of debug chatter on stderr, then the command, as `ssh -vvv`
    // on a slow link would.
    let noisy = "#!/bin/sh\nawk 'BEGIN { line = sprintf(\"%*s\", 200, \"\"); for (i = 0; i < 1000; i++) print \"debug3:\" line }' >&2\nshift\nexec sh -c \"$1\"\n";
    let remote = sb.path().join("noisy.json");

    let out = sb
        .cmd_with_ssh_shim(noisy)
        .env("RUSK_SYNC_REMOTE", format!("u@h:{}", remote.display()))
        .args(["sync", "push", "--force"])
        .output()
        .unwrap();

    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(fs::read_to_string(&remote).unwrap().contains("xxx"));
}

// ---------------------------------------------------------------------------
// R11 — quoting in the completion scripts, one reading of --for-completion-lines
// ---------------------------------------------------------------------------
//
// `rusk edit <id><TAB>` puts the text of the task on the command line, and
// running that line stores the very same text. `rusk list --for-completion-lines`
// is one line per task (the text escaped), so no line of a text passes for
// a task; every script quotes whatever the shell would otherwise split,
// expand or trim. fish inserts one token per Tab and escapes it itself: there
// the id completes first and the next Tab gives the text, and nothing
// rebinds Tab. Each test drives the real script in its shell (no user
// config) against a fake `rusk` that lists the sandbox's tasks and records
// the arguments the completed line would run with; a shell that is not
// installed is skipped.

/// The texts every script must carry through a completed command line,
/// task ids 1.. in this order.
#[cfg(unix)]
const R11_TEXTS: &[&str] = &[
    "task one",
    // №60: a line that looks like a task of its own.
    "two first\n1\thijack of one",
    // №11: lines, a `$`, indentation.
    "first line\nsecond line with $x\n  third indented",
    // №61, №64: a tab, runs of spaces.
    "tabs\there  double  spaces",
    "  leading and trailing spaces  ",
    // №63
    "-d looks like flag",
    // №62
    "it's \"quoted\" $HOME",
    "it's a back\\slash and \"dq\"",
    "literal \\n and \\t, not escapes",
    "curly it\u{2019}s and \u{2018}quotes\u{2019} \u{201c}too\u{201d}",
    "~/not/home *.glob {a,b} #hash (paren) [br] a;b|c&d",
    "windows\r\nline and a trailing newline\n",
    // Whitespace PowerShell splits at; a combining mark first (culture-aware
    // `StartsWith` missed the line and the dash); a control character.
    "no-break\u{a0}space and ideographic\u{3000}space",
    "\u{301}combining first",
    "-\u{301}x dash and a combining mark",
    "bell\u{7} inside",
];

/// A fake `rusk` first on PATH: `list --for-completion-lines` prints what the
/// real one prints for [`R11_TEXTS`]; `edit` records its arguments,
/// NUL-separated, in `<dir>/args.<id>`.
#[cfg(unix)]
struct FakeRusk {
    dir: tempfile::TempDir,
    path: String,
}

#[cfg(unix)]
impl FakeRusk {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let tasks: Vec<Task> = R11_TEXTS
            .iter()
            .enumerate()
            .map(|(i, text)| task(i as u32 + 1, text))
            .collect();
        let sb = Sandbox::with_db(&serde_json::to_string(&tasks).unwrap());
        let out = sb.cmd().args(["list", "--for-completion-lines"]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("listing"), &out.stdout).unwrap();
        let rusk = dir.path().join("rusk");
        fs::write(
            &rusk,
            format!(
                "#!/bin/sh\n\
                 [ \"$1 $2\" = 'list --for-completion-lines' ] && exec cat '{d}/listing'\n\
                 [ \"$1\" = edit ] && printf '%s\\0' \"$@\" > '{d}/args.'\"$2\"\n\
                 exit 0\n",
                d = dir.path().display()
            ),
        )
        .unwrap();
        fs::set_permissions(&rusk, fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:{}", dir.path().display(), std::env::var("PATH").unwrap_or_default());
        Self { dir, path }
    }

    /// The text `rusk edit <id> …` was run with, as rusk joins it: `None`
    /// when the line was never run.
    fn text_run_for(&self, id: usize) -> Option<String> {
        let raw = fs::read(self.dir.path().join(format!("args.{id}"))).ok()?;
        let raw = String::from_utf8(raw).unwrap();
        let args: Vec<&str> = raw.strip_suffix('\0').unwrap_or(&raw).split('\0').collect();
        assert_eq!(args[..2], ["edit", id.to_string().as_str()], "{args:?}");
        let words = match args.get(2) {
            Some(&"--") => &args[3..],
            _ => &args[2..],
        };
        Some(words.join(" "))
    }

    /// Every text came back as it is. `may_decline` names the texts a
    /// shell cannot put on a line and must then leave off it.
    fn assert_round_trips(&self, shell: &str, may_decline: fn(&str) -> bool) {
        for (i, text) in R11_TEXTS.iter().enumerate() {
            match self.text_run_for(i + 1) {
                Some(run) => assert_eq!(run, *text, "{shell}: task {}", i + 1),
                None => assert!(may_decline(text), "{shell}: nothing completed for task {}: {text:?}", i + 1),
            }
        }
    }
}

#[cfg(unix)]
fn never(_: &str) -> bool {
    false
}

/// №60, and the format every script reads: one line per task, whatever
/// the text holds.
#[test]
fn r11_the_completion_listing_has_one_line_per_task() {
    let tasks: Vec<Task> = [
        "task one",
        "two first\n1\thijack of one",
        "back\\slash\r\nand\tmore\n",
    ]
    .iter()
    .enumerate()
    .map(|(i, text)| task(i as u32 + 1, text))
    .collect();
    let sb = Sandbox::with_db(&serde_json::to_string(&tasks).unwrap());
    let out = sb.cmd().args(["list", "--for-completion-lines"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "1\ttask one\n2\ttwo first\\n1\\thijack of one\n3\tback\\\\slash\\r\\nand\\tmore\\n\n"
    );
    // The scripts installed by an older rusk stay as they were: an upgrade
    // of the binary does not replace them, and they cannot read the escaped
    // form (they would store `\\` for a backslash).
    let out = sb.cmd().args(["list", "--for-completion"]).output().unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "1\ttask one\n2\ttwo first\n1\thijack of one\n3\tback\\slash\nand\tmore\n"
    );
}

#[test]
#[cfg(unix)]
fn r11_bash_completion_round_trips_every_text() {
    let fake = FakeRusk::new();
    let script = format!(
        r#"source {COMPLETIONS}/rusk.bash
for id in $(seq {n}); do
    COMP_WORDS=(rusk edit "$id"); COMP_CWORD=2; COMP_LINE="rusk edit $id"; COMPREPLY=()
    _rusk_completion
    [ "${{#COMPREPLY[@]}}" -gt 0 ] && eval "rusk edit ${{COMPREPLY[0]}}"
done
exit 0
"#,
        n = R11_TEXTS.len()
    );
    if run_shell("bash", &["--norc", "--noprofile", "-c", &script], &fake.path).is_some() {
        fake.assert_round_trips("bash", never);
    }
}

#[test]
#[cfg(unix)]
fn r11_zsh_completion_round_trips_every_text() {
    let fake = FakeRusk::new();
    let script = format!(
        r#"_RUSK_ZSH_SKIP_ENTRY=1 source {COMPLETIONS}/rusk.zsh
compadd() {{ local -a a=("$@"); reply+=("${{(@)a[${{a[(i)--]}}+1,-1]}}"); }}
for id in {{1..{n}}}; do
    words=(rusk edit "$id"); CURRENT=3; LBUFFER="rusk edit $id"; reply=()
    _rusk
    (( ${{#reply}} )) && eval "rusk edit ${{reply[1]}}"
done
exit 0
"#,
        n = R11_TEXTS.len()
    );
    if run_shell("zsh", &["-f", "-c", &script], &fake.path).is_some() {
        fake.assert_round_trips("zsh", never);
    }
}

/// fish escapes a candidate as it inserts it, one token per Tab: `rusk edit
/// <id><TAB>` completes the id, the next Tab the text, after `--` when it
/// starts with `-`. A text with a tab cannot be a fish candidate (the tab
/// starts its description), and fish leaves a leading `~` unescaped: those
/// are left off rather than cut or expanded.
#[test]
#[cfg(unix)]
fn r11_fish_completion_round_trips_every_text() {
    let fake = FakeRusk::new();
    let script = format!(
        r#"source {COMPLETIONS}/rusk.fish
function first_candidate
    complete --escape -C "$argv[1]" | string split -m 1 \t | head -n 1
end
for id in (seq {n})
    test (first_candidate "rusk edit $id") = $id; or echo "id $id did not complete to itself"
    set -l line "rusk edit $id "
    set -l cand (first_candidate $line)
    if test "$cand" = --
        set line "$line-- "
        set cand (first_candidate $line)
    else if string match -q -- '-*' "$cand"
        # The flags: there is no text to offer.
        continue
    end
    test -n "$cand"; and eval "$line$cand"
end
"#,
        n = R11_TEXTS.len()
    );
    if let Some(out) = run_shell("fish", &["-N", "-c", &script], &fake.path) {
        assert_eq!(out, "", "fish");
        fake.assert_round_trips("fish", |text| text.contains('\t') || text.starts_with('~'));
    }
}

#[test]
#[cfg(unix)]
fn r11_nu_completion_round_trips_every_text() {
    let fake = FakeRusk::new();
    let script = format!(
        r#"use {COMPLETIONS}/rusk.nu *
print -n (1..{n} | each {{|id| rusk-completions-main [rusk edit ($id | into string)] | get 0?.value? | default "" }} | str join (char nul))
"#,
        n = R11_TEXTS.len()
    );
    let Some(out) = run_shell("nu", &["-n", "-c", &script], &fake.path) else { return };
    for (i, value) in out.split('\0').enumerate() {
        if value.is_empty() {
            continue;
        }
        let run = std::process::Command::new("nu")
            .args(["-n", "-c", &format!("^rusk edit {value}")])
            .env("PATH", &fake.path)
            .output()
            .unwrap();
        assert!(run.status.success(), "nu: task {}: {value:?}: {}", i + 1, stderr_of(&run));
    }
    // The line editor drops control characters other than tab and line feed
    // from what it inserts: such a text is not offered.
    fake.assert_round_trips("nu", |text| {
        text.chars().any(|c| c < ' ' && c != '\t' && c != '\n')
    });
}

#[test]
#[cfg(unix)]
fn r11_powershell_completion_round_trips_every_text() {
    let fake = FakeRusk::new();
    let script = format!(
        r#". {COMPLETIONS}/rusk.ps1
foreach ($id in 1..{n}) {{
    $line = "rusk edit $id"
    $r = TabExpansion2 -inputScript $line -cursorColumn $line.Length
    if ($r.CompletionMatches.Count -gt 0) {{
        Invoke-Expression ("rusk edit " + $r.CompletionMatches[0].CompletionText)
    }}
}}
"#,
        n = R11_TEXTS.len()
    );
    if run_shell("pwsh", &["-NoProfile", "-NonInteractive", "-Command", &script], &fake.path).is_some() {
        fake.assert_round_trips("pwsh", never);
    }
}

/// The text is offered next to the one id `rusk edit` takes, and nowhere
/// else: not for the value of `-a`, not after ids that follow `--`, not for
/// another command that happens to have an `e` among its words — and while
/// `--` is typed without its space, not the flags either (`--after` would
/// land where the text belongs).
#[test]
#[cfg(unix)]
fn r11_fish_offers_the_text_only_next_to_the_id() {
    let (_dir, path) = fake_rusk_on_path();
    let script = format!(
        r#"source {COMPLETIONS}/rusk.fish
for line in 'rusk edit -a 1 ' 'rusk edit -- 2 ' 'rusk add buy e 2 ' 'rusk search e 2 ' 'rusk edit -a 1 2 ' 'rusk edit 1 --' 'RUSK_DB=x rusk e 2 '
    printf '%s => %s\n' $line (complete --escape -C $line | string replace -r '\t.*' '' | string join '|')
end
"#
    );
    let Some(out) = run_shell("fish", &["-N", "-c", &script], &path) else { return };
    let got: Vec<&str> = out.lines().map(|l| l.split(" => ").nth(1).unwrap_or("")).collect();
    for (i, line) in got.iter().enumerate().take(4) {
        assert!(!line.contains("plain") && !line.contains("means"), "line {i}: {out}");
    }
    assert_eq!(got[4], "plain\\ text", "{out}");
    assert_eq!(got[5], "--", "{out}");
    assert_eq!(got[6], "plain\\ text", "{out}");
}

/// №164: loading the fish script took over Tab for the whole session — a
/// binding of the user's own was lost, and any command line that mentioned
/// `rusk edit <N>` was rewritten.
#[test]
#[cfg(unix)]
fn r11_fish_completion_leaves_the_key_bindings_alone() {
    let (_dir, path) = fake_rusk_on_path();
    let script = format!(
        r#"bind \t __my_tab
source {COMPLETIONS}/rusk.fish
complete -C 'rusk edit 1' > /dev/null
bind \t
"#
    );
    if let Some(out) = run_shell("fish", &["-N", "-c", &script], &path) {
        assert!(out.contains("__my_tab"), "{out}");
        assert!(!out.contains("__rusk"), "{out}");
    }
}

// ---------------------------------------------------------------------------
// R18 — the lifecycle of a SQLite connection
// ---------------------------------------------------------------------------
//
// Reading a SQLite database changes nothing in it: no table is created, no
// column added, a write-protected file reads fine, and a SQLite file of
// another program is refused rather than furnished with a `tasks` table.
// The schema is brought up to date by the write that needs it (the legacy
// `CHECK (id BETWEEN 1 AND 255)` included); `.backup` is a copy SQLite makes
// of the committed database (a WAL included), and deleted text does not
// stay behind in the file. A debug binary is pinned to a JSON file, so these
// drive the backend; REVIEW.md has the black-box runs of a release binary.

#[cfg(feature = "backend-sqlite")]
fn tables_of(db: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect()
}

/// A database as rusk created it before ids were widened to u32 and before
/// `after` existed.
#[cfg(feature = "backend-sqlite")]
fn legacy_sqlite(db: &Path) {
    rusqlite::Connection::open(db)
        .unwrap()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS tasks (
                pos      INTEGER PRIMARY KEY,
                id       INTEGER NOT NULL UNIQUE CHECK (id BETWEEN 1 AND 255),
                text     TEXT NOT NULL,
                date     TEXT,
                done     INTEGER NOT NULL DEFAULT 0,
                priority INTEGER NOT NULL DEFAULT 0
            );
            INSERT INTO tasks (pos, id, text) VALUES (1, 1, 'legacy one'), (2, 7, 'legacy seven');",
        )
        .unwrap();
}

/// REVIEW №159: `rusk list` created a `tasks` table in a SQLite file of
/// another program and answered "No tasks".
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_a_foreign_sqlite_file_is_refused_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("foreign.db");
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch(
            "CREATE TABLE bookmarks (url TEXT); INSERT INTO bookmarks VALUES ('https://x');
             PRAGMA user_version = 42;",
        )
        .unwrap();
    let before = fs::read(&db).unwrap();

    let err = chain_of(&backend(&db).load().unwrap_err());
    assert!(err.contains("foreign.db") && err.contains("no tasks table"), "{err}");
    // Not even a save that was never told what the file held writes there.
    let err = chain_of(&backend(&db).save(&[task(1, "must not land here")]).unwrap_err());
    assert!(err.contains("no tasks table"), "{err}");
    assert!(fs::read(&db).unwrap() == before, "the foreign file was changed");
    assert_eq!(tables_of(&db), ["bookmarks"]);
}

/// A SQLite file without any table (another tool only created it) is an
/// empty rusk database: nothing to list, and the first save furnishes it.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_a_sqlite_file_without_tables_is_an_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    rusqlite::Connection::open(&db).unwrap().execute_batch("PRAGMA user_version = 1;").unwrap();
    let before = fs::read(&db).unwrap();

    let b = backend(&db);
    assert!(b.load().unwrap().is_empty());
    assert!(fs::read(&db).unwrap() == before, "a read changed the file");
    b.save(&[task(1, "first")]).unwrap();
    assert_eq!(backend(&db).load().unwrap(), [task(1, "first")]);
}

/// REVIEW №159: a write-protected database of the old schema (no `after`
/// column) could not even be listed — reading tried to add the column.
#[test]
#[cfg(all(unix, feature = "backend-sqlite"))]
fn r18_a_read_only_legacy_database_is_read() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    legacy_sqlite(&db);
    fs::set_permissions(&db, fs::Permissions::from_mode(0o444)).unwrap();
    if fs::OpenOptions::new().write(true).open(&db).is_ok() {
        eprintln!("skipping: running as a user that ignores file modes (root?)");
        return;
    }

    let b = backend(&db);
    let loaded = b.load().unwrap();
    assert_eq!(ids_and_texts(&loaded), [(1, "legacy one"), (7, "legacy seven")]);
    // Writing is refused as what it is.
    let err = chain_of(&b.save(&[task(1, "x")]).unwrap_err());
    assert!(err.contains("read-only"), "{err}");
}

/// Reading an old database leaves its schema alone; the write brings it up
/// to date.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_reading_does_not_migrate_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    legacy_sqlite(&db);
    let before = fs::read(&db).unwrap();

    let loaded = backend(&db).load().unwrap();
    assert_eq!(ids_and_texts(&loaded), [(1, "legacy one"), (7, "legacy seven")]);
    assert!(fs::read(&db).unwrap() == before, "a read changed the file");
}

/// REVIEW №28: the legacy `CHECK (id BETWEEN 1 AND 255)` stayed, so a
/// database created before ids were widened could not store task 256 — and
/// every later save failed with it.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_a_legacy_database_takes_ids_above_255() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    legacy_sqlite(&db);

    let b = backend(&db);
    let mut tasks = b.load().unwrap();
    tasks.push(Task { after: vec![7], ..task(256, "wide id") });
    b.save(&tasks).unwrap();

    let b = backend(&db);
    let mut loaded = b.load().unwrap();
    assert_eq!(
        ids_and_texts(&loaded),
        [(1, "legacy one"), (7, "legacy seven"), (256, "wide id")]
    );
    assert_eq!(loaded[2].after, [7]);
    // And the save after that.
    loaded.push(task(100_000, "wider"));
    b.save(&loaded).unwrap();
    assert_eq!(backend(&db).load().unwrap().len(), 4);
}

/// REVIEW №102: `.backup` was a byte copy of the main file, which misses
/// what is committed in a WAL — and a WAL database gets written by other
/// tools, or when the user switches the journal mode.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_the_backup_holds_what_is_committed_in_a_wal() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    backend(&db).save(&[task(1, "one")]).unwrap();

    // Another program works in WAL mode and has not checkpointed yet.
    let other = rusqlite::Connection::open(&db).unwrap();
    other
        .execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
             INSERT INTO tasks (pos, id, text) VALUES (2, 2, 'only in the wal');",
        )
        .unwrap();

    let b = backend(&db);
    let mut tasks = b.load().unwrap();
    tasks.push(task(3, "three"));
    b.save(&tasks).unwrap();

    // The backup is the state before the save: both tasks, and on its own
    // — it needs no WAL to be read.
    let backup = dir.path().join("tasks.db.backup");
    let conn = rusqlite::Connection::open_with_flags(
        &backup,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let texts: Vec<String> = conn
        .prepare("SELECT text FROM tasks ORDER BY pos")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(texts, ["one", "only in the wal"]);
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0)).unwrap();
    assert_ne!(mode, "wal", "the backup needs a WAL of its own to be read");
    drop(other);
}

/// The copy `rusk restore` keeps of the database it replaces is made the
/// same way: what a WAL holds is not lost with the restore.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_restore_keeps_what_is_committed_in_a_wal() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    backend(&db).save(&[task(1, "one")]).unwrap();
    let b = backend(&db);
    b.load().unwrap();
    b.save(&[task(1, "one"), task(2, "two")]).unwrap();

    let other = rusqlite::Connection::open(&db).unwrap();
    other
        .execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
             INSERT INTO tasks (pos, id, text) VALUES (3, 3, 'only in the wal');",
        )
        .unwrap();

    assert_eq!(backend(&db).restore_from_backup().unwrap(), [task(1, "one")]);
    let kept = SqliteKept::open(&dir.path().join("tasks.db.before_restore"));
    assert_eq!(kept.texts(), ["one", "two", "only in the wal"]);
    drop(other);
}

/// A database rusk cannot make tasks of (a date that is none) is still one
/// SQLite reads: the copy is SQLite's too, and holds what only a WAL has.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_restore_over_rows_rusk_cannot_read_keeps_the_wal() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    backend(&db).save(&[task(1, "one")]).unwrap();
    let b = backend(&db);
    b.load().unwrap();
    b.save(&[task(1, "one"), task(2, "two")]).unwrap();

    let other = rusqlite::Connection::open(&db).unwrap();
    other
        .execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
             INSERT INTO tasks (pos, id, text) VALUES (3, 3, 'good, only in the wal');
             INSERT INTO tasks (pos, id, text, date) VALUES (4, 4, 'bad', 'garbage');",
        )
        .unwrap();
    assert!(backend(&db).load().is_err());

    assert_eq!(backend(&db).restore_from_backup().unwrap(), [task(1, "one")]);
    let kept = rusqlite::Connection::open_with_flags(
        dir.path().join("tasks.db.before_restore"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let texts: Vec<String> = kept
        .prepare("SELECT text FROM tasks ORDER BY pos")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(texts, ["one", "two", "good, only in the wal", "bad"]);
    drop(other);
}

/// A restore that is refused leaves the database as it was — and so no
/// copy of it behind to pile up with every retry: here over a file of
/// another program.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_a_refused_restore_leaves_no_copy() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    backend(&db).save(&[task(1, "one")]).unwrap();
    let b = backend(&db);
    b.load().unwrap();
    b.save(&[task(1, "two")]).unwrap();
    fs::remove_file(&db).unwrap();
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("CREATE TABLE bookmarks (url TEXT);")
        .unwrap();

    let err = chain_of(&backend(&db).restore_from_backup().unwrap_err());
    assert!(err.contains("no tasks table"), "{err}");
    assert!(!dir.path().join("tasks.db.before_restore").exists());
    assert_eq!(tables_of(&db), ["bookmarks"]);
}

/// SQL table names know no case: a `Tasks` table is the table, as it was
/// before reading learned to tell rusk's database from another program's.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_the_tasks_table_is_found_in_any_case() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch(
            "CREATE TABLE Tasks (pos INTEGER PRIMARY KEY, id INTEGER, text TEXT, date TEXT,
                                 done INTEGER, priority INTEGER, After TEXT);
             INSERT INTO Tasks (pos, id, text, After) VALUES (1, 1, 'a', NULL), (2, 2, 'b', '1');",
        )
        .unwrap();
    let b = backend(&db);
    let mut tasks = b.load().unwrap();
    assert_eq!(tasks[1].after, [1]);
    tasks.push(task(3, "c"));
    b.save(&tasks).unwrap();
    assert_eq!(backend(&db).load().unwrap(), tasks);
}

/// A copy of a SQLite database, read the way `rusk restore` reads one.
#[cfg(feature = "backend-sqlite")]
struct SqliteKept(Vec<Task>);

#[cfg(feature = "backend-sqlite")]
impl SqliteKept {
    fn open(path: &Path) -> Self {
        Self(rusk::backend::sqlite::SqliteBackend::new(path.to_path_buf()).load_backup().unwrap())
    }

    fn texts(&self) -> Vec<&str> {
        self.0.iter().map(|t| t.text.as_str()).collect()
    }
}

/// REVIEW №185: the text of a deleted task stayed in the free space of
/// `tasks.db` and was copied into `.backup` with it.
#[test]
#[cfg(feature = "backend-sqlite")]
fn r18_deleted_text_does_not_stay_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("tasks.db");
    let secret = format!("SECRETPASSWORD-hunter2 {}", "q".repeat(3000));
    // One process per command, as the CLI runs them.
    let change = |f: &dyn Fn(&mut Vec<Task>)| {
        let b = backend(&db);
        let mut tasks = b.load().unwrap();
        f(&mut tasks);
        b.save(&tasks).unwrap();
    };
    change(&|t| t.push(task(1, &secret)));
    for id in 2..=5 {
        change(&|t| t.push(task(id, &format!("task {id}"))));
    }
    change(&|t| t.retain(|t| t.id != 1 && t.id != 3));
    change(&|t| t.push(task(1, "after the delete")));
    change(&|t| t.push(task(3, "one more")));

    let holds = |path: &Path| fs::read(path).unwrap().windows(14).any(|w| w == b"SECRETPASSWORD");
    assert!(!holds(&db), "the deleted text is still in the database file");
    assert!(!holds(&dir.path().join("tasks.db.backup")), "the deleted text is in the backup");
}

// ---------------------------------------------------------------------------
// R19 — output, terminals and messages of the CLI
// ---------------------------------------------------------------------------
//
// What a command prints may go to a reader that stops reading (`| head`), to
// a full disk, to a script without a terminal or to a terminal that asked
// for no colors; and what it says has to be what it did. The pure parts —
// the search folding, the wrapped layout with its highlights, the compact
// line, the color policy — are unit-tested next to their code; what only a
// terminal shows (an empty NO_COLOR in a pty, "Edited task" after a failed
// save in the editor) was checked in a pty against a release binary (see
// REVIEW.md).

/// `cmd` run with its standard output a pipe that nobody reads: every write
/// fails with EPIPE, as under `rusk list | head -2` once `head` is done.
#[cfg(unix)]
fn with_closed_stdout(cmd: &mut std::process::Command) -> std::process::Output {
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    cmd.stdout(writer).output().unwrap()
}

/// `cmd` in a session of its own, stdin at EOF: no terminal to ask on, not
/// even the controlling one crossterm falls back on (`/dev/tty`).
#[cfg(unix)]
fn without_terminal(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::unix::process::CommandExt;
    // SAFETY: setsid(2) is async-signal-safe.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.stdin(std::process::Stdio::null())
}

/// First line of what `rusk <args>` printed; the run must succeed.
fn first_line_of(sb: &Sandbox, args: &[&str]) -> String {
    let out = sb.cmd().args(args).output().unwrap();
    assert!(out.status.success(), "{args:?}: {}", stderr_of(&out));
    stdout_of(&out).lines().next().unwrap_or_default().to_string()
}

/// Ids `rusk search --id <query>` prints.
fn search_ids(sb: &Sandbox, query: &str) -> Vec<String> {
    let out = sb.cmd().args(["search", "--id", query]).output().unwrap();
    assert!(out.status.success(), "{query:?}: {}", stderr_of(&out));
    stdout_of(&out).lines().map(str::to_string).collect()
}

/// Text column of the task rows of `rusk list` (ids below 10, no dates).
fn listed_texts(out: &std::process::Output) -> Vec<String> {
    list_lines(out)
        .iter()
        .skip(1)
        .map(|line| line.chars().skip(19).collect())
        .collect()
}

/// `rusk search <query>` with colors forced on.
fn search_colored(sb: &Sandbox, query: &[&str]) -> String {
    let out = sb
        .cmd()
        .env_remove("RUSK_NO_COLOR")
        .env_remove("NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .arg("search")
        .args(query)
        .output()
        .unwrap();
    assert!(out.status.success(), "{query:?}: {}", stderr_of(&out));
    stdout_of(&out)
}

/// The parts of `line` printed in the search highlight (theme
/// `search_match`: bold yellow by default), in order.
fn highlights(line: &str) -> Vec<String> {
    const START: &str = "\x1b[1;33m";
    let mut found = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find(START) {
        let lit = &rest[at + START.len()..];
        let end = lit.find("\x1b[0m").expect("an unterminated highlight");
        found.push(lit[..end].to_string());
        rest = &lit[end..];
    }
    found
}

/// REVIEW №40: `rusk list | head -2` ended in "failed printing to stdout:
/// Broken pipe", a panic with exit 101 — `mark`, `edit`, `add` and
/// `gen -o -` too, after their change was already saved. A reader that
/// stops reading wants no more; that is no failure of the command.
#[test]
#[cfg(unix)]
fn r19_a_reader_that_goes_away_ends_the_command_quietly() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let mut runs: Vec<&[&str]> = vec![
        &["list"],
        &["search", "task"],
        &["mark", "2"],
        &["edit", "3", "renamed"],
        &["add", "fourth"],
    ];
    if cfg!(feature = "web") {
        runs.push(&["gen", "-o", "-"]);
    }
    for args in runs {
        let out = with_closed_stdout(sb.cmd().args(args));
        let err = stderr_of(&out);
        assert!(!err.contains("panicked"), "{args:?}: {err}");
        assert_eq!(out.status.code(), Some(0), "{args:?}: {err}");
        assert!(err.trim().is_empty(), "{args:?}: {err}");
    }
    // What was asked for was done all the same.
    let tasks = db_tasks(&sb);
    assert!(done_of(&tasks, 2));
    assert_eq!(text_of(&tasks, 3), "renamed");
    assert_eq!(text_of(&tasks, 4), "fourth");
}

/// An output that cannot take the text (a full disk) is an error of the
/// command, named as such — not a panic.
#[test]
#[cfg(target_os = "linux")]
fn r19_output_that_cannot_be_written_is_an_error() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let full = fs::OpenOptions::new().write(true).open("/dev/full").unwrap();
    let out = sb.cmd().arg("list").stdout(full).output().unwrap();
    let err = stderr_of(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(!err.contains("panicked"), "{err}");
    assert!(err.contains("standard output"), "{err}");
    assert!(err.contains("No space left on device"), "{err}");
}

/// REVIEW №69: `rusk del` without a terminal printed its question, then
/// died with "Failed to enable raw mode" (piping `y` into it as well), and
/// there was no way to delete from a script.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r19_del_without_a_terminal_asks_for_yes_instead() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    sb.cmd().args(["mark", "3"]).output().unwrap();
    for args in [&["del", "1"][..], &["del", "--done"], &["del", "1,9"]] {
        let out = without_terminal(sb.cmd().args(args)).output().unwrap();
        let err = stderr_of(&out);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {err}");
        assert!(err.contains("--yes"), "{args:?}: {err}");
        assert!(!err.contains("raw mode"), "{args:?}: {err}");
        // No question that nobody can answer.
        assert!(!stdout_of(&out).contains("[y/N]"), "{args:?}: {}", stdout_of(&out));
    }
    assert_eq!(db_tasks(&sb).len(), 3, "nothing may be deleted");

    // Nothing to confirm, nothing to ask.
    let out = without_terminal(sb.cmd().args(["del", "9"])).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("not found"), "{}", stdout_of(&out));
}

/// `--yes` deletes without asking — in every build, so a script works with
/// any of them.
#[test]
#[cfg(unix)]
fn r19_del_yes_deletes_without_asking() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    sb.cmd().args(["mark", "3"]).output().unwrap();
    let out = without_terminal(sb.cmd().args(["del", "1", "--yes"])).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Deleted 1 task"), "{}", stdout_of(&out));
    let out = without_terminal(sb.cmd().args(["del", "--done", "-y"])).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Deleted 1 done task"), "{}", stdout_of(&out));
    let tasks = db_tasks(&sb);
    assert_eq!(tasks.len(), 1, "{tasks:?}");
    assert_eq!(text_of(&tasks, 2), "second task");
}

/// REVIEW №36: `rusk search` listed a match but lit it up only where one
/// printed line held all of it — not across the wrap, not in a word cut in
/// two, not over a run of spaces the list prints as one.
#[test]
fn r19_search_highlights_what_it_found_wherever_it_is_printed() {
    let sb = Sandbox::new();
    for text in [
        // 57 cells for text on 80 columns: the row breaks after "alpha".
        format!("{} alpha beta gamma", "x".repeat(50)),
        "double  space alpha  beta here".to_string(),
        // One word of 110 cells, cut after "nee".
        format!("{}needle{}", "a".repeat(54), "b".repeat(50)),
    ] {
        let out = sb.cmd().args(["add", &text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    }
    for query in [&["alpha", "beta"][..], &["alpha  beta"], &["ALPHA", "  Beta "]] {
        let out = search_colored(&sb, query);
        let lit: Vec<String> = out.lines().flat_map(highlights).collect();
        assert_eq!(lit, ["alpha", "beta", "alpha beta"], "{query:?}: {out}");
    }
    let out = search_colored(&sb, &["needle"]);
    let lit: Vec<String> = out.lines().flat_map(highlights).collect();
    assert_eq!(lit, ["nee", "dle"], "{out}");
}

/// REVIEW №37: the query was lowercased as a string (a final Σ became ς)
/// and the text char by char (σ): a Greek word in capitals did not find
/// itself.
#[test]
fn r19_search_folds_the_query_and_the_text_alike() {
    let sb = Sandbox::new();
    for text in ["ΟΔΟΣ ΕΡΜΟΥ", "οδός ερμού", "Hauptstraße 5"] {
        let out = sb.cmd().args(["add", text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    }
    assert_eq!(search_ids(&sb, "ΟΔΟΣ"), ["1"]);
    assert_eq!(search_ids(&sb, "οδοσ"), ["1"]);
    assert_eq!(search_ids(&sb, "ΟΔΌΣ"), ["2"]);
    assert_eq!(search_ids(&sb, "οδός"), ["2"]);
    assert_eq!(search_ids(&sb, "HAUPTSTRASSE"), ["3"]);
}

/// REVIEW №74: `mark` named the state a task ended in, done before
/// priority: taking the priority off said "undone", and on a done task both
/// `-p` toggles said "done".
#[test]
fn r19_mark_says_what_it_changed() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    for (args, said) in [
        (&["mark", "1", "-p"][..], "Marked task as priority: 1"),
        (&["mark", "1", "-p"], "Removed priority from task: 1"),
        (&["mark", "2"], "Marked task as done: 2"),
        (&["mark", "2", "-p"], "Marked task as priority: 2"),
        (&["mark", "2", "-p"], "Removed priority from task: 2"),
        (&["mark", "2"], "Marked task as undone: 2"),
    ] {
        let line = first_line_of(&sb, args);
        assert!(line.starts_with(said), "{args:?}: {line}");
    }
}

/// REVIEW №78: `rusk add --help` offers `-d _` to clear a date, and
/// `rusk add text -d _` failed with "Invalid date '_'". For a new task it
/// means what it means everywhere: no date.
#[test]
fn r19_add_takes_underscore_for_no_date() {
    let sb = Sandbox::new();
    let out = sb.cmd().args(["add", "buy", "milk", "-d", "_"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "buy milk");
    assert_eq!(date_of(&tasks, 1), serde_json::Value::Null);
}

/// REVIEW №101, what R1 left: a backup that cannot be read is reported as
/// the backup (R1), and the report says the database was left alone.
#[test]
fn r19_a_broken_backup_says_the_database_is_untouched() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    fs::write(sb.db_path().with_extension("json.backup"), "[{\"id\":1,").unwrap();
    let out = sb.cmd().arg("restore").output().unwrap();
    let err = stderr_of(&out);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("nothing was restored"), "{err}");
    assert!(err.contains("database is unchanged"), "{err}");
    assert_eq!(sb.read_db(), THREE_TASKS_DB);
}

/// REVIEW №115: `rusk add "  buy   milk  "` stored the spaces that the list
/// does not show, and then search and "already has this content" went by
/// them: "buy milk" found nothing.
#[test]
fn r19_text_is_stored_trimmed_and_searched_by_words() {
    let sb = Sandbox::new();
    let out = sb.cmd().args(["add", "  buy   milk  "]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(text_of(&db_tasks(&sb), 1), "buy   milk");
    assert_eq!(search_ids(&sb, "buy milk"), ["1"]);
    assert_eq!(search_ids(&sb, " BUY\tmilk "), ["1"]);

    let before = sb.read_db();
    let line = first_line_of(&sb, &["edit", "1", "buy   milk "]);
    assert!(line.starts_with("Task unchanged: 1"), "{line}");
    assert_eq!(sb.read_db(), before);

    // Review of R19: a text stored with spaces at its edges before texts
    // were cleaned keeps them while its words stay the same.
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"  buy milk  "}]"#);
    let before = sb.read_db();
    let line = first_line_of(&sb, &["edit", "1", "  buy milk  "]);
    assert!(line.starts_with("Task unchanged: 1"), "{line}");
    assert_eq!(sb.read_db(), before);
}

/// REVIEW №122: the compact view cut closing quotes (the opening one was
/// left dangling), turned a line of punctuation into an empty row and kept
/// the CJK sentence ends it was meant to cut.
#[test]
fn r19_compact_view_trims_sentence_ends_only() {
    let cases = [
        ("Read the book \"Dune\"", "Read the book \"Dune\""),
        ("Read «Le Petit Prince».", "Read «Le Petit Prince»"),
        ("...", "..."),
        ("rockin' the boys' “quoted”", "rockin' the boys' “quoted”"),
        ("Buy milk.", "Buy milk"),
        ("牛乳を買う。", "牛乳を買う"),
        ("本当に？", "本当に"),
        ("注意：", "注意"),
    ];
    let sb = Sandbox::new();
    for (text, _) in cases {
        let out = sb.cmd().args(["add", text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    }
    let out = sb.cmd().args(["list", "-c"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let expected: Vec<&str> = cases.iter().map(|(_, shown)| *shown).collect();
    assert_eq!(listed_texts(&out), expected);
}

/// REVIEW №133: a new task gets the lowest free id but was put at the end:
/// with task 2 deleted, the next one was listed 1, 3, 4, 2.
#[test]
fn r19_a_task_with_a_reused_id_is_listed_in_its_place() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"t1"},{"id":3,"text":"t3"},{"id":4,"text":"t4"}]"#);
    let line = first_line_of(&sb, &["add", "t5"]);
    assert!(line.starts_with("Added task: 2"), "{line}");
    assert_eq!(search_ids(&sb, "t"), ["1", "2", "3", "4"]);
    let out = sb.cmd().args(["list", "-c"]).output().unwrap();
    assert_eq!(listed_texts(&out), ["t1", "t5", "t3", "t4"]);

    // Review of R19: a list in an order of the user's own keeps it, and a
    // new task goes at the end, as it always did.
    let sb = Sandbox::with_db(r#"[{"id":5,"text":"t5"},{"id":1,"text":"t1"},{"id":3,"text":"t3"}]"#);
    first_line_of(&sb, &["add", "new"]);
    assert_eq!(search_ids(&sb, "t"), ["5", "1", "3"]);
    let out = sb.cmd().args(["list", "-c"]).output().unwrap();
    assert_eq!(listed_texts(&out), ["t5", "t1", "t3", "new"]);
}

/// REVIEW №134: clearing a date or a dependency list the task did not have
/// reported "cleared ( was: empty )".
#[test]
fn r19_edit_reports_a_clear_only_when_something_was_cleared() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let out = stdout_of(&sb.cmd().args(["edit", "2", "renamed", "-d", "_"]).output().unwrap());
    assert!(!out.contains("cleared") && out.contains(" - date: empty"), "{out}");
    let out = stdout_of(&sb.cmd().args(["edit", "2", "renamed2", "-a", "_"]).output().unwrap());
    assert!(!out.contains("cleared") && out.contains(" - after: empty"), "{out}");
    let out = stdout_of(&sb.cmd().args(["edit", "1", "-d", "_"]).output().unwrap());
    assert!(out.contains(" - date: cleared (was: 1-jan-27)"), "{out}");
}

/// REVIEW №135: `edit` printed dates as 01-01-2027 where `add` and the list
/// say 1-jan-27, and the first line of `mark` and `edit` ended in ": ".
#[test]
fn r19_messages_print_dates_and_ids_one_way() {
    let sb = Sandbox::new();
    assert_eq!(first_line_of(&sb, &["add", "one", "-d", "31-12-2026"]), "Added task: 1: (31-dec-26)");
    let out = stdout_of(&sb.cmd().args(["edit", "1", "-d", "1-1-2027"]).output().unwrap());
    assert!(out.contains(" - date: 1-jan-27 (was: 31-dec-26)"), "{out}");
    assert_eq!(first_line_of(&sb, &["mark", "1"]), "Marked task as done: 1:");
    assert_eq!(first_line_of(&sb, &["edit", "1", "two"]), "Edited task: 1:");
    assert_eq!(first_line_of(&sb, &["edit", "1", "two"]), "Task unchanged: 1:");
}

/// REVIEW №141: without the `interactive` feature, `rusk add` without text
/// failed with "Task text cannot be empty" and dropped a `-d` in silence.
#[test]
#[cfg(not(feature = "interactive"))]
fn r19_add_without_text_names_the_missing_feature() {
    let sb = Sandbox::new();
    for args in [&["add"][..], &["add", "-d", "2d"]] {
        let out = sb.cmd().args(args).output().unwrap();
        let err = stderr_of(&out);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {err}");
        assert!(err.contains("'interactive' feature"), "{args:?}: {err}");
    }
    assert_eq!(sb.read_db(), "");
}

/// REVIEW №142: `--help` offered the editor, SQLite, ssh and the sync
/// variables whether or not the build had them.
#[test]
fn r19_help_offers_only_what_this_build_has() {
    let sb = Sandbox::new();
    let help = |args: &[&str]| -> String {
        let out = sb.cmd().args(args).arg("--help").output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", stderr_of(&out));
        stdout_of(&out)
    };
    let root = help(&[]);
    let all = format!("{root}{}{}", help(&["add"]), help(&["edit"]));
    let del = help(&["del"]);
    for (built, text, words) in [
        (cfg!(feature = "interactive"), &all, &["editor", "TUI", "EDITOR.md"][..]),
        (cfg!(feature = "interactive"), &del, &["confirmed on the terminal"]),
        (!cfg!(feature = "interactive"), &del, &["has no terminal UI"]),
        (cfg!(feature = "backend-sqlite"), &root, &[" .db/.sqlite"]),
        (cfg!(feature = "backend-ssh"), &root, &["user@host"]),
        (cfg!(feature = "backend-http"), &root, &["https://", "RUSK_DB_TOKEN"]),
        (cfg!(feature = "sync"), &root, &["RUSK_SYNC_REMOTE", "RUSK_SYNC_TOKEN"]),
        // With a space before: "CONFIG.md" is no format.
        (cfg!(feature = "fmt-markdown"), &root, &[" .md"]),
        (cfg!(feature = "fmt-todotxt"), &root, &[" .txt"]),
        (cfg!(feature = "fmt-ndjson"), &root, &[" .ndjson"]),
        (cfg!(feature = "fmt-ics"), &root, &[" .ics"]),
        (cfg!(feature = "completions"), &root, &["rusk completions install"]),
    ] {
        for word in words {
            assert_eq!(text.contains(word), built, "{word:?} in the help of this build:\n{text}");
        }
    }
}

/// REVIEW №166: clap painted `--help` and argument errors on its own,
/// blind to RUSK_NO_COLOR and `no_color = true`; and a non-empty NO_COLOR
/// now wins over CLICOLOR_FORCE for rusk's own output as it does for clap's.
#[test]
fn r19_no_color_reaches_help_and_argument_errors() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let config = sb.path().join("no-color.cfg");
    fs::write(&config, "no_color = true\n").unwrap();
    let colored = |cmd: &mut std::process::Command, args: &[&str]| -> bool {
        let out = cmd.env("CLICOLOR_FORCE", "1").args(args).output().unwrap();
        out.stdout.contains(&0x1b) || out.stderr.contains(&0x1b)
    };
    let runs: [&[&str]; 3] = [&["mark", "1", "--bogus"], &["mark", "-h"], &["mark", "abc"]];
    for args in runs {
        // Forced colors do reach every one of these otherwise.
        assert!(colored(sb.cmd().env_remove("RUSK_NO_COLOR").env_remove("NO_COLOR"), args), "{args:?}");
        assert!(!colored(sb.cmd().env("RUSK_NO_COLOR", "1").env_remove("NO_COLOR"), args), "{args:?}");
        assert!(
            !colored(sb.cmd().env_remove("RUSK_NO_COLOR").env_remove("NO_COLOR").env("RUSK_CONFIG", &config), args),
            "{args:?} with no_color = true"
        );
        assert!(!colored(sb.cmd().env_remove("RUSK_NO_COLOR").env("NO_COLOR", "1"), args), "{args:?}");
    }
}

/// REVIEW №168: with `compact = true` in the config, nothing brought back
/// the full view for one run.
#[test]
fn r19_no_compact_overrides_the_config_for_one_run() {
    let sb = Sandbox::new();
    let config = sb.path().join("compact.cfg");
    fs::write(&config, "compact = true\n").unwrap();
    let out = sb.cmd().args(["add", "first line.\nsecond line"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let list = |args: &[&str]| -> String {
        let out = sb.cmd().env("RUSK_CONFIG", &config).args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", stderr_of(&out));
        stdout_of(&out)
    };
    assert!(!list(&["list"]).contains("second line"));
    assert!(list(&["list", "--no-compact"]).contains("second line"));
    // The last of the two wins.
    assert!(!list(&["list", "--no-compact", "-c"]).contains("second line"));
    assert!(list(&["list", "-c", "--no-compact"]).contains("second line"));
}

/// Review of R19: the help of `-d -h` said `Usage: add` for `rusk add`.
#[test]
fn r19_help_for_a_date_value_names_the_command() {
    let sb = Sandbox::new();
    for (args, usage) in [(&["add", "-d", "-h"][..], "Usage: rusk add"), (&["edit", "1", "-d", "-h"], "Usage: rusk edit")] {
        let out = sb.cmd().args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", stderr_of(&out));
        assert!(stdout_of(&out).contains(usage), "{args:?}: {}", stdout_of(&out));
    }
}

/// Review of R19: installing completions for two shells when the second
/// cannot be installed said nothing about the first one, which was.
#[test]
#[cfg(feature = "completions")]
fn r19_a_failed_completion_install_names_what_was_installed() {
    let sb = Sandbox::new();
    let home = sb.path().join("home");
    fs::create_dir_all(home.join(".config")).unwrap();
    fs::write(home.join(".config").join("fish"), "not a directory").unwrap();
    let out = sb.cmd().args(["completions", "install", "bash", "fish"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Bash completion installed to:"), "{}", stdout_of(&out));
    assert!(stderr_of(&out).contains("Failed to create directory"), "{}", stderr_of(&out));
}

/// REVIEW №169: the compact view cut a long first line and hid the other
/// lines of a task without a mark: a shortened task looked like a whole one.
#[test]
fn r19_compact_view_marks_a_task_it_shortened() {
    let sb = Sandbox::new();
    for text in [
        "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen",
        "short first\nsecond line",
        "Step one:\nstep two",
        "whole.",
    ] {
        let out = sb.cmd().args(["add", text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    }
    let out = sb.cmd().args(["list", "-c"]).output().unwrap();
    let texts = listed_texts(&out);
    assert!(texts[0].starts_with("one two three") && texts[0].ends_with('…'), "{texts:?}");
    assert_eq!(texts[1..], ["short first…", "Step one…", "whole"]);
    for line in list_lines(&out) {
        assert!(cells(&line) <= 80, "{} cells: {line}", cells(&line));
    }
}

// ---------------------------------------------------------------------------
// R24 — the editor: keys, words, selection, undo, the first-line date
// ---------------------------------------------------------------------------
//
// Driven in a pseudo-terminal (`Sandbox::in_pty`, python's `pty`; skipped
// without python3): the editor is started, the keys are typed once it has
// the screen, and what counts is what it saved. The pure parts are
// unit-tested in `src/cli/editor/` (`text_ops`, `state`, `input`) and in
// `src/cli/handlers.rs` (the prefill and the first-line date).

/// The editor has the screen (alternate screen entered).
#[cfg(all(unix, feature = "interactive"))]
const EDITOR_UP: &[u8] = b"\x1b[?1049h";
#[cfg(all(unix, feature = "interactive"))]
const CTRL_S: &[u8] = b"\x13";

/// `rusk <args>` in a pty, keys typed 150 ms apart once the editor is up.
#[cfg(all(unix, feature = "interactive"))]
fn in_editor(sb: &Sandbox, args: &[&str], keys: &[&[u8]], kitty: bool) -> Option<common::PtyRun> {
    let steps: Vec<(u64, &[u8])> = keys.iter().map(|k| (150, *k)).collect();
    let run = sb.in_pty(args, EDITOR_UP, &steps, kitty)?;
    assert!(run.code.is_some(), "the editor did not exit: {}", String::from_utf8_lossy(&run.screen));
    Some(run)
}

/// REVIEW №9: Ctrl+W and Ctrl+← went back two words.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_a_word_back_is_one_word() {
    let sb = Sandbox::new();
    let Some(run) = in_editor(&sb, &["add"], &[b"note buy milk", b"\x17", CTRL_S], false) else {
        return;
    };
    assert_eq!(run.code, Some(0), "{}", run.after_editor());
    let Some(_) = in_editor(&sb, &["add"], &[b"buy milk", b"\x1b[1;5D", b"X", CTRL_S], false) else {
        return;
    };
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "note buy");
    assert_eq!(text_of(&tasks, 2), "buy Xmilk");
}

/// REVIEW №45: after Ctrl+A on an empty buffer (or Shift+End at the end of
/// a line) the first capital typed was lost.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_a_capital_after_an_empty_selection_is_kept() {
    let sb = Sandbox::new();
    let Some(_) = in_editor(&sb, &["add"], &[b"\x01", b"Buy milk", CTRL_S], false) else {
        return;
    };
    let Some(_) = in_editor(&sb, &["add"], &[b"x", b"\x1b[1;2F", b"Hello", CTRL_S], false) else {
        return;
    };
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "Buy milk");
    assert_eq!(text_of(&tasks, 2), "xHello");
}

/// REVIEW №46: an unbound Ctrl+letter, Alt+letter or Ctrl+Space typed its
/// bare character into the text.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_an_unbound_shortcut_types_nothing() {
    let sb = Sandbox::new();
    let keys: &[&[u8]] = &[b"ab", b"\x05", b"\x04", b"\x1bb", b"\x00", b"\x02", CTRL_S];
    let Some(_) = in_editor(&sb, &["add"], keys, false) else {
        return;
    };
    assert_eq!(text_of(&db_tasks(&sb), 1), "ab");
}

/// REVIEW №47: Ctrl+Shift+K could not be told from Ctrl+K, and reported
/// with the capital letter it typed a `K`. A terminal that reports the
/// Shift gets the line deleted; the editor does not switch the kitty
/// keyboard protocol on for it (review of R24: in a non-Latin layout that
/// protocol reports Ctrl+S as the letter of that layout, and no shortcut
/// worked), so the terminal is left as it was.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_ctrl_shift_k_deletes_the_line_where_the_terminal_can_say_so() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"hello world\nsecond line"},{"id":2,"text":"one\ntwo"}]"#);
    // Kitty sends Ctrl+Shift+K as CSI 107;6u without being asked; xterm's
    // CSI u form of modifyOtherKeys sends the capital, CSI 75;6u.
    for (id, key) in [("1", &b"\x1b[107;6u"[..]), ("2", b"\x1b[75;6u")] {
        let Some(run) = in_editor(&sb, &["edit", id], &[b"\x1b[C\x1b[C", key, CTRL_S], true) else {
            return;
        };
        assert!(!run.saw(b"\x1b[?u") && !run.saw(b"\x1b[>"), "the keyboard protocol was touched");
        // Nor is the terminal asked anything: one that does not answer
        // kept the screen blank for two seconds per task.
        assert!(!run.saw(b"\x1b[c"), "the terminal was queried");
    }
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "second line");
    assert_eq!(text_of(&tasks, 2), "two");

    // Ctrl+K kills to the end of the line.
    let Some(_) = in_editor(&sb, &["edit", "2"], &[b"\x1b[C", b"\x0b", CTRL_S], false) else {
        return;
    };
    assert_eq!(text_of(&db_tasks(&sb), 2), "t");
}

/// REVIEW №116: a key that changed nothing still made a step to undo:
/// Ctrl+Z after a Backspace at the start undid nothing.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_only_a_real_edit_is_undone() {
    let sb = Sandbox::new();
    let keys: &[&[u8]] = &[b"ab", b"\x1b[D", b"\x1b[D", b"\x7f", b"\x1a", b"x", CTRL_S];
    let Some(_) = in_editor(&sb, &["add"], keys, false) else {
        return;
    };
    assert_eq!(text_of(&db_tasks(&sb), 1), "x");
}

/// REVIEW №33: `rusk add tomorrow call mom`, then `rusk edit 1` and Ctrl+S
/// untouched made "tomorrow" the due date and dropped it from the text.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_a_date_like_first_word_stays_text() {
    let sb = Sandbox::new();
    for text in ["tomorrow call mom", "2d fix", "_ note"] {
        let out = sb.cmd().args(["add", text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    }
    let before = sb.read_db();
    for id in ["1", "2", "3"] {
        let Some(run) = in_editor(&sb, &["edit", id], &[CTRL_S], false) else {
            return;
        };
        assert!(run.after_editor().contains("Task unchanged"), "{}", run.after_editor());
    }
    assert_eq!(sb.read_db(), before);

    // Edited elsewhere, the word is still text.
    let Some(_) = in_editor(&sb, &["edit", "1"], &[b"\x1b[F", b"!", CTRL_S], false) else {
        return;
    };
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "tomorrow call mom!");
    assert_eq!(date_of(&tasks, 1), serde_json::Value::Null);

    // Review of R24: a date typed in front of the `_` left the `_` in the
    // text; the `_` right after a date marks the word after it as text.
    let Some(_) = in_editor(&sb, &["edit", "2"], &[b"01-01-2027 ", CTRL_S], false) else {
        return;
    };
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 2), "2d fix");
    assert_eq!(date_of(&tasks, 2), "2027-01-01");
    // With a date, the text opens as `01-01-2027 _ 2d fix`: the date
    // deleted, the `_` is the empty date and `2d` still text.
    let Some(_) = in_editor(&sb, &["edit", "2"], &[b"\x1b[3~".repeat(11).as_slice(), CTRL_S], false) else {
        return;
    };
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 2), "2d fix");
    assert_eq!(date_of(&tasks, 2), serde_json::Value::Null);
}

/// Review of R24: whether the first word reads as a date was judged on the
/// stored text, the editor reads it from the text it shows — without the
/// control characters. `2<ESC>d fix` saved untouched lost `2d`.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_a_date_behind_a_control_character_stays_text() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"2\u001bd fix"},{"id":2,"text":"tomorrow\u0000 call"}]"#);
    for id in ["1", "2"] {
        let Some(_) = in_editor(&sb, &["edit", id], &[CTRL_S], false) else {
            return;
        };
    }
    let tasks = db_tasks(&sb);
    assert_eq!(date_of(&tasks, 1), serde_json::Value::Null);
    assert_eq!(date_of(&tasks, 2), serde_json::Value::Null);
    assert!(text_of(&tasks, 1).ends_with("d fix") && text_of(&tasks, 1).starts_with('2'), "{tasks:?}");
    assert!(text_of(&tasks, 2).starts_with("tomorrow"), "{tasks:?}");
}

/// Review of R24: `rusk add -d` restoring a draft that begins with `_`
/// stored the `_` as text. The date of the command line takes its place.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_a_restored_draft_takes_the_date_of_the_command_line() {
    let sb = Sandbox::new();
    let draft = serde_json::json!({
        "key": "new-task",
        "base": rusk::revision::text_revision(""),
        "text": "_ tomorrow call",
        "timestamp": "2026-09-27T10:00:00+00:00",
    });
    std::fs::write(sb.path().join("rusk_debug").join("editor-new-task.draft"), draft.to_string()).unwrap();
    let steps: &[(u64, &[u8])] = &[(150, b"y"), (800, CTRL_S)];
    let Some(run) = sb.in_pty(&["add", "-d", "01-01-2027"], b"Restore unsaved draft", steps, false) else {
        return;
    };
    assert_eq!(run.code, Some(0), "{}", run.after_editor());
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "tomorrow call");
    assert_eq!(date_of(&tasks, 1), "2027-01-01");
}

/// REVIEW №32, №118: a date alone on the first line, or empty lines at the
/// end, were stored as a text with an empty line in front or behind.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_empty_lines_around_the_text_are_not_stored() {
    let sb = Sandbox::new();
    let keys: &[&[u8]] = &[b"19-09-2026", b"\r", b"buy milk", b"\r", b"\r", CTRL_S];
    let Some(_) = in_editor(&sb, &["add"], keys, false) else {
        return;
    };
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "buy milk");
    assert_eq!(date_of(&tasks, 1), "2026-09-19");
}

/// REVIEW №117: a text with CRLF opened "dirty": Esc asked whether to
/// discard changes nobody made.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r24_a_crlf_text_opens_unchanged() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"a\r\nb"}]"#);
    let before = sb.read_db();
    let Some(run) = in_editor(&sb, &["edit", "1"], &[b"\x1b"], false) else {
        return;
    };
    assert_eq!(run.code, Some(0));
    assert!(!String::from_utf8_lossy(&run.screen).contains("Discard"), "a clean buffer asked to discard");
    assert_eq!(sb.read_db(), before);
}

// ---------------------------------------------------------------------------
// R20 — `rusk serve`: requests, tokens, hosts, headers
// ---------------------------------------------------------------------------
//
// A real `rusk serve --port 0` of the sandbox, spoken to in raw HTTP/1.1
// over a keep-alive connection, as a browser does. The routing rules
// themselves are unit-tested in `src/web/server.rs` against tiny_http's
// `TestRequest`.

/// `rusk serve` of `sb` with `config` as its config file; killed on drop.
#[cfg(feature = "web")]
struct Served {
    child: std::process::Child,
    port: u16,
    _stdout: std::io::BufReader<std::process::ChildStdout>,
}

#[cfg(feature = "web")]
impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(feature = "web")]
fn serve(sb: &Sandbox, config: &str, args: &[&str]) -> Served {
    use std::io::BufRead;
    let mut child = sb
        .cmd()
        .env("RUSK_CONFIG", config)
        .args(["serve", "--port", "0"])
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut port = None;
    for _ in 0..10 {
        let mut line = String::new();
        if out.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if let Some(addr) = line.split("http://").nth(1) {
            port = addr.trim().rsplit(':').next().and_then(|p| p.parse().ok());
        }
        if line.contains("Ctrl+C") {
            break;
        }
    }
    let Some(port) = port else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("the server printed no address");
    };
    Served { child, port, _stdout: out }
}

/// One keep-alive HTTP/1.1 connection.
#[cfg(feature = "web")]
struct Http(std::net::TcpStream);

/// The body of a chunked response in `raw`, once its last chunk is there.
#[cfg(feature = "web")]
fn dechunk(raw: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    let mut at = 0;
    loop {
        let line_end = at + raw[at..].windows(2).position(|w| w == b"\r\n")?;
        let size_text = String::from_utf8_lossy(&raw[at..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        let data = line_end + 2;
        if size == 0 {
            return Some(body);
        }
        if raw.len() < data + size + 2 {
            return None;
        }
        body.extend_from_slice(&raw[data..data + size]);
        at = data + size + 2;
    }
}

#[cfg(feature = "web")]
impl Http {
    fn to(port: u16) -> Self {
        let stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
        Self(stream)
    }

    /// Sends `head` (request line and headers, without the blank line) and
    /// `body`, and returns the response: headers, then the body unless the
    /// request was a HEAD.
    fn send(&mut self, head: &str, body: &str) -> String {
        use std::io::{Read, Write};
        let length = if body.is_empty() { String::new() } else { format!("Content-Length: {}\r\n", body.len()) };
        write!(self.0, "{head}\r\n{length}\r\n{body}").unwrap();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        let end = loop {
            let n = self.0.read(&mut chunk).unwrap_or_else(|e| panic!("{head}: {e}"));
            assert!(n > 0, "{head}: connection closed");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break at + 4;
            }
        };
        let headers = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
        let length: usize = headers
            .lines()
            .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
            .unwrap_or(0);
        let chunked = headers
            .lines()
            .any(|l| l.starts_with("transfer-encoding:") && l.contains("chunked"));
        if head.starts_with("HEAD ") {
            // No body.
        } else if chunked {
            // tiny_http sends a body over 32 KiB in chunks: read them all,
            // so that the connection stays in step, and put them together.
            let mut raw = buf.split_off(end);
            loop {
                if let Some(body) = dechunk(&raw) {
                    buf.extend_from_slice(&body);
                    break;
                }
                let n = self.0.read(&mut chunk).unwrap();
                assert!(n > 0, "{head}: connection closed in a chunked body");
                raw.extend_from_slice(&chunk[..n]);
            }
        } else {
            while buf.len() < end + length {
                let n = self.0.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    fn get(&mut self, path: &str, headers: &str) -> String {
        self.send(&format!("GET {path} HTTP/1.1\r\nHost: localhost{headers}"), "")
    }

    fn json(&mut self, method: &str, path: &str, body: &str) -> String {
        self.send(
            &format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json"),
            body,
        )
    }
}

#[cfg(feature = "web")]
fn status(response: &str) -> u16 {
    response.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// The value of response header `name`, lowercased name match.
#[cfg(feature = "web")]
fn header_in<'a>(response: &'a str, name: &str) -> Option<&'a str> {
    response.lines().take_while(|l| !l.is_empty()).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

#[cfg(feature = "web")]
fn token_config(sb: &Sandbox, token: &str) -> String {
    let path = sb.path().join("token.cfg");
    fs::write(&path, format!("web_token = \"{token}\"\n")).unwrap();
    path.display().to_string()
}

/// REVIEW №51: one client that sent the headers of a big body and then
/// stalled held up every other client.
#[test]
#[cfg(feature = "web")]
fn r20_a_stalled_client_does_not_hold_up_the_others() {
    use std::io::Write;
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let server = serve(&sb, "", &[]);
    let mut stalled = std::net::TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    write!(
        stalled,
        "POST /api/tasks HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
         Content-Length: 500000\r\n\r\n{{\"text\":\"{}",
        "a".repeat(3000)
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));

    let started = std::time::Instant::now();
    let mut other = Http::to(server.port);
    other.0.set_read_timeout(Some(std::time::Duration::from_secs(4))).unwrap();
    let res = other.get("/api/tasks", "");
    assert_eq!(status(&res), 200, "{res}");
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    drop(stalled);
}

/// REVIEW №54: a body over 1 MiB was cut and answered "invalid JSON", so a
/// database of more than 1 MB could not be pushed; a body over the limit is
/// 413 now.
#[test]
#[cfg(feature = "web")]
fn r20_a_big_list_goes_through_and_a_too_big_body_is_413() {
    let sb = Sandbox::with_db("[]");
    let server = serve(&sb, "", &[]);
    let mut http = Http::to(server.port);
    let list: Vec<String> = (1..=3000)
        .map(|id| format!(r#"{{"id":{id},"text":"task {id} {}"}}"#, "x".repeat(400)))
        .collect();
    let body = format!("[{}]", list.join(","));
    assert!(body.len() > 1_100_000);
    let res = http.json("PUT", "/api/tasks", &body);
    assert_eq!(status(&res), 200, "{}", &res[..res.len().min(300)]);
    assert_eq!(db_tasks(&sb).len(), 3000);

    let res = http.send(
        "PUT /api/tasks HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 40000000",
        "",
    );
    assert_eq!(status(&res), 413, "{res}");
}

/// REVIEW №52: a token with a `;` or non-ASCII letters was cut or dropped
/// on its way into the cookie and could never sign in; the server says so
/// at start instead. A token with a space inside travels, and serves.
#[test]
#[cfg(feature = "web")]
fn r20_a_token_that_cannot_travel_is_refused_at_start() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let server = serve(&sb, &token_config(&sb, "two words"), &[]);
    let res = Http::to(server.port).get("/api/tasks", "\r\nAuthorization: Bearer two words");
    assert_eq!(status(&res), 200, "{res}");
    drop(server);

    for token in ["tok;en", "café", " padded"] {
        let sb = Sandbox::new();
        let config = token_config(&sb, token);
        let mut child = sb
            .cmd()
            .env("RUSK_CONFIG", &config)
            .args(["serve", "--port", "0"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // A server that starts serves until it is stopped.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                panic!("{token}: the server started with a token that cannot sign in");
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let out = child.wait_with_output().unwrap();
        let err = stderr_of(&out);
        assert_eq!(out.status.code(), Some(1), "{token}: {err}");
        assert!(err.contains("web_token may hold printable ASCII only"), "{token}: {err}");
    }
}

/// REVIEW №109: a `+` in a bookmarked `/?token=` became a space.
#[test]
#[cfg(feature = "web")]
fn r20_a_bookmark_keeps_a_plus_in_the_token() {
    let sb = Sandbox::new();
    let server = serve(&sb, &token_config(&sb, "ab+cd/ef=="), &[]);
    let res = Http::to(server.port).get("/?token=ab+cd/ef==", "");
    assert_eq!(status(&res), 303, "{res}");
    assert!(header_in(&res, "Set-Cookie").unwrap().starts_with("rusk_token=ab+cd/ef==;"));
}

/// REVIEW №110, №111: HEAD got 404/405; a stale `?token=` got raw JSON.
#[test]
#[cfg(feature = "web")]
fn r20_head_works_and_a_stale_bookmark_gets_the_login_page() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let server = serve(&sb, "", &[]);
    let mut http = Http::to(server.port);
    for path in ["/", "/api/tasks"] {
        let res = http.send(&format!("HEAD {path} HTTP/1.1\r\nHost: localhost"), "");
        assert_eq!(status(&res), 200, "{path}: {res}");
    }
    drop(server);

    let sb = Sandbox::new();
    let server = serve(&sb, &token_config(&sb, "sekret"), &[]);
    let res = Http::to(server.port).get("/?token=old", "");
    assert_eq!(status(&res), 401);
    assert!(header_in(&res, "Content-Type").unwrap().starts_with("text/html"), "{res}");
    assert!(res.contains("<form"), "{res}");
}

/// REVIEW №112: the loopback check compared strings: `127.0.0.2`, `[::1]`
/// and `LOCALHOST` were refused without a token, and `localhost` was bound
/// on `::1` only.
#[test]
#[cfg(feature = "web")]
fn r20_loopback_is_an_address_not_a_spelling() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    // All of 127.0.0.0/8 is there on Linux; elsewhere 127.0.0.1 alone.
    let hosts: &[&str] = if cfg!(target_os = "linux") {
        &["127.0.0.2", "LOCALHOST", "localhost", "localhost."]
    } else {
        &["LOCALHOST", "localhost", "localhost."]
    };
    for &host in hosts {
        let server = serve(&sb, "", &["--host", host]);
        if host != "127.0.0.2" {
            // Bound on 127.0.0.1, where rusk's own http client looks.
            let res = Http::to(server.port).get("/api/tasks", "");
            assert_eq!(status(&res), 200, "{host}: {res}");
        }
    }
    let out = sb.cmd().args(["serve", "--port", "0", "--host", "0.0.0.0"]).output().unwrap();
    assert!(stderr_of(&out).contains("refusing to serve on 0.0.0.0 without authentication"));
}

/// REVIEW №113, SECURITY.md M2: without a token any `Host` was served, so
/// a page that rebinds its own name to 127.0.0.1 could change the tasks.
#[test]
#[cfg(feature = "web")]
fn r20_without_a_token_other_host_names_are_refused() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let server = serve(&sb, "", &[]);
    let mut http = Http::to(server.port);
    let res = http.send(
        "POST /api/tasks HTTP/1.1\r\nHost: evil.attacker.example\r\nContent-Type: application/json",
        r#"{"text":"injected"}"#,
    );
    assert_eq!(status(&res), 403, "{res}");
    assert_eq!(db_tasks(&sb).len(), 3);
    let res = http.send(&format!("GET /api/tasks HTTP/1.1\r\nHost: 127.0.0.1:{}", server.port), "");
    assert_eq!(status(&res), 200, "{res}");
}

/// REVIEW №151, №197, №198, №152: `Application/JSON` was 415; a stale
/// cookie hid a valid Bearer token; the page could be framed by another
/// site; the cookie lacked `Secure` behind TLS and there was no way out.
#[test]
#[cfg(feature = "web")]
fn r20_headers_the_server_reads_and_sends() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let server = serve(&sb, &token_config(&sb, "sekret"), &[]);
    let mut http = Http::to(server.port);
    let res = http.send(
        "POST /api/tasks HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer sekret\r\nContent-Type: Application/JSON; charset=UTF-8",
        r#"{"text":"mixed case"}"#,
    );
    assert_eq!(status(&res), 201, "{res}");

    let res = http.get("/api/tasks", "\r\nCookie: rusk_token=oldtoken\r\nAuthorization: Bearer sekret");
    assert_eq!(status(&res), 200, "{res}");

    let res = http.get("/", "\r\nAuthorization: Bearer sekret");
    assert_eq!(header_in(&res, "X-Frame-Options"), Some("DENY"), "{res}");
    assert_eq!(header_in(&res, "Content-Security-Policy"), Some("frame-ancestors 'none'"));

    // The page behind a token offers to sign out.
    assert!(res.contains("signedIn: true"), "{res}");

    let res = http.get("/?token=sekret", "\r\nX-Forwarded-Proto: https");
    assert!(header_in(&res, "Set-Cookie").unwrap().ends_with("; Secure"), "{res}");
    // Signing out is a POST that says it is JSON (review of R20: a GET
    // could be sent by any page, or prefetched).
    let res = http.json("POST", "/logout", "{}");
    assert_eq!(status(&res), 204, "{res}");
    assert!(header_in(&res, "Set-Cookie").unwrap().contains("Max-Age=0"), "{res}");
    assert_eq!(status(&http.get("/logout", "")), 405);
}

/// Review of R20: requests whose body never came each held one of 32
/// slots, and then everybody got 503; a sign-in form of 32 MiB was read
/// before anybody signed in.
#[test]
#[cfg(feature = "web")]
fn r20_stalled_bodies_hold_up_nobody() {
    use std::io::Write;
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let server = serve(&sb, &token_config(&sb, "sekret"), &[]);
    let mut http = Http::to(server.port);
    let form = "POST /auth HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded";
    let res = http.send(form, &format!("token={}", "x".repeat(5000)));
    assert_eq!(status(&res), 413, "{res}");

    let mut stalled = Vec::new();
    for i in 0..40 {
        let mut client = std::net::TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        let head = if i % 2 == 0 {
            "POST /auth HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2000\r\n\r\ntoken="
        } else {
            "GET /api/tasks HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2000\r\n\r\n"
        };
        client.write_all(head.as_bytes()).unwrap();
        stalled.push(client);
    }
    std::thread::sleep(std::time::Duration::from_millis(300));

    let started = std::time::Instant::now();
    let mut http = Http::to(server.port);
    http.0.set_read_timeout(Some(std::time::Duration::from_secs(4))).unwrap();
    let res = http.get("/api/tasks", "\r\nAuthorization: Bearer sekret");
    assert_eq!(status(&res), 200, "{res}");
    let res = http.json("POST", "/api/tasks", &format!(r#"{{"text":"{}"}}"#, "long ".repeat(400)));
    assert_eq!(status(&res), 401, "{res}");
    let res = http.send(
        "POST /api/tasks HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer sekret\r\nContent-Type: application/json",
        &format!(r#"{{"text":"{}"}}"#, "long ".repeat(400)),
    );
    assert_eq!(status(&res), 201, "{res}");
    let res = http.send(form, &format!("token={}", "x".repeat(5000)));
    assert_eq!(status(&res), 413, "{res}");
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    drop(stalled);
}

/// Review of R20: when tiny_http could not accept a connection any more
/// (no file descriptors left) it stopped for good, and the server ended
/// with success — a supervisor had no reason to start it again.
#[test]
#[cfg(all(unix, feature = "web"))]
fn r20_a_server_that_cannot_accept_fails() {
    use std::io::BufRead;
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let template = sb.cmd();
    let mut sh = std::process::Command::new("sh");
    for (key, value) in template.get_envs() {
        match value {
            Some(value) => sh.env(key, value),
            None => sh.env_remove(key),
        };
    }
    let mut child = sh
        .current_dir(sb.path())
        .arg("-c")
        .arg("ulimit -n 32 && exec \"$0\" serve --port 0")
        .arg(template.get_program())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut out = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    let port: u16 = line.trim().rsplit(':').next().and_then(|p| p.parse().ok()).expect(&line);

    let mut clients = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("the server went on without taking connections");
        }
        if clients.len() < 64
            && let Ok(client) = std::net::TcpStream::connect(("127.0.0.1", port))
        {
            clients.push(client);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let mut err = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut err).unwrap();
    assert_eq!(status.code(), Some(1), "{err}");
    assert!(err.contains("stopped taking connections"), "{err}");
}

/// REVIEW №150: the parts of the API no test spoke to: the login form,
/// DELETE of the done tasks, `after` over HTTP, a refused PUT, 404 / 405.
#[test]
#[cfg(feature = "web")]
fn r20_the_rest_of_the_api() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let server = serve(&sb, &token_config(&sb, "sekret"), &[]);
    let mut http = Http::to(server.port);
    let form = "POST /auth HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded";
    let res = http.send(form, "token=nope");
    assert_eq!(status(&res), 401);
    let res = http.send(form, "token=sekret");
    assert_eq!(status(&res), 303, "{res}");
    let cookie = header_in(&res, "Set-Cookie").unwrap().split(';').next().unwrap().to_string();

    let with = |method: &str, path: &str| {
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nCookie: {cookie}\r\nContent-Type: application/json")
    };
    let res = http.send(&with("POST", "/api/tasks"), r#"{"text":"after one","after":[1]}"#);
    assert_eq!(status(&res), 201, "{res}");
    assert!(res.contains(r#""after":[1]"#), "{res}");
    let res = http.send(&with("PATCH", "/api/tasks/4"), r#"{"after":[9]}"#);
    assert_eq!(status(&res), 400, "{res}");
    let res = http.send(&with("PATCH", "/api/tasks/2"), r#"{"done":true}"#);
    assert_eq!(status(&res), 200);
    let res = http.send(&with("DELETE", "/api/tasks/done"), "");
    assert_eq!(status(&res), 200, "{res}");
    assert!(res.contains(r#""deleted":1"#), "{res}");
    let res = http.send(&with("PUT", "/api/tasks"), r#"[{"id":0,"text":"zero"}]"#);
    assert_eq!(status(&res), 400, "{res}");
    let res = http.send(&with("DELETE", "/api/tasks"), "");
    assert_eq!(status(&res), 405, "{res}");
    let res = http.send(&with("GET", "/api/nothing"), "");
    assert_eq!(status(&res), 404, "{res}");
}

// ---------------------------------------------------------------------------
// R21 — `rusk sync` and the transports
// ---------------------------------------------------------------------------
//
// The ssh side runs through the fake `ssh` of `tests/common` (the remote
// command runs here, on a path of the sandbox); the http side through a
// real `rusk serve`, or a fake `curl` that shows what it was given. The
// decision table and the state file are unit-tested in `src/sync.rs`, the
// header file in `src/transport.rs`.

/// `rusk sync <args>` against `remote` (an ssh location) through the fake ssh.
#[cfg(all(unix, feature = "sync"))]
fn sync_with(sb: &Sandbox, remote: &str, args: &[&str]) -> std::process::Output {
    sb.cmd_with_fake_ssh()
        .env("RUSK_SYNC_REMOTE", remote)
        .arg("sync")
        .args(args)
        .output()
        .unwrap()
}

/// An ssh remote `name` in the sandbox, and its location.
#[cfg(all(unix, feature = "sync"))]
fn ssh_remote(sb: &Sandbox, name: &str) -> (std::path::PathBuf, String) {
    let path = sb.path().join("remote").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let location = format!("user@host:{}", path.display());
    (path, location)
}

/// REVIEW №23: a first sync into an empty local database was refused as
/// "both changed", and `sync pull` pointed at `sync push`.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r21_a_first_sync_seeds_an_empty_local_database() {
    let sb = Sandbox::new();
    let (remote_path, remote) = ssh_remote(&sb, "tasks.json");
    fs::write(&remote_path, THREE_TASKS_DB).unwrap();
    let out = sync_with(&sb, &remote, &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Pulled 3 task(s)"), "{}", stdout_of(&out));
    assert_eq!(db_tasks(&sb).len(), 3);

    let sb = Sandbox::new();
    let out = sync_with(&sb, &remote, &["pull"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(db_tasks(&sb).len(), 3);
}

/// REVIEW №23: in a conflict, `sync push` sent the user to `sync pull` and
/// `sync pull` back to `sync push`; a state file of garbage went unsaid.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r21_a_conflict_names_the_ways_out_and_why_there_is_no_history() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let (remote_path, remote) = ssh_remote(&sb, "tasks.json");
    fs::write(&remote_path, r#"[{"id":1,"text":"remote only"}]"#).unwrap();
    fs::write(sb.db_path().with_extension("json.sync"), "garbage").unwrap();
    for args in [&[][..], &["push"], &["pull"]] {
        let out = sync_with(&sb, &remote, args);
        let err = stderr_of(&out);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {err}");
        assert!(err.contains("push --force") && err.contains("pull --force"), "{args:?}: {err}");
        assert!(!err.contains("first"), "{args:?}: {err}");
        assert!(err.contains("tasks.json.sync' is not one rusk wrote"), "{args:?}: {err}");
    }
}

/// REVIEW №24: a remote in a format that cannot hold a text as it was sent
/// (Markdown reads a line that looks like a list item as a task of its own,
/// a leading `!` as the priority mark) read back differently, and the next
/// `rusk sync` pulled that over the local database.
#[test]
#[cfg(all(unix, feature = "sync", feature = "fmt-markdown"))]
fn r21_a_lossy_remote_does_not_bounce_back() {
    let sb = Sandbox::with_db(
        r#"[{"id":1,"text":"first\n- [ ] nested item"},{"id":3,"text":"! bang"}]"#,
    );
    let (remote_path, remote) = ssh_remote(&sb, "tasks.md");
    let before = sb.read_db();
    let out = sync_with(&sb, &remote, &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Pushed 2 task(s)"), "{}", stdout_of(&out));
    assert!(stderr_of(&out).contains("does not hold the tasks exactly"), "{}", stderr_of(&out));
    let remote_after = fs::read_to_string(&remote_path).unwrap();

    let out = sync_with(&sb, &remote, &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Already in sync"), "{}", stdout_of(&out));
    assert_eq!(sb.read_db(), before, "the local database was overwritten");
    assert_eq!(fs::read_to_string(&remote_path).unwrap(), remote_after);

    // A change on either side is a change again.
    sb.cmd().args(["add", "one more"]).output().unwrap();
    let out = sync_with(&sb, &remote, &[]);
    assert!(stdout_of(&out).contains("Pushed 3 task(s)"), "{}", stdout_of(&out));
}

/// REVIEW №70: `--force` read both sides first, so it could not replace
/// the one side that did not read — which is when it is needed.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r21_force_replaces_a_side_that_does_not_read() {
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let (remote_path, remote) = ssh_remote(&sb, "tasks.json");
    fs::write(&remote_path, "{garbage").unwrap();
    let out = sync_with(&sb, &remote, &["push", "--force"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(fs::read_to_string(&remote_path).unwrap().contains("first task"));

    sb.write_db("{garbage");
    let out = sync_with(&sb, &remote, &["pull", "--force"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(db_tasks(&sb).len(), 3);
    // The database that did not read is kept as the backup.
    assert_eq!(fs::read_to_string(sb.db_path().with_extension("json.backup")).unwrap(), "{garbage");
}

/// REVIEW №107: a `.sync` that could not be written was not said; the
/// next sync then reported a divergence that was none.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r21_a_sync_that_cannot_be_recorded_says_so() {
    use std::os::unix::fs::PermissionsExt;
    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let (_, remote) = ssh_remote(&sb, "tasks.json");
    let dir = sb.db_path().parent().unwrap().to_path_buf();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    let out = sync_with(&sb, &remote, &[]);
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    if fs::metadata(sb.db_path().with_extension("json.sync")).is_ok() {
        eprintln!("skipping: running as a user that ignores directory modes (root?)");
        return;
    }
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stderr_of(&out).contains("could not be recorded"), "{}", stderr_of(&out));
}

/// Review of R21: what a side holds after the sync was read back from it,
/// so a change another writer made right after the write passed for this
/// sync's: the next sync said "Already in sync", and a later push dropped
/// it. The fake ssh here writes the remote itself right after rusk's write.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r21_a_change_made_right_after_the_sync_is_not_taken_for_it() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"a"}]"#);
    let (remote_path, remote) = ssh_remote(&sb, "tasks.json");
    let count = sb.path().join("ssh-calls");
    let meanwhile = sb.path().join("meanwhile.json");
    fs::write(&meanwhile, r#"[{"id":1,"text":"a"},{"id":2,"text":"b"},{"id":3,"text":"added meanwhile"}]"#).unwrap();
    let shim = format!(
        "#!/bin/sh\nshift\nn=$(cat '{count}' 2>/dev/null || echo 0); n=$((n + 1)); echo $n > '{count}'\n\
         sh -c \"$1\" <&0; rc=$?\n[ \"$(cat '{armed}' 2>/dev/null)\" = $n ] && cp '{meanwhile}' '{remote}'\nexit $rc\n",
        count = count.display(),
        armed = sb.path().join("armed").display(),
        meanwhile = meanwhile.display(),
        remote = remote_path.display(),
    );
    let sync = |args: &[&str]| {
        sb.cmd_with_ssh_shim(&shim)
            .env("RUSK_SYNC_REMOTE", &remote)
            .arg("sync")
            .args(args)
            .output()
            .unwrap()
    };
    assert!(sync(&[]).status.success());
    sb.cmd().args(["add", "b"]).output().unwrap();
    // The next sync reads the remote (call 1) and writes it (call 2); the
    // other writer comes right after that.
    let calls: u32 = fs::read_to_string(&count).unwrap().trim().parse().unwrap();
    fs::write(sb.path().join("armed"), format!("{}", calls + 2)).unwrap();
    let out = sync(&[]);
    assert!(stdout_of(&out).contains("Pushed 2 task(s)"), "{}", stdout_of(&out));
    assert!(!stderr_of(&out).contains("does not hold the tasks exactly"), "{}", stderr_of(&out));

    let out = sync(&[]);
    assert!(stdout_of(&out).contains("Pulled 3 task(s)"), "{}\n{}", stdout_of(&out), stderr_of(&out));
    assert_eq!(text_of(&db_tasks(&sb), 3), "added meanwhile");
}

/// Review of R21: `--force` over a side that holds the same tasks wrote it
/// again and replaced its `.backup`, the one copy of what came before.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r21_force_over_the_same_tasks_writes_nothing() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"a"},{"id":2,"text":"b"}]"#);
    let (remote_path, remote) = ssh_remote(&sb, "tasks.json");
    fs::write(&remote_path, sb.read_db()).unwrap();
    let backup = sb.db_path().with_extension("json.backup");
    fs::write(&backup, r#"[{"id":1,"text":"a"},{"id":2,"text":"b"},{"id":3,"text":"c"}]"#).unwrap();
    let remote_before = fs::read(&remote_path).unwrap();
    for direction in ["pull", "push"] {
        let out = sync_with(&sb, &remote, &[direction, "--force"]);
        assert!(out.status.success(), "{}", stderr_of(&out));
        assert!(stdout_of(&out).contains("Already in sync"), "{direction}: {}", stdout_of(&out));
    }
    assert!(fs::read_to_string(&backup).unwrap().contains("\"c\""), "the backup was replaced");
    assert_eq!(fs::read(&remote_path).unwrap(), remote_before);
}

/// Review of R21: without a sync on record, `push` towards the side that
/// has the tasks (and `pull` the other way) spoke of "the last sync", and
/// `pull --force` was offered to "discard" a change — it empties.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r21_a_first_sync_the_wrong_way_says_what_happens() {
    let sb = Sandbox::new();
    let (remote_path, remote) = ssh_remote(&sb, "tasks.json");
    fs::write(&remote_path, THREE_TASKS_DB).unwrap();
    let out = sync_with(&sb, &remote, &["push"]);
    let err = stderr_of(&out);
    assert!(!out.status.success());
    assert!(err.contains("have not been synced before") && !err.contains("since the last sync"), "{err}");
    assert!(err.contains("`rusk sync pull` (or `rusk sync`) copies those tasks here"), "{err}");
    assert!(err.contains("`rusk sync push --force` empties"), "{err}");

    let sb = Sandbox::with_db(THREE_TASKS_DB);
    let (_, remote) = ssh_remote(&sb, "missing.json");
    let out = sync_with(&sb, &remote, &["pull"]);
    let err = stderr_of(&out);
    assert!(!out.status.success());
    assert!(err.contains("have not been synced before") && err.contains("empties the local database"), "{err}");
}

/// Review of R21: a lossy remote synced by an older rusk, whose state has
/// one hash for both sides, bounced back once after the upgrade.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r21_an_older_state_with_a_lossy_remote_does_not_bounce() {
    let sb = Sandbox::with_db(
        r#"[{"id":1,"text":"first\n- [ ] nested item"},{"id":3,"text":"! bang"}]"#,
    );
    let (_, remote) = ssh_remote(&sb, "tasks.md");
    assert!(sync_with(&sb, &remote, &[]).status.success());
    // The state as rusk wrote it up to 0.7.3: one hash, the local one.
    let state_path = sb.db_path().with_extension("json.sync");
    let state: serde_json::Value = serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
    let legacy = serde_json::json!({"remote": state["remote"], "hash": state["local_hash"]});
    fs::write(&state_path, legacy.to_string()).unwrap();
    let before = sb.read_db();
    let out = sync_with(&sb, &remote, &[]);
    assert!(stdout_of(&out).contains("Already in sync"), "{}\n{}", stdout_of(&out), stderr_of(&out));
    assert_eq!(sb.read_db(), before);
}

/// REVIEW №103: the Bearer token was on curl's command line, where `ps`
/// shows it to every local user.
#[test]
#[cfg(all(unix, feature = "sync"))]
fn r21_the_token_is_not_on_curls_command_line() {
    use std::os::unix::fs::PermissionsExt;
    let sb = Sandbox::new();
    let bin = sb.path().join("fake-curl");
    fs::create_dir_all(&bin).unwrap();
    let log = sb.path().join("curl.log");
    // Logs its arguments and the content of every `-H @file`, then answers
    // like `curl -i` with an empty task list.
    let script = format!(
        "#!/bin/sh\nfor a in \"$@\"; do printf 'arg:%s\\n' \"$a\" >> '{log}'; \
         case \"$a\" in @*) printf 'file:%s\\n' \"$(cat \"${{a#@}}\")\" >> '{log}';; esac; done\n\
         printf 'HTTP/1.1 200 OK\\r\\nContent-Type: application/json\\r\\n\\r\\n[]'\n",
        log = log.display()
    );
    let curl = bin.join("curl");
    fs::write(&curl, script).unwrap();
    fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(bin.clone()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    // Through `rusk sync`: a debug binary keeps its database local whatever
    // RUSK_DB says, but it talks to the sync remote.
    let out = sb
        .cmd()
        .env("PATH", path)
        .env("RUSK_SYNC_REMOTE", "http://127.0.0.1:9")
        .env("RUSK_SYNC_TOKEN", "s3cret")
        .arg("sync")
        .output()
        .unwrap();
    let logged = fs::read_to_string(&log).unwrap_or_else(|_| panic!("curl was not run: {}", stderr_of(&out)));
    assert!(logged.contains("file:Authorization: Bearer s3cret"), "{logged}");
    assert!(!logged.lines().any(|l| l.starts_with("arg:") && l.contains("s3cret")), "{logged}");
    // The file is gone once curl is done.
    let file = logged.lines().find_map(|l| l.strip_prefix("arg:@")).unwrap();
    assert!(!std::path::Path::new(file).exists(), "{file} was left behind");
}

/// REVIEW №58: the sync state was keyed by the remote as typed:
/// `http://h:p/` and then `http://h:p` was a remote never synced with,
/// and a local change after a pull became "both changed".
#[test]
#[cfg(all(unix, feature = "sync", feature = "web"))]
fn r21_a_remote_is_one_however_it_is_spelled() {
    if std::process::Command::new("curl").arg("--version").output().is_err() {
        eprintln!("skipping: no curl");
        return;
    }
    let served = Sandbox::with_db(THREE_TASKS_DB);
    let server = serve(&served, "", &[]);
    let sb = Sandbox::new();
    let sync = |remote: &str, args: &[&str]| {
        sb.cmd().env("RUSK_SYNC_REMOTE", remote).arg("sync").args(args).output().unwrap()
    };
    let with_slash = format!("http://127.0.0.1:{}/", server.port);
    let out = sync(&with_slash, &["pull", "--force"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    sb.cmd().args(["add", "local only"]).output().unwrap();
    let out = sync(with_slash.trim_end_matches('/'), &[]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Pushed 4 task(s)"), "{}", stdout_of(&out));
}

// ---------------------------------------------------------------------------
// R22 — `git_backend`
// ---------------------------------------------------------------------------
//
// The binary with `git_backend = true` saving into a directory of the
// sandbox, and the system `git` looking at the result (skipped without git).

/// `git` in `dir`, without the developer's configuration; its stdout.
/// The run must succeed.
#[cfg(feature = "backend-git")]
fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", stderr_of(&out));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A sandbox whose root is a git repository of Alice's, with `git_backend`
/// on; the database lives in `rusk_debug/` below the root.
#[cfg(feature = "backend-git")]
fn in_alices_repository() -> (Sandbox, std::path::PathBuf) {
    let sb = Sandbox::new();
    git_out(sb.path(), &["init", "-q"]);
    git_out(sb.path(), &["config", "user.name", "Alice"]);
    git_out(sb.path(), &["config", "user.email", "alice@example.com"]);
    let config = sb.path().join("git.cfg");
    fs::write(&config, "git_backend = true\n").unwrap();
    (sb, config)
}

/// REVIEW №21: every commit was `rusk <rusk@localhost>`, whatever identity
/// the repository had.
#[test]
#[cfg(feature = "backend-git")]
fn r22_a_configured_identity_is_kept() {
    if !git_available() {
        return;
    }
    let (sb, config) = in_alices_repository();
    let out = sb.cmd().env("RUSK_CONFIG", &config).args(["add", "proj task"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let log = git_out(sb.path(), &["log", "--format=%an <%ae> / %cn <%ce> / %s"]);
    assert_eq!(log.trim(), "Alice <alice@example.com> / Alice <alice@example.com> / rusk: update tasks.json (1 tasks)");
}

/// SECURITY.md M1: a hook of the repository ran as the user on every save.
#[test]
#[cfg(all(unix, feature = "backend-git"))]
fn r22_no_hook_of_the_repository_runs() {
    use std::os::unix::fs::PermissionsExt;
    if !git_available() {
        return;
    }
    let (sb, config) = in_alices_repository();
    let ran = sb.path().join("HOOK_EXECUTED");
    for hook in ["pre-commit", "commit-msg", "post-commit"] {
        let path = sb.path().join(".git").join("hooks").join(hook);
        fs::write(&path, format!("#!/bin/sh\ntouch '{}'\n", ran.display())).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let out = sb.cmd().env("RUSK_CONFIG", &config).args(["add", "trigger"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(!ran.exists(), "a hook ran");
    assert!(git_out(sb.path(), &["log", "--oneline"]).contains("rusk: update"));
}

/// REVIEW №100: in a repository rusk did not create, `.backup`, `.lock`
/// and the like showed up in `git status`.
#[test]
#[cfg(feature = "backend-git")]
fn r22_the_auxiliary_files_stay_out_of_git_status() {
    if !git_available() {
        return;
    }
    let (sb, config) = in_alices_repository();
    for text in ["one", "two"] {
        let out = sb.cmd().env("RUSK_CONFIG", &config).args(["add", text]).output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
    }
    assert!(sb.db_path().with_extension("json.backup").exists());
    let status = git_out(sb.path(), &["status", "--short"]);
    assert!(!status.contains("rusk_debug/"), "{status}");
    // Only the database's own: a `.backup` of the user's elsewhere is theirs.
    fs::write(sb.path().join("notes.backup"), "mine").unwrap();
    assert!(git_out(sb.path(), &["status", "--short"]).contains("notes.backup"));
}

/// REVIEW №177: any failure of `git rev-parse` — a repository of another
/// owner, above all — was taken for "no repository", and rusk created one
/// inside the user's, splitting the history for good.
#[test]
#[cfg(feature = "backend-git")]
fn r22_a_repository_git_will_not_use_is_no_reason_to_create_one() {
    if !git_available() {
        return;
    }
    let (sb, config) = in_alices_repository();
    // A git that knows no test switch for another owner has nothing to show.
    let refused = std::process::Command::new("git")
        .arg("-C")
        .arg(sb.path())
        .args(["rev-parse", "--git-dir"])
        .env("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1")
        .output()
        .is_ok_and(|out| !out.status.success());
    if !refused {
        return;
    }
    let out = sb
        .cmd()
        .env("RUSK_CONFIG", &config)
        .env("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1")
        .args(["add", "two"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let err = stderr_of(&out);
    assert!(err.contains("belongs to another user, and git does not trust it"), "{err}");
    // Not git's advice to trust it anyway: its config would run as the user.
    assert!(!err.contains("safe.directory"), "{err}");
    assert!(!sb.db_path().parent().unwrap().join(".git").exists(), "a nested repository was created");
}

/// Review of R22: the identity git has — from `EMAIL` here — was replaced
/// by `rusk <rusk@localhost>` wherever `user.email` was not configured.
#[test]
#[cfg(feature = "backend-git")]
fn r22_git_s_own_identity_is_used() {
    if !git_available() {
        return;
    }
    let sb = Sandbox::new();
    git_out(sb.path(), &["init", "-q"]);
    let config = sb.path().join("git.cfg");
    fs::write(&config, "git_backend = true\n").unwrap();
    let out = sb
        .cmd()
        .env("RUSK_CONFIG", &config)
        .env("EMAIL", "bob@example.com")
        .args(["add", "one"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(git_out(sb.path(), &["log", "-1", "--format=%ae / %ce"]).trim(), "bob@example.com / bob@example.com");
}

/// Review of R22: an `info/exclude` that could not be written stopped the
/// commit; an enclosing repository that ignores the database was a
/// cryptic `git add` failure on every save.
#[test]
#[cfg(all(unix, feature = "backend-git"))]
fn r22_what_keeps_the_history_from_going_on_is_said() {
    use std::os::unix::fs::PermissionsExt;
    if !git_available() {
        return;
    }
    let (sb, config) = in_alices_repository();
    let exclude = sb.path().join(".git").join("info").join("exclude");
    fs::write(&exclude, "").unwrap();
    fs::set_permissions(&exclude, fs::Permissions::from_mode(0o444)).unwrap();
    let out = sb.cmd().env("RUSK_CONFIG", &config).args(["add", "one"]).output().unwrap();
    assert!(stderr_of(&out).contains("failed to update"), "{}", stderr_of(&out));
    assert!(git_out(sb.path(), &["log", "--oneline"]).contains("rusk: update"), "no commit");

    fs::set_permissions(&exclude, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(sb.path().join(".gitignore"), "rusk_debug/\n").unwrap();
    let out = sb.cmd().env("RUSK_CONFIG", &config).args(["add", "two"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let err = stderr_of(&out);
    assert!(err.contains("ignores tasks.json, so nothing is committed"), "{err}");
}

/// REVIEW №98: a database right in the home directory made the home
/// directory a git repository, without a word.
#[test]
#[cfg(feature = "backend-git")]
fn r22_the_home_directory_is_not_made_a_repository() {
    if !git_available() {
        return;
    }
    let sb = Sandbox::new();
    let config = sb.path().join("git.cfg");
    fs::write(&config, "git_backend = true\n").unwrap();
    let home = sb.db_path().parent().unwrap().to_path_buf();
    let out = sb
        .cmd()
        .env("RUSK_CONFIG", &config)
        .env("HOME", &home)
        .args(["add", "first"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stderr_of(&out).contains("directly in your home directory"), "{}", stderr_of(&out));
    assert!(!home.join(".git").exists());
}

/// REVIEW №59: `git_backend = true` was taken in silence by a build that
/// cannot commit.
#[test]
#[cfg(not(feature = "backend-git"))]
fn r22_git_backend_without_the_feature_is_said() {
    let sb = Sandbox::new();
    let config = sb.path().join("git.cfg");
    fs::write(&config, "git_backend = true\n").unwrap();
    let out = sb.cmd().env("RUSK_CONFIG", &config).args(["add", "one"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stderr_of(&out).contains("needs the 'backend-git' feature"), "{}", stderr_of(&out));
}

// ---------------------------------------------------------------------------
// R25 — config, dates, environment
// ---------------------------------------------------------------------------
//
// The config parser and the date parser are unit-tested in `src/config.rs`
// and `src/parser/date.rs`; here the binary is run with a config file of the
// test's own and colors forced on where the result is a color.

/// `rusk <args>` with `config` as the config file and colors forced on.
fn with_config(sb: &Sandbox, config: &[u8], args: &[&str]) -> std::process::Output {
    let path = sb.path().join("r25.cfg");
    fs::write(&path, config).unwrap();
    sb.cmd()
        .env("RUSK_CONFIG", &path)
        .env_remove("RUSK_NO_COLOR")
        .env("CLICOLOR_FORCE", "1")
        .args(args)
        .output()
        .unwrap()
}

/// REVIEW №12: a release binary took `RUST_TEST_THREADS` in its
/// environment for a test harness and moved the database to
/// `$TMPDIR/rusk_debug` in silence. Only a release build shows it: run
/// `cargo test --release --test review_known_bugs r25_`.
#[test]
#[cfg(not(debug_assertions))]
fn r25_a_release_binary_is_never_in_test_mode() {
    let sb = Sandbox::new();
    let elsewhere = sb.path().join("elsewhere.json");
    let out = sb.cmd().env("RUSK_DB", &elsewhere).env("RUST_TEST_THREADS", "4").args(["add", "probe"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(elsewhere.exists(), "RUSK_DB was not used");
    assert!(!sb.db_path().exists(), "the database went to the test-mode path");
}

/// REVIEW №18: years of one to three digits, signed and five-digit years
/// were taken from the command line and the web API, and showed as a
/// two-digit year nobody could read.
#[test]
fn r25_a_date_has_a_four_digit_year() {
    let sb = Sandbox::new();
    for value in ["01-01--2025", "11-jan-20255", "11-jan-025", "1-1-100", "01-01-+2025", "11-jan-02025"] {
        let out = sb.cmd().args(["add", "t", "-d", value]).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{value}: {}", stdout_of(&out));
        assert!(stderr_of(&out).contains(&format!("Invalid date '{value}'")), "{value}: {}", stderr_of(&out));
    }
    assert_eq!(sb.read_db(), "");

    // Another century keeps its four digits on screen.
    let out = sb.cmd().args(["add", "old", "-d", "4-5-1975"]).output().unwrap();
    assert!(stdout_of(&out).contains("(4-may-1975)"), "{}", stdout_of(&out));

    // Review of R25: an offset past the years is refused in the same words,
    // quoting the value as typed (`+` too) and naming the task; one past
    // what a date can hold panicked.
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"a","date":"9999-12-31"},{"id":2,"text":"b"}]"#);
    let out = sb.cmd().args(["edit", "2,1", "-d", "+1d"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr_of(&out).contains("task 1: Invalid date '+1d': the year 10000 is out of range (dates run from 1000 to 9999)"),
        "{}",
        stderr_of(&out)
    );
    assert_eq!(date_of(&db_tasks(&sb), 1), "9999-12-31");
    for value in ["4294967295w4294967295w4294967295w4294967295w", "100000000d", "1-1-10000"] {
        let out = sb.cmd().args(["add", "t", "-d", value]).output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{value}: {}", stderr_of(&out));
        assert!(stderr_of(&out).contains("(dates run from 1000 to 9999)"), "{value}: {}", stderr_of(&out));
    }
}

/// Review of R25: the files a previous rusk wrote hold such dates (it took
/// `1-1-205` for the year 205). Reading them strictly locked the user out
/// of the database — "corrupted, rm it" — with no way to correct the date.
/// They are read as they are, shown, and named in a warning.
#[test]
fn r25_an_old_year_stays_open_to_correct() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"pay rent","date":"0205-01-01"},{"id":2,"text":"b"}]"#);
    let out = sb.cmd().arg("list").output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("1-jan-0205"), "{}", stdout_of(&out));
    let err = stderr_of(&out);
    assert!(err.contains("task 1") && err.contains("0205-01-01") && err.contains("1000 to 9999"), "{err}");
    assert!(!err.contains("rm '"), "{err}");
    let out = sb.cmd().args(["edit", "1", "-d", "_"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let out = sb.cmd().arg("list").output().unwrap();
    assert_eq!(stderr_of(&out), "");

    // Every codec reads what chrono reads; the load says so.
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("y.csv");
    fs::write(&csv, "id,text,date,done,priority,after\n1,a,26-07-10,false,false,\n").unwrap();
    assert_eq!(backend(&csv).load().unwrap()[0].date.unwrap().to_string(), "0026-07-10");
    let json = dir.path().join("y.json");
    fs::write(&json, r#"[{"id":1,"text":"a","date":"+20255-01-11"}]"#).unwrap();
    assert_eq!(backend(&json).load().unwrap()[0].date.unwrap().to_string(), "+20255-01-11");
    #[cfg(feature = "fmt-ics")]
    {
        let ics = dir.path().join("y.ics");
        fs::write(&ics, "BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:rusk-1@rusk\nSUMMARY:a\nDUE;VALUE=DATE:01000101\nEND:VTODO\nEND:VCALENDAR\n").unwrap();
        assert_eq!(backend(&ics).load().unwrap()[0].date.unwrap().to_string(), "0100-01-01");
    }
    // A value that is no date at all is named, and the file is not called
    // corrupted: correcting the value keeps every task.
    fs::write(&json, r#"[{"id":1,"text":"a","date":"2026-13-45"}]"#).unwrap();
    let err = chain_of(&backend(&json).load().unwrap_err());
    assert!(err.contains("2026-13-45") && err.contains("correct that value"), "{err}");
    assert!(!err.contains("rm '") && !err.contains("corrupted"), "{err}");
}

/// Review of R25: the editor opened a task due on 01-01-0100 with that
/// token in front, no longer read it as a date, and saved it untouched as
/// the text `01-01-0100 old` with no date.
#[test]
#[cfg(all(unix, feature = "interactive"))]
fn r25_the_editor_keeps_a_tasks_own_date() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"old","date":"0100-01-01"}]"#);
    let Some(run) = in_editor(&sb, &["edit", "1"], &[CTRL_S], false) else {
        return;
    };
    assert_eq!(run.code, Some(0), "{}", run.after_editor());
    let tasks = db_tasks(&sb);
    assert_eq!(text_of(&tasks, 1), "old");
    assert_eq!(date_of(&tasks, 1), "0100-01-01");
}

/// REVIEW №18, the web API: a date of the wrong shape was "invalid JSON"
/// at a column, and a signed year was taken.
#[test]
#[cfg(feature = "web")]
fn r25_the_api_names_a_bad_date() {
    use rusk::web::api::{create_task, replace_tasks, update_task};
    let dir = tempfile::tempdir().unwrap();
    let mut tm = TaskManager::new_empty_with_path(dir.path().join("tasks.json"));
    for (date, why) in [
        ("2d", "is not written YYYY-MM-DD"),
        ("-2025-01-01", "is not written YYYY-MM-DD"),
        ("2026-7-1", "is not written YYYY-MM-DD"),
        ("2026-02-30", "is no day of the calendar"),
        ("0999-01-01", "is outside the years 1000-9999"),
    ] {
        let res = create_task(&mut tm, &format!(r#"{{"text":"x","date":"{date}"}}"#), None);
        assert_eq!(res.status, 400, "{date}: {}", res.body);
        assert!(res.body.contains(&format!("invalid task: date '{date}' {why}")), "{date}: {}", res.body);
    }
    assert_eq!(create_task(&mut tm, r#"{"text":"x","date":"2026-07-01"}"#, None).status, 201);
    let res = update_task(&mut tm, 1, r#"{"date":"10000-01-01"}"#, None);
    assert!(res.status == 400 && res.body.contains("invalid task: date '10000-01-01'"), "{}", res.body);
    // JSON that does not parse is still said to be that.
    let res = create_task(&mut tm, r#"{"text":"x","#, None);
    assert!(res.status == 400 && res.body.contains("invalid JSON"), "{}", res.body);
    // A whole list with a date outside the years is not taken either.
    let res = replace_tasks(&mut tm, r#"[{"id":1,"text":"a","date":"0205-01-01"}]"#, None);
    assert_eq!(res.status, 400, "{}", res.body);
    assert!(res.body.contains("task 1 is due on 0205-01-01, outside the years 1000-9999"), "{}", res.body);
}

/// REVIEW №41, №125, №126, №127, №128, №129, №130: the config parser.
#[test]
fn r25_the_config_file_means_what_it_says() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"disabled foo"}]"#);
    // №41: a comment after an empty value is a comment.
    let out = with_config(&sb, b"keywords =   # disabled\n", &["list"]);
    assert!(!stdout_of(&out).contains("\x1b[33mdisabled"), "{}", stdout_of(&out));
    // №126: a BOM does not spoil the first key.
    let out = with_config(&sb, b"\xef\xbb\xbfno_color = true\n", &["list"]);
    assert!(!stdout_of(&out).contains('\x1b') && stderr_of(&out).is_empty(), "{}", stderr_of(&out));
    // №127: `default` puts back the built-in value.
    let out = with_config(&sb, b"error = green\nerror = default\n", &["mark", "abc"]);
    assert!(stderr_of(&out).contains("\x1b[31m"), "{:?}", stderr_of(&out));
    // №128: a line that is not UTF-8 costs that line only.
    let out = with_config(&sb, b"# caf\xe9\nerror = green\n", &["mark", "abc"]);
    assert!(stderr_of(&out).contains("\x1b[32m"), "{:?}", stderr_of(&out));
    assert!(!stderr_of(&out).contains("could not read config"), "{}", stderr_of(&out));
    // №129: a broken quote is said; №130: in the order of the lines.
    let out = with_config(&sb, b"unused1 = 1\nerror = \"green\n", &["list"]);
    let err = stderr_of(&out);
    let first = err.find("cfg:1:").expect(&err);
    let second = err.find("cfg:2: a quoted value without its closing quote").expect(&err);
    assert!(first < second, "{err}");

    // №125: the template names every theme key.
    let fresh = sb.path().join("fresh.cfg");
    let out = sb.cmd().env("RUSK_CONFIG", &fresh).arg("list").output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let template = fs::read_to_string(&fresh).expect("the template is written");
    assert!(template.contains("# search_match = yellow") && template.contains("# sync_token ="), "{template}");

    // Review of R25: the warning about a broken quote does not echo what
    // follows it (a token), and a Latin-1 letter in a comment costs nothing.
    let out = with_config(&sb, b"web_token = \"abc\"def123\n", &["list"]);
    assert!(stderr_of(&out).contains("after the closing quote") && !stderr_of(&out).contains("def123"), "{}", stderr_of(&out));
    let out = with_config(&sb, b"error = green # caf\xe9\n", &["mark", "abc"]);
    assert!(stderr_of(&out).contains("\x1b[32m") && !stderr_of(&out).contains("cfg:"), "{:?}", stderr_of(&out));
}

/// REVIEW №131, №132: the date forms are in the help, and months are added
/// in one step (`+1m1m` from 31-01 was 28-03, `+2m` 31-03).
#[test]
fn r25_date_forms_and_month_offsets() {
    let out = Sandbox::new().cmd().args(["add", "--help"]).output().unwrap();
    let help = stdout_of(&out);
    assert!(help.contains("DD-Mon-YY") && help.contains("two-digit") && help.contains("1000 to 9999"), "{help}");

    let sb = Sandbox::new();
    sb.cmd().args(["add", "e1", "-d", "31-01-2026"]).output().unwrap();
    let out = sb.cmd().args(["edit", "1", "-d", "+1m1m"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(date_of(&db_tasks(&sb), 1), "2026-03-31");
}
