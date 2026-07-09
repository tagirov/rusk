use anyhow::Result;
use colored::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::backend::Backend;
use crate::model::{Task, TaskId};
use crate::parse_cli_date_for_edit;
use crate::parser::date::is_cli_date_clear_value;

pub type MarkResult = (Vec<(TaskId, bool)>, Vec<TaskId>);

/// Manages task operations and persistence
pub struct TaskManager {
    pub tasks: Vec<Task>,
    backend: Backend,
}

fn eprint_db_location(location: &str) {
    eprintln!("{}", format!("Database path: {location}").blue());
}

struct DbReporter {
    location: String,
}

impl Drop for DbReporter {
    fn drop(&mut self) {
        eprint_db_location(&self.location);
    }
}

impl TaskManager {
    fn is_test_mode() -> bool {
        crate::is_test_mode()
    }

    fn maybe_log_db_location(location: &str) {
        static REPORTER: OnceLock<DbReporter> = OnceLock::new();
        if Self::is_test_mode() {
            let _ = REPORTER.get_or_init(|| DbReporter {
                location: location.to_string(),
            });
        } else if cfg!(debug_assertions) {
            eprint_db_location(location);
        }
    }

    fn create_sample_tasks() -> Vec<Task> {
        let today = chrono::Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let tomorrow = today + chrono::Duration::days(1);
        let next_week = today + chrono::Duration::days(7);
        let last_week = today - chrono::Duration::days(7);

        vec![
            Task { id: 1, text: "Simple task without date".to_string(), date: None, done: false, priority: false },
            Task { id: 2, text: "Completed task without date".to_string(), date: None, done: true, priority: false },
            Task { id: 3, text: "Overdue task from last week".to_string(), date: Some(last_week), done: false, priority: true },
            Task { id: 4, text: "Completed overdue task".to_string(), date: Some(yesterday), done: true, priority: false },
            Task { id: 5, text: "Task due today".to_string(), date: Some(today), done: false, priority: true },
            Task { id: 6, text: "Completed task due today".to_string(), date: Some(today), done: true, priority: false },
            Task { id: 7, text: "Task due tomorrow".to_string(), date: Some(tomorrow), done: false, priority: false },
            Task { id: 8, text: "Completed future task".to_string(), date: Some(next_week), done: true, priority: false },
            Task { id: 9, text: "Short".to_string(), date: None, done: false, priority: false },
            Task { id: 10, text: "This is a very long task description that contains multiple words and demonstrates how the system handles longer text content".to_string(), date: Some(tomorrow), done: false, priority: false },
            Task { id: 11, text: "Task with special chars: @#$%^&*()".to_string(), date: None, done: false, priority: false },
            Task { id: 12, text: "Complete task 42 and review items 1-10".to_string(), date: Some(next_week), done: false, priority: false },
            Task { id: 13, text: "Buy groceries: milk, bread, eggs, and cheese".to_string(), date: Some(tomorrow), done: false, priority: false },
            Task { id: 14, text: "Long-term project milestone".to_string(), date: Some(today + chrono::Duration::days(30)), done: false, priority: false },
        ]
    }

    pub fn new() -> Result<Self> {
        let backend = Backend::resolve()?;
        let mut tasks = backend.load()?;
        Self::maybe_log_db_location(&backend.describe());

        if cfg!(debug_assertions) && !Self::is_test_mode() && tasks.is_empty() {
            tasks = Self::create_sample_tasks();
            let tm = Self { tasks, backend };
            tm.save()?;
            return Ok(tm);
        }

        Ok(Self { tasks, backend })
    }

    /// Fresh manager without the debug sample-task seeding: used where the
    /// database is reloaded per operation (web server requests).
    pub fn open() -> Result<Self> {
        let backend = Backend::resolve()?;
        let tasks = backend.load()?;
        Ok(Self { tasks, backend })
    }

    pub fn new_for_restore() -> Result<Self> {
        let backend = Backend::resolve()?;
        Self::maybe_log_db_location(&backend.describe());
        Ok(Self {
            tasks: Vec::new(),
            backend,
        })
    }

    pub fn new_empty() -> Result<Self> {
        let db_path = std::env::temp_dir()
            .join("rusk_test")
            .join(std::process::id().to_string())
            .join("tasks.json");
        Self::maybe_log_db_location(&db_path.display().to_string());
        Ok(Self::new_empty_with_path(db_path))
    }

    /// Empty manager over a local database at `path` (tests and tools).
    /// Panics on a database location this build cannot handle — the callers
    /// pass known-good extensions.
    pub fn new_empty_with_path(path: PathBuf) -> Self {
        let backend =
            Backend::from_local_path(path).expect("unsupported database path in this build");
        Self {
            tasks: Vec::new(),
            backend,
        }
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn tasks_mut(&mut self) -> &mut Vec<Task> {
        &mut self.tasks
    }

    pub fn backend(&self) -> &Backend {
        &self.backend
    }

    /// The database file path for local backends; `None` for remote ones.
    pub fn local_path(&self) -> Option<&Path> {
        self.backend.local_path()
    }

    /// Where the database lives, for messages and logs.
    pub fn location(&self) -> String {
        self.backend.describe()
    }

    pub fn add_task(&mut self, text: Vec<String>, date: Option<String>) -> Result<()> {
        let text = text.join(" ");
        let date = match date {
            None => None,
            Some(d) => Some(parse_cli_date_for_edit(&d, None)?),
        };
        self.add_task_with_parsed_date(text, date)
    }

    /// Like [`add_task`](Self::add_task) but with an already-parsed due date (avoids re-parsing after the editor).
    pub fn add_task_with_parsed_date(
        &mut self,
        text: String,
        date: Option<chrono::NaiveDate>,
    ) -> Result<()> {
        if text.trim().is_empty() {
            anyhow::bail!("Task text cannot be empty");
        }
        let id = self.generate_next_id()?;
        let task = Task {
            id,
            text: text.clone(),
            date,
            done: false,
            priority: false,
        };
        self.tasks.push(task);
        self.save()?;
        Ok(())
    }

    pub fn delete_tasks(&mut self, ids: Vec<TaskId>) -> Result<Vec<TaskId>> {
        let mut deleted_count = 0;
        let mut not_found = Vec::new();

        let mut sorted_ids = ids;
        sorted_ids.sort_unstable_by(|a, b| b.cmp(a));

        for id in sorted_ids {
            if let Some(idx) = self.find_task_by_id(id) {
                self.tasks.remove(idx);
                deleted_count += 1;
            } else {
                not_found.push(id);
            }
        }

        if deleted_count > 0 {
            self.save()?;
        }

        Ok(not_found)
    }

    pub fn delete_all_done(&mut self) -> Result<usize> {
        let done_count = self.tasks.iter().filter(|t| t.done).count();
        if done_count == 0 {
            Ok(0)
        } else {
            self.tasks.retain(|t| !t.done);
            self.save()?;
            Ok(done_count)
        }
    }

    pub fn mark_tasks(&mut self, ids: Vec<TaskId>) -> Result<MarkResult> {
        self.toggle_tasks(ids, |task| {
            task.done = !task.done;
            task.done
        })
    }

    /// Toggles the `priority` flag for the given task ids. Returns `(Vec<(id, new_priority)>, not_found)`.
    /// Does not touch `done`: the priority is preserved across later done toggles.
    pub fn mark_priority_tasks(&mut self, ids: Vec<TaskId>) -> Result<MarkResult> {
        self.toggle_tasks(ids, |task| {
            task.priority = !task.priority;
            task.priority
        })
    }

    /// Shared toggle loop: `toggle` flips one flag on the task and returns its new state.
    /// Saves only when at least one task was found.
    fn toggle_tasks(
        &mut self,
        ids: Vec<TaskId>,
        toggle: impl Fn(&mut Task) -> bool,
    ) -> Result<MarkResult> {
        let mut not_found = Vec::new();
        let mut marked = Vec::new();
        let ids_len = ids.len();

        for id in ids {
            if let Some(idx) = self.find_task_by_id(id) {
                marked.push((id, toggle(&mut self.tasks[idx])));
            } else {
                not_found.push(id);
            }
        }

        if not_found.len() < ids_len {
            self.save()?;
        }

        Ok((marked, not_found))
    }

    pub fn edit_tasks(
        &mut self,
        ids: Vec<TaskId>,
        text: Option<Vec<String>>,
        date: Option<String>,
    ) -> Result<(Vec<TaskId>, Vec<TaskId>, Vec<TaskId>)> {
        let mut not_found = Vec::new();
        let mut edited = Vec::new();
        let mut unchanged = Vec::new();

        for id in ids {
            if let Some(idx) = self.find_task_by_id(id) {
                let task = &mut self.tasks[idx];
                let mut was_changed = false;

                if let Some(words) = &text {
                    let joined = words.join(" ");
                    if task.text != joined {
                        task.text = joined;
                        was_changed = true;
                    }
                }

                if let Some(ref new_date) = date {
                    if is_cli_date_clear_value(new_date) {
                        if task.date.is_some() {
                            task.date = None;
                            was_changed = true;
                        }
                    } else {
                        let parsed_date = parse_cli_date_for_edit(new_date, task.date)?;
                        if task.date != Some(parsed_date) {
                            task.date = Some(parsed_date);
                            was_changed = true;
                        }
                    }
                }

                if was_changed {
                    edited.push(id);
                } else {
                    unchanged.push(id);
                }
            } else {
                not_found.push(id);
            }
        }

        if !edited.is_empty() {
            self.save()?;
        }

        Ok((edited, unchanged, not_found))
    }

    pub fn find_task_by_id(&self, id: TaskId) -> Option<usize> {
        self.tasks.iter().position(|t| t.id == id)
    }

    pub fn generate_next_id(&self) -> Result<TaskId> {
        let mut used: Vec<TaskId> = self.tasks.iter().map(|t| t.id).collect();
        used.sort_unstable();

        let mut id: TaskId = 1;
        for &used_id in &used {
            if id == used_id {
                id = id
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("Maximum number of tasks reached"))?;
            } else {
                break;
            }
        }

        Ok(id)
    }

    pub fn save(&self) -> Result<()> {
        self.backend.save(&self.tasks)
    }

    /// The local database file path this build would use, ignoring remote
    /// locations (test/debug runs are pinned to a temp file anyway).
    pub fn resolve_db_path() -> PathBuf {
        Backend::resolve()
            .ok()
            .and_then(|b| b.local_path().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from(".rusk").join("tasks.json"))
    }

    /// Directory for auxiliary local state (editor drafts). Remote databases
    /// have no local directory, so those fall back to a temp subdirectory.
    pub fn get_db_dir() -> PathBuf {
        let backend = Backend::resolve().ok();
        match backend.as_ref().and_then(|b| b.local_path()) {
            Some(path) => path.parent().unwrap_or(path).to_path_buf(),
            None => std::env::temp_dir().join("rusk"),
        }
    }

    pub fn load_tasks_from_path(path: &Path) -> Result<Vec<Task>> {
        Backend::from_local_path(path.to_path_buf())?.load()
    }

    pub fn restore_from_backup(&mut self) -> Result<()> {
        self.tasks = self.backend.restore_from_backup()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::TaskManager;
    use chrono::NaiveDate;

    #[test]
    fn add_task_with_parsed_date_roundtrip() {
        let mut tm = TaskManager::new_empty().unwrap();
        let d = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        tm.add_task_with_parsed_date("Hello".to_string(), Some(d))
            .unwrap();
        assert_eq!(tm.tasks[0].text, "Hello");
        assert_eq!(tm.tasks[0].date, Some(d));
    }

    #[test]
    fn csv_db_roundtrip_via_task_manager() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.csv");
        let mut tm = TaskManager::new_empty_with_path(path.clone());
        let d = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
        tm.add_task_with_parsed_date("CSV task, with \"quotes\"\nsecond line".to_string(), Some(d))
            .unwrap();
        tm.add_task_with_parsed_date("plain".to_string(), None)
            .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("id,text,date,done,priority"));

        let loaded = TaskManager::load_tasks_from_path(&path).unwrap();
        assert_eq!(loaded, tm.tasks);
    }

    #[cfg(feature = "fmt-markdown")]
    #[test]
    fn markdown_db_roundtrip_via_task_manager() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.md");
        let mut tm = TaskManager::new_empty_with_path(path.clone());
        tm.add_task_with_parsed_date(
            "markdown task\nwith continuation".to_string(),
            NaiveDate::from_ymd_opt(2026, 7, 15),
        )
        .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("- [ ] markdown task @2026-07-15"), "{raw}");

        let loaded = TaskManager::load_tasks_from_path(&path).unwrap();
        assert_eq!(loaded, tm.tasks);
    }

    #[cfg(feature = "backend-sqlite")]
    #[test]
    fn sqlite_db_roundtrip_via_task_manager() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        let mut tm = TaskManager::new_empty_with_path(path.clone());
        tm.add_task_with_parsed_date("sqlite task".to_string(), None)
            .unwrap();

        let loaded = TaskManager::load_tasks_from_path(&path).unwrap();
        assert_eq!(loaded, tm.tasks);
    }
}
