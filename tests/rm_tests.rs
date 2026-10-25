//! `rusk rm done`: the completed tasks — all of them, or with `--older` those
//! marked done a while ago — and the day `rusk mark` records for that.

use chrono::{Duration, Local, NaiveDate};

mod common;
use common::Sandbox;

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn db_tasks(sb: &Sandbox) -> Vec<serde_json::Value> {
    serde_json::from_str(&sb.read_db()).expect("the database must stay valid JSON")
}

fn ids_of(tasks: &[serde_json::Value]) -> Vec<u64> {
    tasks.iter().map(|t| t["id"].as_u64().unwrap()).collect()
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn days_ago(days: i64) -> String {
    (today() - Duration::days(days)).format("%Y-%m-%d").to_string()
}

/// The day `days` ago as the command line takes a date.
fn dmy(days: i64) -> String {
    (today() - Duration::days(days)).format("%d-%m-%Y").to_string()
}

/// Open task 1; task 2 done ten days ago, task 3 three days ago, task 4
/// done with no day on record; task 5 waits for task 2.
fn aged_db() -> Sandbox {
    Sandbox::with_db(&format!(
        r#"[{{"id":1,"text":"open"}},
            {{"id":2,"text":"done long ago","done":true,"done_on":"{}"}},
            {{"id":3,"text":"done lately","done":true,"done_on":"{}"}},
            {{"id":4,"text":"done, day unknown","done":true}},
            {{"id":5,"text":"waits for 2","after":[2]}}]"#,
        days_ago(10),
        days_ago(3)
    ))
}

#[test]
fn rm_deletes_every_done_task_dated_or_not() {
    let sb = aged_db();
    let out = sb.cmd().args(["rm", "done", "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let said = stdout_of(&out);
    assert!(said.contains("Deleted 3 done task(s)."), "{said}");
    assert!(said.contains("Task 5 no longer depends on 2."), "{said}");
    assert!(!said.contains("Kept"), "{said}");
    let tasks = db_tasks(&sb);
    assert_eq!(ids_of(&tasks), [1, 5]);
    assert!(tasks[1].get("after").is_none(), "{tasks:?}");
}

#[test]
fn rm_older_deletes_the_old_dated_ones_and_names_the_undated() {
    let sb = aged_db();
    let out = sb.cmd().args(["rm", "done", "--older", "7d", "--yes"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let said = stdout_of(&out);
    assert!(said.contains("Deleted 1 done task(s)."), "{said}");
    assert!(said.contains("Task 5 no longer depends on 2."), "{said}");
    assert!(said.contains("Kept: 1 done task has no day on record"), "{said}");
    assert!(said.contains("`rusk rm done` without --older"), "{said}");
    assert_eq!(ids_of(&db_tasks(&sb)), [1, 3, 4, 5]);

    // The boundary is inclusive: done exactly that long ago counts.
    let out = sb.cmd().args(["rm", "done", "--older", "3d", "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Deleted 1 done task(s)."), "{}", stdout_of(&out));
    assert_eq!(ids_of(&db_tasks(&sb)), [1, 4, 5]);
}

/// A bound is a date as well as an age, and `--newer` makes the other end
/// of a range; both ends are inclusive.
#[test]
fn rm_bounds_take_dates_and_make_ranges() {
    let sb = aged_db();
    // Older than a date: the one done ten days ago (the undated is kept).
    let out = sb.cmd().args(["rm", "done", "--older", &dmy(5), "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Deleted 1 done task(s)."), "{}", stdout_of(&out));
    assert_eq!(ids_of(&db_tasks(&sb)), [1, 3, 4, 5]);
    // Newer than a week: the one done three days ago.
    let out = sb.cmd().args(["rm", "done", "--newer", "1w", "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let said = stdout_of(&out);
    assert!(said.contains("Deleted 1 done task(s).") && said.contains("Kept: 1 done task"), "{said}");
    assert_eq!(ids_of(&db_tasks(&sb)), [1, 4, 5]);

    // A range, as ages and as dates, around the task done ten days ago.
    let sb = aged_db();
    let out = sb.cmd().args(["rm", "done", "--newer", "2w", "--older", "5d", "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Deleted 1 done task(s)."), "{}", stdout_of(&out));
    assert_eq!(ids_of(&db_tasks(&sb)), [1, 3, 4, 5]);
    let out = sb.cmd().args(["rm", "done", "--newer", &dmy(3), "--older", &dmy(3), "--yes"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Deleted 1 done task(s)."), "{}", stdout_of(&out));
    assert_eq!(ids_of(&db_tasks(&sb)), [1, 4, 5]);
    // A range with nothing in it says what it looked for.
    let out = sb.cmd().args(["rm", "done", "--newer", "3d", "--older", "today", "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(
        stdout_of(&out).contains("No done tasks older than today and newer than 3d to delete."),
        "{}",
        stdout_of(&out)
    );
}

/// An empty range, or a bound that is neither an age nor a date, is a
/// wrong argument: refused before the database is touched.
#[test]
fn rm_refuses_an_empty_range_and_a_bound_that_is_nothing() {
    let sb = aged_db();
    let out = sb.cmd().args(["rm", "done", "--newer", "7d", "--older", "1m", "-y"]).output().unwrap();
    assert!(!out.status.success(), "an empty range must be refused");
    assert!(stderr_of(&out).contains("empty range"), "{}", stderr_of(&out));
    let out = sb.cmd().args(["rm", "done", "--newer", "yesterday", "-y"]).output().unwrap();
    assert!(!out.status.success());
    let err = stderr_of(&out);
    assert!(err.contains("--newer") && err.contains("neither an age") && err.contains("nor a date"), "{err}");
    let out = sb.cmd().args(["rm", "1", "--newer", "7d", "-y"]).output().unwrap();
    assert!(!out.status.success(), "--newer goes with done");
    assert!(stderr_of(&out).contains("go with the word `done`"), "{}", stderr_of(&out));
    assert_eq!(db_tasks(&sb).len(), 5, "nothing may be deleted");
}

#[test]
fn rm_older_with_nothing_old_enough_says_so_and_changes_nothing() {
    let sb = aged_db();
    let before = sb.read_db();
    let out = sb.cmd().args(["rm", "done", "--older", "1m", "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let said = stdout_of(&out);
    assert!(said.contains("No done tasks older than 1m to delete."), "{said}");
    assert!(said.contains("Kept: 1 done task has no day on record"), "{said}");
    assert_eq!(sb.read_db(), before, "nothing to delete, nothing to write");
}

#[test]
fn rm_with_nothing_done_says_so() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"open"}]"#);
    let out = sb.cmd().args(["rm", "done", "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("No done tasks to delete."), "{}", stdout_of(&out));
    assert_eq!(ids_of(&db_tasks(&sb)), [1]);
}

/// A wrong age is a wrong argument: said as such, before the database is
/// touched; `rm` takes no ids.
#[test]
fn rm_rejects_a_bad_age_and_ids() {
    let sb = aged_db();
    for (age, says) in [("7x", "unknown unit"), ("7", "expected unit"), ("0d", "positive"), ("", "neither an age")] {
        let out = sb.cmd().args(["rm", "done", "--older", age, "-y"]).output().unwrap();
        assert!(!out.status.success(), "{age:?} must fail");
        let err = stderr_of(&out);
        assert!(err.contains("--older") && err.contains(says), "{age:?}: {err}");
        assert!(!err.contains("relative date"), "{age:?}: {err}");
    }
    // `rm` takes ids or the one word `done`, not both and not nothing;
    // `--older` goes with `done`.
    let out = sb.cmd().args(["rm", "done", "2", "-y"]).output().unwrap();
    assert!(!out.status.success(), "ids after done must be refused");
    assert!(stderr_of(&out).contains("takes nothing more"), "{}", stderr_of(&out));
    let out = sb.cmd().args(["rm", "-y"]).output().unwrap();
    assert!(!out.status.success(), "something to remove is required");
    assert!(stderr_of(&out).contains("rusk rm done"), "{}", stderr_of(&out));
    let out = sb.cmd().args(["rm", "1", "--older", "7d", "-y"]).output().unwrap();
    assert!(!out.status.success(), "--older goes with done");
    assert!(stderr_of(&out).contains("go with the word `done`"), "{}", stderr_of(&out));
    assert_eq!(db_tasks(&sb).len(), 5, "nothing may be deleted");
}

/// `rm` is the one deletion: by id, or the completed tasks with `done`.
#[test]
fn rm_deletes_by_id_and_the_completed_alike() {
    let sb = aged_db();
    let out = sb.cmd().args(["rm", "1", "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Deleted 1 task(s)."), "{}", stdout_of(&out));
    let out = sb.cmd().args(["rm", "3,4", "--yes"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("Deleted 2 task(s)."), "{}", stdout_of(&out));
    let out = sb.cmd().args(["rm", "done", "-y"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let said = stdout_of(&out);
    assert!(said.contains("Deleted 1 done task(s).") && said.contains("Task 5 no longer depends on 2."), "{said}");
    assert_eq!(ids_of(&db_tasks(&sb)), [5]);
}

#[test]
fn mark_records_the_day_and_unmarking_forgets_it() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"open"}]"#);
    let out = sb.cmd().args(["mark", "1"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let task = &db_tasks(&sb)[0];
    assert_eq!(task["done"], true);
    assert_eq!(task["done_on"], today().format("%Y-%m-%d").to_string());

    let out = sb.cmd().args(["mark", "1"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let task = &db_tasks(&sb)[0];
    assert_eq!(task["done"], false);
    assert!(task.get("done_on").is_none(), "{task}");
}

/// A completion date belongs to a done task: one on an open task is
/// dropped when the list is read, and gone with the next save.
#[test]
fn an_open_task_loses_a_stray_done_on() {
    let sb = Sandbox::with_db(r#"[{"id":1,"text":"open","done_on":"2026-01-01"}]"#);
    let out = sb.cmd().args(["add", "another"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let tasks = db_tasks(&sb);
    assert_eq!(tasks[0]["done"], false);
    assert!(tasks[0].get("done_on").is_none(), "{tasks:?}");
}
