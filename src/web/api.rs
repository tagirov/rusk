//! JSON API over [`TaskManager`]. Handlers are transport-agnostic pure
//! functions returning `(status, JSON body)`, so they are unit-testable
//! without a running server and never print.
//!
//! Responses name the revision of what they describe (`etag`, sent as the
//! `ETag` header): the list for `/api/tasks`, the task for
//! `/api/tasks/{id}`. A client that sends a revision back as `If-Match`
//! changes what it has seen, not whatever is there by now — ids are reused,
//! so a stale client may be pointing at a different task. A request on one
//! task is conditional on that task (or on the whole list, which is
//! stricter), a request on the list on the list. If the condition fails the
//! answer is 412 and nothing is written. Without `If-Match` a request
//! applies to whatever is there. Either way the change runs inside
//! [`TaskManager::update`], so it never overwrites what a CLI command saves
//! at the same moment.

use crate::model::normalize;
use crate::revision::{if_match_allows, list_revision, task_revision};
use crate::storage::{
    clean_text, insert_by_id, next_free_id, position_of, remove_done, remove_tasks, text_changes,
    validate_after,
};
use crate::{Task, TaskId, TaskManager};
use chrono::NaiveDate;
use serde::Deserialize;

pub struct ApiResponse {
    pub status: u16,
    /// JSON body; empty for 204.
    pub body: String,
    /// Revision of the resource as this response describes it or has left
    /// it (the `ETag` header): the list or the task.
    pub etag: Option<String>,
}

impl ApiResponse {
    fn json(status: u16, value: &impl serde::Serialize) -> Self {
        Self {
            status,
            body: serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()),
            etag: None,
        }
    }

    fn no_content() -> Self {
        Self {
            status: 204,
            body: String::new(),
            etag: None,
        }
    }

    pub fn error(status: u16, message: &str) -> Self {
        Self::json(status, &serde_json::json!({ "error": message }))
    }

    fn with_revision_of(mut self, tasks: &[Task]) -> Self {
        self.etag = list_revision(tasks).ok();
        self
    }

    fn task(status: u16, task: &Task) -> Self {
        let mut res = Self::json(status, task);
        res.etag = task_revision(task).ok();
        res
    }
}

/// A request the API turns down from inside an update; the status travels
/// through `anyhow` to [`finish`].
#[derive(Debug)]
struct Refusal {
    status: u16,
    message: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Refusal {}

fn refuse(status: u16, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(Refusal {
        status,
        message: message.into(),
    })
}

/// `If-Match`: the client means what it has seen, not whatever is there
/// now. A request on the list names the revision of the list; a request on
/// one task (`id`) may name the revision of that task instead, so changes
/// to other tasks do not get in its way. Runs inside the update, so nothing
/// can change between this check and the write.
fn check_if_match(tasks: &[Task], id: Option<TaskId>, if_match: Option<&str>) -> anyhow::Result<()> {
    let Some(header) = if_match else {
        return Ok(());
    };
    if if_match_allows(header, &list_revision(tasks)?) {
        return Ok(());
    }
    let task = id.and_then(|id| position_of(tasks, id)).map(|idx| &tasks[idx]);
    match task {
        Some(task) if if_match_allows(header, &task_revision(task)?) => Ok(()),
        Some(_) | None if id.is_some() => Err(refuse(
            412,
            "the task has changed since it was loaded; reload the list and try again",
        )),
        _ => Err(refuse(
            412,
            "the task list has changed since it was loaded; reload it and try again",
        )),
    }
}

/// Turns the outcome of an update into the response, which `ok` builds
/// from what the change returned.
fn finish<T>(result: anyhow::Result<T>, ok: impl FnOnce(T) -> ApiResponse) -> ApiResponse {
    match result {
        Ok(value) => ok(value),
        Err(e) => match e.downcast_ref::<Refusal>() {
            Some(refusal) => ApiResponse::error(refusal.status, &refusal.message),
            None if e.is::<crate::StaleDatabase>() => ApiResponse::error(409, &format!("{e:#}")),
            // The whole chain: "Failed to write the database file" alone
            // tells the client nothing, the cause under it does.
            None => ApiResponse::error(500, &format!("{e:#}")),
        },
    }
}

#[derive(Deserialize)]
struct NewTask {
    text: String,
    #[serde(default)]
    date: Option<NaiveDate>,
    #[serde(default)]
    after: Vec<TaskId>,
}

/// PATCH body: absent fields stay untouched. `date` uses the double-Option
/// pattern so `"date": null` (clear) differs from the field being absent.
/// `after` replaces the whole dependency list (`[]` clears it).
#[derive(Deserialize)]
pub struct TaskPatch {
    text: Option<String>,
    #[serde(default, deserialize_with = "some_option")]
    date: Option<Option<NaiveDate>>,
    done: Option<bool>,
    priority: Option<bool>,
    after: Option<Vec<TaskId>>,
}

fn some_option<'de, D>(deserializer: D) -> Result<Option<Option<NaiveDate>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<NaiveDate>::deserialize(deserializer)?))
}

/// GET /api/tasks
pub fn list_tasks(tm: &TaskManager) -> ApiResponse {
    ApiResponse::json(200, &tm.tasks()).with_revision_of(tm.tasks())
}

/// GET /api/tasks/{id} — one task; its `ETag` is what `If-Match` on a
/// PATCH or DELETE of the same task is compared with.
pub fn get_task(tm: &TaskManager, id: TaskId) -> ApiResponse {
    match tm.find_task_by_id(id) {
        Some(idx) => ApiResponse::task(200, &tm.tasks()[idx]),
        None => ApiResponse::error(404, &format!("no task with id {id}")),
    }
}

/// POST /api/tasks — `{"text": "...", "date": "YYYY-MM-DD" | null}`
pub fn create_task(tm: &mut TaskManager, body: &str, if_match: Option<&str>) -> ApiResponse {
    let new: NewTask = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ApiResponse::error(400, &format!("invalid JSON: {e}")),
    };
    if new.text.trim().is_empty() {
        return ApiResponse::error(400, "Task text cannot be empty");
    }
    let created = tm.update(|tasks| {
        check_if_match(tasks, None, if_match)?;
        // A bad dependency list is a client error, not a save failure.
        let after =
            validate_after(tasks, None, &new.after).map_err(|e| refuse(400, format!("{e:#}")))?;
        let task = Task {
            id: next_free_id(tasks)?,
            text: clean_text(&new.text).to_string(),
            date: new.date,
            done: false,
            priority: false,
            after,
        };
        insert_by_id(tasks, task.clone());
        Ok(task)
    });
    finish(created, |task| ApiResponse::task(201, &task))
}

/// PATCH /api/tasks/{id} — any subset of `{text, date, done, priority}`.
pub fn update_task(
    tm: &mut TaskManager,
    id: TaskId,
    body: &str,
    if_match: Option<&str>,
) -> ApiResponse {
    let patch: TaskPatch = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ApiResponse::error(400, &format!("invalid JSON: {e}")),
    };
    if let Some(text) = &patch.text
        && text.trim().is_empty()
    {
        return ApiResponse::error(400, "Task text cannot be empty");
    }
    let updated = tm.update(|tasks| {
        check_if_match(tasks, Some(id), if_match)?;
        let Some(idx) = position_of(tasks, id) else {
            return Err(refuse(404, format!("no task with id {id}")));
        };
        let new_after = match &patch.after {
            Some(list) => Some(
                validate_after(tasks, Some(id), list).map_err(|e| refuse(400, format!("{e:#}")))?,
            ),
            None => None,
        };

        let task = &mut tasks[idx];
        if let Some(text) = &patch.text
            && text_changes(&task.text, text)
        {
            task.text = clean_text(text).to_string();
        }
        if let Some(date) = patch.date {
            task.date = date;
        }
        if let Some(done) = patch.done {
            task.done = done;
        }
        if let Some(priority) = patch.priority {
            task.priority = priority;
        }
        if let Some(after) = new_after {
            task.after = after;
        }
        Ok(task.clone())
    });
    finish(updated, |task| ApiResponse::task(200, &task))
}

/// DELETE /api/tasks/{id}
pub fn delete_task(tm: &mut TaskManager, id: TaskId, if_match: Option<&str>) -> ApiResponse {
    let deleted = tm.update(|tasks| {
        check_if_match(tasks, Some(id), if_match)?;
        if !remove_tasks(tasks, &[id]).is_empty() {
            return Err(refuse(404, format!("no task with id {id}")));
        }
        Ok(())
    });
    finish(deleted, |()| ApiResponse::no_content())
}

/// DELETE /api/tasks/done — parallels `rusk del --done`.
pub fn delete_done(tm: &mut TaskManager, if_match: Option<&str>) -> ApiResponse {
    let deleted = tm.update(|tasks| {
        check_if_match(tasks, None, if_match)?;
        Ok(remove_done(tasks))
    });
    let tasks = tm.tasks();
    finish(deleted, |deleted| {
        ApiResponse::json(200, &serde_json::json!({ "deleted": deleted })).with_revision_of(tasks)
    })
}

/// PUT /api/tasks — replace the whole list (the http database backend and
/// `rusk sync push`). With `If-Match` this is a compare-and-swap: a list
/// derived from an older revision never replaces a newer one.
///
/// The list is stored exactly as sent — sync compares what it pushed with
/// what it reads back — so it must follow the rules every loaded list is
/// made to follow ([`normalize`]): a list that a load would have to repair
/// is refused, with what is wrong.
pub fn replace_tasks(tm: &mut TaskManager, body: &str, if_match: Option<&str>) -> ApiResponse {
    let new: Vec<Task> = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ApiResponse::error(400, &format!("invalid JSON: {e}")),
    };
    match normalize(&mut new.clone()) {
        Ok(repairs) if repairs.is_empty() => {}
        Ok(repairs) => {
            return ApiResponse::error(400, &format!("not a valid task list: {}", repairs.problems()));
        }
        Err(e) => return ApiResponse::error(400, &format!("{e:#}")),
    }
    let replaced = tm.update(|tasks| {
        check_if_match(tasks, None, if_match)?;
        *tasks = new.clone();
        Ok(tasks.len())
    });
    // What the database holds now — its format may hold less than it was
    // sent — worked out, not read back: `rusk sync` records it as the state
    // of this side, and a read would hold another writer's change too.
    let held = tm.stored_form(&new);
    let tasks = tm.tasks();
    finish(replaced, |count| {
        ApiResponse::json(200, &serde_json::json!({ "count": count, "tasks": held }))
            .with_revision_of(tasks)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tm() -> (tempfile::TempDir, TaskManager) {
        let dir = tempfile::tempdir().unwrap();
        let tm = TaskManager::new_empty_with_path(dir.path().join("tasks.json"));
        (dir, tm)
    }

    #[test]
    fn crud_roundtrip() {
        let (_dir, mut tm) = tm();

        let res = create_task(&mut tm, r#"{"text":"buy milk","date":"2026-07-10"}"#, None);
        assert_eq!(res.status, 201);
        assert!(res.body.contains("buy milk"));

        let res = list_tasks(&tm);
        assert_eq!(res.status, 200);
        let tasks: Vec<Task> = serde_json::from_str(&res.body).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].date.unwrap().to_string(), "2026-07-10");

        let res = update_task(&mut tm, 1, r#"{"done":true,"priority":true}"#, None);
        assert_eq!(res.status, 200);
        assert!(tm.tasks()[0].done && tm.tasks()[0].priority);

        // Clearing the date requires an explicit null.
        let res = update_task(&mut tm, 1, r#"{"date":null}"#, None);
        assert_eq!(res.status, 200);
        assert_eq!(tm.tasks()[0].date, None);

        // Absent date stays untouched.
        let res = update_task(&mut tm, 1, r#"{"text":"renamed"}"#, None);
        assert_eq!(res.status, 200);
        assert_eq!(tm.tasks()[0].text, "renamed");

        let res = delete_task(&mut tm, 1, None);
        assert_eq!(res.status, 204);
        assert!(tm.tasks().is_empty());

        // Changes persisted to disk.
        let path = tm.local_path().unwrap().to_path_buf();
        let loaded = TaskManager::load_tasks_from_path(&path).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn errors() {
        let (_dir, mut tm) = tm();
        assert_eq!(create_task(&mut tm, "{}", None).status, 400);
        assert_eq!(create_task(&mut tm, r#"{"text":"  "}"#, None).status, 400);
        assert_eq!(
            create_task(&mut tm, r#"{"text":"x","date":"10-07-2026"}"#, None).status,
            400,
            "non-ISO dates are rejected"
        );
        assert_eq!(update_task(&mut tm, 9, r#"{"done":true}"#, None).status, 404);
        assert_eq!(delete_task(&mut tm, 9, None).status, 404);

        create_task(&mut tm, r#"{"text":"x"}"#, None);
        assert_eq!(update_task(&mut tm, 1, r#"{"text":""}"#, None).status, 400);
        assert_eq!(update_task(&mut tm, 1, "not json", None).status, 400);
    }

    #[test]
    fn ids_grow_past_255() {
        let (_dir, mut tm) = tm();
        for i in 1..=255u32 {
            tm.tasks.push(Task {
                id: i,
                text: format!("t{i}"),
                date: None,
                done: false,
                priority: false, after: Vec::new(),
            });
        }
        assert_eq!(create_task(&mut tm, r#"{"text":"one more"}"#, None).status, 201);
        assert_eq!(tm.tasks().last().unwrap().id, 256);
    }

    #[test]
    fn delete_done_counts() {
        let (_dir, mut tm) = tm();
        create_task(&mut tm, r#"{"text":"a"}"#, None);
        create_task(&mut tm, r#"{"text":"b"}"#, None);
        update_task(&mut tm, 1, r#"{"done":true}"#, None);
        let res = delete_done(&mut tm, None);
        assert_eq!(res.status, 200);
        assert!(res.body.contains("\"deleted\":1"));
        assert_eq!(tm.tasks().len(), 1);
    }

    fn quoted(etag: &Option<String>) -> String {
        format!("\"{}\"", etag.as_deref().expect("the response names a revision"))
    }

    /// REVIEW №162: a client that sends back the revision it has seen never
    /// changes a list it has not seen.
    #[test]
    fn if_match_on_the_list_guards_every_change() {
        let (_dir, mut tm) = tm();
        create_task(&mut tm, r#"{"text":"alpha"}"#, None);
        create_task(&mut tm, r#"{"text":"beta"}"#, None);
        let seen = quoted(&list_tasks(&tm).etag);

        // Elsewhere: task 2 is deleted and its id goes to a new task.
        assert_eq!(delete_task(&mut tm, 2, None).status, 204);
        assert_eq!(create_task(&mut tm, r#"{"text":"BRAND NEW"}"#, None).status, 201);
        let current = list_tasks(&tm);

        // The stale client tries to delete / rewrite / complete "beta".
        let stale = Some(seen.as_str());
        assert_eq!(delete_task(&mut tm, 2, stale).status, 412);
        assert_eq!(update_task(&mut tm, 2, r#"{"text":"beta!"}"#, stale).status, 412);
        assert_eq!(update_task(&mut tm, 2, r#"{"done":true}"#, stale).status, 412);
        assert_eq!(update_task(&mut tm, 7, r#"{"done":true}"#, stale).status, 412);
        assert_eq!(delete_done(&mut tm, stale).status, 412);
        assert_eq!(replace_tasks(&mut tm, "[]", stale).status, 412);
        assert_eq!(create_task(&mut tm, r#"{"text":"x"}"#, stale).status, 412);
        assert_eq!(list_tasks(&tm).body, current.body, "a refused request changed something");

        // With the current revision the same requests go through; answers
        // about the list name the revision they left behind.
        let fresh = quoted(&current.etag);
        assert_eq!(update_task(&mut tm, 2, r#"{"done":true}"#, Some(&fresh)).status, 200);
        let fresh = quoted(&list_tasks(&tm).etag);
        let res = delete_done(&mut tm, Some(&fresh));
        assert_eq!(res.status, 200);
        assert_eq!(res.etag, list_tasks(&tm).etag);
        let res = replace_tasks(&mut tm, "[]", Some(&quoted(&res.etag)));
        assert_eq!(res.status, 200);
        assert_eq!(res.etag, list_tasks(&tm).etag);

        // `*` and a list of tags are valid If-Match values too.
        create_task(&mut tm, r#"{"text":"again"}"#, None);
        assert_eq!(update_task(&mut tm, 1, r#"{"done":true}"#, Some("*")).status, 200);
        let tags = format!("\"0000\", W/{}", quoted(&list_tasks(&tm).etag));
        assert_eq!(update_task(&mut tm, 1, r#"{"done":false}"#, Some(&tags)).status, 200);
    }

    /// A request on one task may be conditional on that task alone: changes
    /// to other tasks do not refuse it (two quick taps on a page, a CLI
    /// `add` while an edit dialog is open), a change to the task itself —
    /// or its id going to another task — does.
    #[test]
    fn if_match_on_a_task_ignores_changes_to_other_tasks() {
        let (_dir, mut tm) = tm();
        create_task(&mut tm, r#"{"text":"alpha"}"#, None);
        let beta = create_task(&mut tm, r#"{"text":"beta"}"#, None);
        let beta_seen = quoted(&beta.etag);
        assert_eq!(beta.etag, get_task(&tm, 2).etag, "POST names the revision of the new task");
        assert_eq!(get_task(&tm, 9).status, 404);

        // The rest of the list moves on; task 2 is still what was seen.
        create_task(&mut tm, r#"{"text":"added elsewhere"}"#, None);
        update_task(&mut tm, 1, r#"{"done":true}"#, None);
        let res = update_task(&mut tm, 2, r#"{"priority":true}"#, Some(&beta_seen));
        assert_eq!(res.status, 200, "{}", res.body);
        assert_eq!(res.etag, get_task(&tm, 2).etag, "PATCH names the new revision of the task");
        assert_ne!(quoted(&res.etag), beta_seen);

        // That was a change to task 2: the revision seen before is stale...
        assert_eq!(update_task(&mut tm, 2, r#"{"text":"x"}"#, Some(&beta_seen)).status, 412);
        assert_eq!(delete_task(&mut tm, 2, Some(&beta_seen)).status, 412);
        // ...the new one is not.
        let beta_now = quoted(&res.etag);
        assert_eq!(delete_task(&mut tm, 2, Some(&beta_now)).status, 204);

        // The id goes to another task: no revision of "beta" fits it, and a
        // task that is gone is a failed precondition, not a 404.
        assert_eq!(delete_task(&mut tm, 2, Some(&beta_now)).status, 412);
        create_task(&mut tm, r#"{"text":"BRAND NEW"}"#, None);
        // Listed where the id puts it (REVIEW №133).
        assert_eq!(tm.tasks()[1].id, 2);
        assert_eq!(update_task(&mut tm, 2, r#"{"done":true}"#, Some(&beta_now)).status, 412);
        assert_eq!(delete_task(&mut tm, 2, Some(&beta_now)).status, 412);
        assert_eq!(tm.tasks()[1].text, "BRAND NEW");

        // A task revision says nothing about the list.
        let brand_new = quoted(&get_task(&tm, 2).etag);
        assert_eq!(replace_tasks(&mut tm, "[]", Some(&brand_new)).status, 412);
        assert_eq!(delete_done(&mut tm, Some(&brand_new)).status, 412);
    }

    /// The server reloads per request, but a handler must not depend on it:
    /// a manager whose snapshot went stale applies the request to the
    /// current list (validation included) instead of writing the snapshot.
    #[test]
    fn requests_apply_to_the_current_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let mut seed = TaskManager::open_at(path.clone()).unwrap();
        create_task(&mut seed, r#"{"text":"alpha"}"#, None);
        create_task(&mut seed, r#"{"text":"beta"}"#, None);

        let mut stale = TaskManager::open_at(path.clone()).unwrap();
        let mut cli = TaskManager::open_at(path.clone()).unwrap();
        cli.add_task(vec!["from the cli".into()], None).unwrap();
        cli.delete_tasks(vec![1]).unwrap();

        assert_eq!(update_task(&mut stale, 1, r#"{"done":true}"#, None).status, 404);
        assert_eq!(create_task(&mut stale, r#"{"text":"x","after":[1]}"#, None).status, 400);
        let res = update_task(&mut stale, 2, r#"{"priority":true}"#, None);
        assert_eq!(res.status, 200);
        let res = create_task(&mut stale, r#"{"text":"from the web"}"#, None);
        assert_eq!(res.status, 201);
        assert!(res.body.contains(r#""id":1"#), "the freed id is reused: {}", res.body);

        let on_disk = TaskManager::load_tasks_from_path(&path).unwrap();
        let texts: Vec<&str> = on_disk.iter().map(|t| t.text.as_str()).collect();
        // The reused id 1 is listed first (REVIEW №133).
        assert_eq!(texts, ["from the web", "beta", "from the cli"]);
        assert!(on_disk[1].priority);
    }

    /// REVIEW №196: a PATCH that changes nothing (the dialog saved as it
    /// was opened, `{}` from a script) is answered like any other, but
    /// writes nothing: the one-level `.backup` keeps the state before the
    /// last real change, and `git_backend` gets no empty commit.
    #[test]
    fn a_patch_that_changes_nothing_writes_nothing() {
        let (dir, mut tm) = tm();
        for text in ["one", "two", "three"] {
            create_task(&mut tm, &format!(r#"{{"text":"{text}"}}"#), None);
        }
        assert_eq!(delete_task(&mut tm, 3, None).status, 204);
        let backup = dir.path().join("tasks.json.backup");
        let before = std::fs::read_to_string(&backup).unwrap();
        assert!(before.contains("three"), "the backup holds the state before the delete");

        let res = update_task(&mut tm, 1, "{}", None);
        assert_eq!(res.status, 200, "{}", res.body);
        assert_eq!(res.etag, get_task(&tm, 1).etag);
        let res = update_task(&mut tm, 1, r#"{"text":"one","date":null,"priority":false}"#, None);
        assert_eq!(res.status, 200, "{}", res.body);

        assert_eq!(std::fs::read_to_string(&backup).unwrap(), before);
    }

    /// REVIEW №22: the error body carries the cause, not only the step that
    /// failed. (A file in place of the database directory fails for every
    /// user, root included.)
    #[test]
    fn a_failed_save_tells_the_client_why() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("not-a-dir");
        std::fs::write(&blocker, "").unwrap();
        let mut tm = TaskManager::new_empty_with_path(blocker.join("tasks.json"));

        let res = create_task(&mut tm, r#"{"text":"x"}"#, None);
        assert_eq!(res.status, 500);
        assert!(res.body.contains("not-a-dir"), "{}", res.body);
        assert!(res.body.contains("os error"), "{}", res.body);
    }

    #[test]
    fn replace_tasks_validates() {
        let (_dir, mut tm) = tm();
        let res = replace_tasks(
            &mut tm,
            r#"[{"id":1,"text":"a","date":null,"done":false,"priority":false},
                {"id":2,"text":"b","date":"2026-01-01","done":true,"priority":false}]"#,
            None,
        );
        assert_eq!(res.status, 200);
        assert_eq!(tm.tasks().len(), 2);

        let dup = r#"[{"id":1,"text":"a","date":null,"done":false,"priority":false},
                      {"id":1,"text":"b","date":null,"done":false,"priority":false}]"#;
        assert_eq!(replace_tasks(&mut tm, dup, None).status, 400);
        assert_eq!(replace_tasks(&mut tm, "[{}]", None).status, 400);
    }
}
