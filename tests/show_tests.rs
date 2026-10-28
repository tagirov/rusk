//! `rusk <IDS>`: the tasks with these ids, or the ids no task has; with
//! `--json`, the tasks as JSON lines, which `rusk add --json` adds to
//! another database (a copy through a pipe).

use std::io::Write;
use std::process::{Output, Stdio};

mod common;
use common::Sandbox;

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn db_tasks(sb: &Sandbox) -> Vec<serde_json::Value> {
    serde_json::from_str(&sb.read_db()).expect("the database must stay valid JSON")
}

fn run(sb: &Sandbox, args: &[&str]) -> Output {
    sb.cmd().args(args).stdin(Stdio::null()).output().unwrap()
}

/// `rusk add --json` in `sb`, `input` on its standard input.
fn add_json(sb: &Sandbox, input: &str) -> Output {
    let mut child = sb
        .cmd()
        .args(["add", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

/// Task 1 open; task 2 due, with priority, waiting for task 1; task 3 done
/// on a day on record.
fn source() -> Sandbox {
    Sandbox::with_db(
        r#"[{"id":1,"text":"base"},
            {"id":2,"text":"deploy","date":"2026-11-10","priority":true,"after":[1]},
            {"id":3,"text":"shipped","done":true,"done_on":"2026-10-25"}]"#,
    )
}

#[test]
fn an_id_shows_that_task_alone() {
    let sb = source();
    let out = run(&sb, &["2"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let said = stdout_of(&out);
    assert!(said.contains("deploy (1)"), "{said}");
    assert!(!said.contains("base") && !said.contains("shipped"), "{said}");
    assert!(!said.contains("not found"), "{said}");
}

#[test]
fn ids_no_task_has_are_said_after_the_ones_found() {
    let sb = source();
    let out = run(&sb, &["3,", "9"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let said = stdout_of(&out);
    let shipped = said.find("shipped").expect(&said);
    let missing = said.find("Tasks not found IDs: 9").expect(&said);
    assert!(shipped < missing, "{said}");
    assert!(!said.contains("deploy"), "{said}");
}

#[test]
fn an_id_no_task_has_is_not_found_and_nothing_else() {
    let sb = source();
    let before = sb.read_db();
    let out = run(&sb, &["9"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert_eq!(stdout_of(&out).trim(), "Tasks not found IDs: 9");
    assert_eq!(sb.read_db(), before);
}

/// A word that does not start like an id gets clap's answer to a command
/// it does not know, the tip on a similar one included, as before IDS.
#[test]
fn a_mistyped_command_is_still_an_unknown_command() {
    let sb = source();
    for args in [&["lsit"][..], &["lsit", "--json"], &["--json", "lsit"]] {
        let out = run(&sb, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let said = stderr_of(&out);
        assert!(said.contains("unrecognized subcommand 'lsit'"), "{args:?}: {said}");
        assert!(said.contains("'list'"), "{args:?}: {said}");
    }
}

#[test]
fn a_wrong_id_list_is_an_error_before_the_database() {
    let sb = source();
    for (args, what) in [
        (&["1,x"][..], "'x' in '1,x' is not a task id"),
        (&["1", "2"], "unexpected argument '2' after the ids '1'"),
        (&["3", "list"], "unexpected argument 'list' after the ids '3'"),
    ] {
        let out = run(&sb, args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        assert!(stderr_of(&out).contains(what), "{args:?}: {}", stderr_of(&out));
        assert!(stdout_of(&out).is_empty(), "{args:?}");
    }
}

#[test]
fn json_needs_ids_and_add_json_takes_nothing_else() {
    let sb = source();
    assert_eq!(run(&sb, &["--json"]).status.code(), Some(2));
    for args in [&["add", "--json", "text"][..], &["add", "--json", "-d", "2w"], &["add", "--json", "-a", "1"]] {
        let out = run(&sb, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(stderr_of(&out).contains("cannot be used with"), "{args:?}: {}", stderr_of(&out));
    }
}

/// stdout is the data: one task per line, in the database's order, with
/// every field; the ids no task has go to stderr.
#[test]
fn json_prints_the_tasks_as_lines_and_the_missing_ids_on_stderr() {
    let sb = source();
    let out = run(&sb, &["3,9,2", "--json"]);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let lines: Vec<serde_json::Value> = stdout_of(&out)
        .lines()
        .map(|line| serde_json::from_str(line).expect(line))
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["id"], 2);
    assert_eq!(lines[0]["date"], "2026-11-10");
    assert_eq!(lines[0]["priority"], true);
    assert_eq!(lines[0]["after"], serde_json::json!([1]));
    assert_eq!(lines[1]["id"], 3);
    assert_eq!(lines[1]["done_on"], "2026-10-25");
    assert!(stderr_of(&out).contains("Tasks not found IDs: 9"), "{}", stderr_of(&out));
}

/// The whole pipe: each copy gets the target's next free id and keeps its
/// fields; a dependency that comes along points at the new id, one that
/// stays behind is dropped and said.
#[test]
fn a_copy_through_the_pipe_lands_with_new_ids() {
    let src = source();
    let dst = Sandbox::with_db(r#"[{"id":1,"text":"already here"}]"#);

    let alone = stdout_of(&run(&src, &["2", "--json"]));
    let out = add_json(&dst, &alone);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let said = stdout_of(&out);
    assert!(said.contains("Added task: 2:"), "{said}");
    assert!(said.contains("Task 2 no longer depends on 1: not among the copied tasks."), "{said}");

    let all = stdout_of(&run(&src, &["1,2,3", "--json"]));
    let out = add_json(&dst, &all);
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(!stdout_of(&out).contains("no longer depends"), "{}", stdout_of(&out));

    let tasks = db_tasks(&dst);
    let ids: Vec<u64> = tasks.iter().map(|t| t["id"].as_u64().unwrap()).collect();
    assert_eq!(ids, [1, 2, 3, 4, 5]);
    assert_eq!(tasks[0]["text"], "already here");
    assert_eq!(tasks[1]["text"], "deploy");
    assert!(tasks[1].get("after").is_none(), "{}", tasks[1]);
    assert_eq!(tasks[2]["text"], "base");
    assert_eq!(tasks[3]["text"], "deploy");
    assert_eq!(tasks[3]["after"], serde_json::json!([3]));
    assert_eq!(tasks[3]["date"], "2026-11-10");
    assert_eq!(tasks[3]["priority"], true);
    assert_eq!(tasks[4]["text"], "shipped");
    assert_eq!(tasks[4]["done"], true);
    assert_eq!(tasks[4]["done_on"], "2026-10-25");
    // The source is only read.
    assert_eq!(db_tasks(&src).len(), 3);
}

/// A source id given twice belongs to the first task with it; a line
/// without an id is a task no other depends on; a completion day on an
/// open task is dropped, as on load.
#[test]
fn hand_written_lines_are_read_as_the_database_reads_them() {
    let dst = Sandbox::with_db("[]");
    let input = r#"{"id":7,"text":"first seven"}
{"id":7,"text":"second seven","after":[7]}

{"text":"no id","after":[7],"done_on":"2026-10-25"}
"#;
    let out = add_json(&dst, input);
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&dst);
    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[1]["after"], serde_json::json!([1]));
    assert_eq!(tasks[2]["after"], serde_json::json!([1]));
    assert!(tasks[2].get("done_on").is_none(), "{}", tasks[2]);
}

/// Nothing is added unless every line is a task that can be added.
#[test]
fn an_input_that_is_not_all_tasks_adds_nothing() {
    let dst = Sandbox::with_db(r#"[{"id":1,"text":"already here"}]"#);
    let before = dst.read_db();
    for (input, what) in [
        ("", "no tasks on standard input"),
        ("\n  \n", "no tasks on standard input"),
        ("{\"text\":\"fine\"}\nnot json\n", "NDJSON line 2"),
        ("{\"text\":\"fine\"}\n{\"text\":\"  \"}\n", "Task text cannot be empty (task 2 of the input)"),
        ("{\"id\":5,\"text\":\"s\",\"after\":[5]}\n", "Task 5 of the input cannot depend on itself"),
        (
            "{\"id\":1,\"text\":\"a\",\"after\":[2]}\n{\"id\":2,\"text\":\"b\",\"after\":[1]}\n",
            "Dependency cycle in the input",
        ),
    ] {
        let out = add_json(&dst, input);
        assert_eq!(out.status.code(), Some(1), "{input:?}");
        assert!(stderr_of(&out).contains(what), "{input:?}: {}", stderr_of(&out));
        assert_eq!(dst.read_db(), before, "{input:?}");
    }
}
