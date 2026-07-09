//! JSON API over [`TaskManager`]. Handlers are transport-agnostic pure
//! functions returning `(status, JSON body)`, so they are unit-testable
//! without a running server and never print.

use crate::{Task, TaskId, TaskManager};
use chrono::NaiveDate;
use serde::Deserialize;

pub struct ApiResponse {
    pub status: u16,
    /// JSON body; empty for 204.
    pub body: String,
}

impl ApiResponse {
    fn json(status: u16, value: &impl serde::Serialize) -> Self {
        Self {
            status,
            body: serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()),
        }
    }

    fn no_content() -> Self {
        Self {
            status: 204,
            body: String::new(),
        }
    }

    pub fn error(status: u16, message: &str) -> Self {
        Self::json(status, &serde_json::json!({ "error": message }))
    }
}

#[derive(Deserialize)]
struct NewTask {
    text: String,
    #[serde(default)]
    date: Option<NaiveDate>,
}

/// PATCH body: absent fields stay untouched. `date` uses the double-Option
/// pattern so `"date": null` (clear) differs from the field being absent.
#[derive(Deserialize)]
pub struct TaskPatch {
    text: Option<String>,
    #[serde(default, deserialize_with = "some_option")]
    date: Option<Option<NaiveDate>>,
    done: Option<bool>,
    priority: Option<bool>,
}

fn some_option<'de, D>(deserializer: D) -> Result<Option<Option<NaiveDate>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<NaiveDate>::deserialize(deserializer)?))
}

fn save_or_500(tm: &TaskManager, ok: ApiResponse) -> ApiResponse {
    match tm.save() {
        Ok(()) => ok,
        Err(e) => ApiResponse::error(500, &e.to_string()),
    }
}

/// GET /api/tasks
pub fn list_tasks(tm: &TaskManager) -> ApiResponse {
    ApiResponse::json(200, &tm.tasks())
}

/// POST /api/tasks — `{"text": "...", "date": "YYYY-MM-DD" | null}`
pub fn create_task(tm: &mut TaskManager, body: &str) -> ApiResponse {
    let new: NewTask = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ApiResponse::error(400, &format!("invalid JSON: {e}")),
    };
    if new.text.trim().is_empty() {
        return ApiResponse::error(400, "Task text cannot be empty");
    }
    match tm.add_task_with_parsed_date(new.text, new.date) {
        Ok(()) => ApiResponse::json(201, &tm.tasks().last()),
        Err(e) => ApiResponse::error(500, &e.to_string()),
    }
}

/// PATCH /api/tasks/{id} — any subset of `{text, date, done, priority}`.
pub fn update_task(tm: &mut TaskManager, id: TaskId, body: &str) -> ApiResponse {
    let patch: TaskPatch = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ApiResponse::error(400, &format!("invalid JSON: {e}")),
    };
    let Some(idx) = tm.find_task_by_id(id) else {
        return ApiResponse::error(404, &format!("no task with id {id}"));
    };
    if let Some(text) = &patch.text
        && text.trim().is_empty()
    {
        return ApiResponse::error(400, "Task text cannot be empty");
    }

    let task = &mut tm.tasks_mut()[idx];
    if let Some(text) = patch.text {
        task.text = text;
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
    let updated = tm.tasks()[idx].clone();
    save_or_500(tm, ApiResponse::json(200, &updated))
}

/// DELETE /api/tasks/{id}
pub fn delete_task(tm: &mut TaskManager, id: TaskId) -> ApiResponse {
    if tm.find_task_by_id(id).is_none() {
        return ApiResponse::error(404, &format!("no task with id {id}"));
    }
    match tm.delete_tasks(vec![id]) {
        Ok(_) => ApiResponse::no_content(),
        Err(e) => ApiResponse::error(500, &e.to_string()),
    }
}

/// DELETE /api/tasks/done — parallels `rusk del --done`.
pub fn delete_done(tm: &mut TaskManager) -> ApiResponse {
    match tm.delete_all_done() {
        Ok(deleted) => ApiResponse::json(200, &serde_json::json!({ "deleted": deleted })),
        Err(e) => ApiResponse::error(500, &e.to_string()),
    }
}

/// PUT /api/tasks — replace the whole list (used by `rusk sync push`).
pub fn replace_tasks(tm: &mut TaskManager, body: &str) -> ApiResponse {
    let tasks: Vec<Task> = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return ApiResponse::error(400, &format!("invalid JSON: {e}")),
    };
    let mut ids: Vec<TaskId> = tasks.iter().map(|t| t.id).collect();
    ids.sort_unstable();
    if ids.first() == Some(&0) || ids.windows(2).any(|w| w[0] == w[1]) {
        return ApiResponse::error(400, "task ids must be unique and nonzero");
    }
    let count = tasks.len();
    tm.tasks = tasks;
    save_or_500(tm, ApiResponse::json(200, &serde_json::json!({ "count": count })))
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

        let res = create_task(&mut tm, r#"{"text":"buy milk","date":"2026-07-10"}"#);
        assert_eq!(res.status, 201);
        assert!(res.body.contains("buy milk"));

        let res = list_tasks(&tm);
        assert_eq!(res.status, 200);
        let tasks: Vec<Task> = serde_json::from_str(&res.body).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].date.unwrap().to_string(), "2026-07-10");

        let res = update_task(&mut tm, 1, r#"{"done":true,"priority":true}"#);
        assert_eq!(res.status, 200);
        assert!(tm.tasks()[0].done && tm.tasks()[0].priority);

        // Clearing the date requires an explicit null.
        let res = update_task(&mut tm, 1, r#"{"date":null}"#);
        assert_eq!(res.status, 200);
        assert_eq!(tm.tasks()[0].date, None);

        // Absent date stays untouched.
        let res = update_task(&mut tm, 1, r#"{"text":"renamed"}"#);
        assert_eq!(res.status, 200);
        assert_eq!(tm.tasks()[0].text, "renamed");

        let res = delete_task(&mut tm, 1);
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
        assert_eq!(create_task(&mut tm, "{}").status, 400);
        assert_eq!(create_task(&mut tm, r#"{"text":"  "}"#).status, 400);
        assert_eq!(
            create_task(&mut tm, r#"{"text":"x","date":"10-07-2026"}"#).status,
            400,
            "non-ISO dates are rejected"
        );
        assert_eq!(update_task(&mut tm, 9, r#"{"done":true}"#).status, 404);
        assert_eq!(delete_task(&mut tm, 9).status, 404);

        create_task(&mut tm, r#"{"text":"x"}"#);
        assert_eq!(update_task(&mut tm, 1, r#"{"text":""}"#).status, 400);
        assert_eq!(update_task(&mut tm, 1, "not json").status, 400);
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
                priority: false,
            });
        }
        assert_eq!(create_task(&mut tm, r#"{"text":"one more"}"#).status, 201);
        assert_eq!(tm.tasks().last().unwrap().id, 256);
    }

    #[test]
    fn delete_done_counts() {
        let (_dir, mut tm) = tm();
        create_task(&mut tm, r#"{"text":"a"}"#);
        create_task(&mut tm, r#"{"text":"b"}"#);
        update_task(&mut tm, 1, r#"{"done":true}"#);
        let res = delete_done(&mut tm);
        assert_eq!(res.status, 200);
        assert!(res.body.contains("\"deleted\":1"));
        assert_eq!(tm.tasks().len(), 1);
    }

    #[test]
    fn replace_tasks_validates() {
        let (_dir, mut tm) = tm();
        let res = replace_tasks(
            &mut tm,
            r#"[{"id":1,"text":"a","date":null,"done":false,"priority":false},
                {"id":2,"text":"b","date":"2026-01-01","done":true,"priority":false}]"#,
        );
        assert_eq!(res.status, 200);
        assert_eq!(tm.tasks().len(), 2);

        let dup = r#"[{"id":1,"text":"a","date":null,"done":false,"priority":false},
                      {"id":1,"text":"b","date":null,"done":false,"priority":false}]"#;
        assert_eq!(replace_tasks(&mut tm, dup).status, 400);
        assert_eq!(replace_tasks(&mut tm, "[{}]").status, 400);
    }
}
