use anyhow::{Context, Result};
use colored::*;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::model::Task;
use crate::parse_cli_date_for_edit;
use crate::parser::date::is_cli_date_clear_value;

pub type MarkResult = (Vec<(u8, bool)>, Vec<u8>);

/// Manages task operations and persistence
pub struct TaskManager {
    pub tasks: Vec<Task>,
    pub db_path: PathBuf,
}

/// Byte offset (0-based) of the first character of each 1-based line in `s`.
fn line_starts(s: &str) -> Vec<usize> {
    let mut v = vec![0];
    for (i, c) in s.char_indices() {
        if c == '\n' {
            v.push(i + 1);
        }
    }
    v
}

fn line_byte_end(data: &str, starts: &[usize], one_based_line: usize) -> Option<usize> {
    if one_based_line < 1 || one_based_line > starts.len() {
        return None;
    }
    let s = if one_based_line < starts.len() {
        starts[one_based_line]
    } else {
        data.len()
    };
    Some(s)
}

fn error_byte_in_file(data: &str, line: usize, column: usize) -> Option<usize> {
    if line < 1 {
        return None;
    }
    let starts = line_starts(data);
    if line > starts.len() {
        return None;
    }
    let line0 = line - 1;
    let line_start = starts[line0];
    let after_line = if line0 + 1 < starts.len() {
        starts[line0 + 1]
    } else {
        data.len()
    };
    let line_len = after_line - line_start;
    let col0 = column.saturating_sub(1);
    if col0 > line_len {
        return None;
    }
    Some(line_start + col0)
}

fn json_error_line_context(data: &str, e: &serde_json::Error) -> Option<String> {
    let n = e.line();
    if n == 0 {
        return None;
    }
    let lines: Vec<_> = data.lines().collect();
    let i = n - 1;
    if i >= lines.len() {
        return None;
    }
    let starts = line_starts(data);
    let first = i.saturating_sub(1) + 1;
    let last = (i + 2).min(lines.len());
    let range_str = if first <= last {
        match (
            starts.get(first - 1).copied(),
            line_byte_end(data, &starts, last),
        ) {
            (Some(a), Some(b)) if a <= b => format!("context file bytes {a}..{b}"),
            _ => String::new(),
        }
    } else {
        String::new()
    };

    let err_str = error_byte_in_file(data, n, e.column())
        .map(|b| format!("(error at byte {b})"))
        .unwrap_or_default();

    let ctx = (i.saturating_sub(1)..(i + 2).min(lines.len()))
        .map(|j| format!("{}: {}", j + 1, lines[j].trim_end()))
        .collect::<Vec<_>>()
        .join(" | ");

    let parts = [err_str.as_str(), range_str.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if parts.is_empty() {
        Some(ctx)
    } else {
        Some(format!("{parts}: {ctx}"))
    }
}

/// Non-fatal warning on stderr (backup / atomic-write fallbacks), in the
/// theme warning color (yellow by default).
fn warn_yellow(msg: &str) {
    eprintln!("{}", crate::config::theme().warning.paint(msg));
}

fn eprint_db_path(path: &std::path::Path) {
    eprintln!("{}", format!("Database path: {}", path.display()).blue());
}

struct DbReporter {
    path: PathBuf,
}

impl Drop for DbReporter {
    fn drop(&mut self) {
        eprint_db_path(&self.path);
    }
}

impl TaskManager {
    fn is_test_mode() -> bool {
        crate::is_test_mode()
    }

    fn maybe_log_db_path(path: &std::path::Path) {
        static REPORTER: OnceLock<DbReporter> = OnceLock::new();
        if Self::is_test_mode() {
            let _ = REPORTER.get_or_init(|| DbReporter {
                path: path.to_path_buf(),
            });
        } else if cfg!(debug_assertions) {
            eprint_db_path(path);
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
        let db_path = Self::resolve_db_path();
        let mut tasks = Self::load_tasks_from_path(&db_path)?;
        Self::maybe_log_db_path(&db_path);

        if cfg!(debug_assertions) && !Self::is_test_mode() && tasks.is_empty() {
            tasks = Self::create_sample_tasks();
            let tm = Self {
                tasks,
                db_path: db_path.clone(),
            };
            tm.save()?;
            return Ok(tm);
        }

        Ok(Self { tasks, db_path })
    }

    pub fn new_for_restore() -> Result<Self> {
        let db_path = Self::resolve_db_path();
        Self::maybe_log_db_path(&db_path);
        Ok(Self {
            tasks: Vec::new(),
            db_path,
        })
    }

    pub fn new_empty() -> Result<Self> {
        let db_path = std::env::temp_dir()
            .join("rusk_test")
            .join(std::process::id().to_string())
            .join("tasks.json");
        Self::maybe_log_db_path(&db_path);
        Ok(Self {
            tasks: Vec::new(),
            db_path,
        })
    }

    pub fn new_empty_with_path(path: PathBuf) -> Self {
        Self {
            tasks: Vec::new(),
            db_path: path,
        }
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn tasks_mut(&mut self) -> &mut Vec<Task> {
        &mut self.tasks
    }

    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
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

    pub fn delete_tasks(&mut self, ids: Vec<u8>) -> Result<Vec<u8>> {
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

    pub fn mark_tasks(&mut self, ids: Vec<u8>) -> Result<MarkResult> {
        self.toggle_tasks(ids, |task| {
            task.done = !task.done;
            task.done
        })
    }

    /// Toggles the `priority` flag for the given task ids. Returns `(Vec<(id, new_priority)>, not_found)`.
    /// Does not touch `done`: the priority is preserved across later done toggles.
    pub fn mark_priority_tasks(&mut self, ids: Vec<u8>) -> Result<MarkResult> {
        self.toggle_tasks(ids, |task| {
            task.priority = !task.priority;
            task.priority
        })
    }

    /// Shared toggle loop: `toggle` flips one flag on the task and returns its new state.
    /// Saves only when at least one task was found.
    fn toggle_tasks(
        &mut self,
        ids: Vec<u8>,
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
        ids: Vec<u8>,
        text: Option<Vec<String>>,
        date: Option<String>,
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
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

    pub fn find_task_by_id(&self, id: u8) -> Option<usize> {
        self.tasks.iter().position(|t| t.id == id)
    }

    pub fn find_tasks_by_ids(&self, ids: &[u8]) -> (Vec<usize>, Vec<u8>) {
        let mut found_indices = Vec::new();
        let mut not_found = Vec::new();

        for &id in ids {
            if let Some(idx) = self.find_task_by_id(id) {
                found_indices.push(idx);
            } else {
                not_found.push(id);
            }
        }

        (found_indices, not_found)
    }

    pub fn generate_next_id(&self) -> Result<u8> {
        let mut used: Vec<u8> = self.tasks.iter().map(|t| t.id).collect();
        used.sort_unstable();

        let mut id = 1u8;
        for &used_id in &used {
            if id == used_id {
                // Wrap instead of overflow: 255 used ids wrap to 0, caught below
                // (plain `+= 1` panics in debug builds when all 255 ids are taken).
                id = id.wrapping_add(1);
            } else {
                break;
            }
        }

        if id == 0 {
            anyhow::bail!("Maximum number of tasks (255) reached");
        }

        Ok(id)
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create directory for the database file")?;
        }

        if crate::config::config().backup && self.db_path.exists() {
            let backup_path = Self::aux_path(&self.db_path, "backup");
            if let Err(e) = fs::copy(&self.db_path, &backup_path) {
                warn_yellow(&format!("Warning: Failed to create backup: {e}"));
            }
        }

        let data = match crate::codec::DbFormat::from_path(&self.db_path) {
            crate::codec::DbFormat::Json => {
                serde_json::to_string_pretty(&self.tasks).context("Failed to serialize tasks")?
            }
            crate::codec::DbFormat::Csv => crate::codec::to_csv(&self.tasks),
        };

        let temp_path = Self::aux_path(&self.db_path, "tmp");

        if let Some(temp_parent) = temp_path.parent() {
            fs::create_dir_all(temp_parent)
                .context("Failed to create directory for temporary file")?;
        }

        fs::write(&temp_path, &data).context("Failed to write temporary database file")?;

        let ensure_dir = || {
            if let Some(parent) = self.db_path.parent() {
                fs::create_dir_all(parent)
            } else {
                Ok(())
            }
        };

        match fs::rename(&temp_path, &self.db_path) {
            Ok(_) => {}
            Err(e) => {
                ensure_dir().ok();

                match fs::copy(&temp_path, &self.db_path) {
                    Ok(_) => {
                        let _ = fs::remove_file(&temp_path);
                        if !Self::is_test_mode() {
                            warn_yellow(&format!(
                                "Warning: Atomic rename failed ({e}), used copy+remove instead"
                            ));
                        }
                    }
                    Err(copy_err) => {
                        ensure_dir().ok();
                        let _ = fs::remove_file(&temp_path);
                        fs::write(&self.db_path, data).context("Failed to write database file")?;
                        if !Self::is_test_mode() {
                            warn_yellow(&format!(
                                "Warning: Atomic write failed ({e}), copy fallback also failed ({copy_err}), used direct write instead"
                            ));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Auxiliary sibling of the database file: `tasks.json` + `backup` →
    /// `tasks.json.backup` (and `tasks.csv` → `tasks.csv.backup`), so the
    /// base format stays recognizable in the name.
    fn aux_path(path: &std::path::Path, suffix: &str) -> PathBuf {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("json");
        path.with_extension(format!("{ext}.{suffix}"))
    }

    /// Directory values (existing dir or trailing `/`) get `tasks.json` appended.
    fn db_path_from_value(path: PathBuf) -> PathBuf {
        if path.is_dir() || path.to_string_lossy().ends_with('/') {
            path.join("tasks.json")
        } else {
            path
        }
    }

    pub fn resolve_db_path() -> PathBuf {
        if Self::is_test_mode() || cfg!(debug_assertions) {
            std::env::temp_dir().join("rusk_debug").join("tasks.json")
        } else if let Ok(db_path) = std::env::var("RUSK_DB") {
            Self::db_path_from_value(PathBuf::from(db_path))
        } else if let Some(db_path) = &crate::config::config().rusk_db {
            // `rusk_db` from the config file; the RUSK_DB env var wins above.
            Self::db_path_from_value(db_path.clone())
        } else {
            PathBuf::from(".rusk").join("tasks.json")
        }
    }

    pub fn get_db_dir() -> PathBuf {
        let db_path = Self::resolve_db_path();
        db_path.parent().unwrap_or(&db_path).to_path_buf()
    }

    pub fn load_tasks_from_path(path: &PathBuf) -> Result<Vec<Task>> {
        if !path.exists() {
            Ok(Vec::new())
        } else {
            let data = fs::read_to_string(path).context("Failed to read the database file")?;

            if crate::codec::DbFormat::from_path(path) == crate::codec::DbFormat::Csv {
                return crate::codec::from_csv(&data).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to parse the CSV database file at '{}': {e}",
                        path.display()
                    )
                });
            }

            match serde_json::from_str(&data) {
                Ok(tasks) => Ok(tasks),
                Err(e) => {
                    let context_line = json_error_line_context(&data, &e)
                        .map(|c| format!(" Context: {c}"))
                        .unwrap_or_default();
                    let error_msg = format!(
                        "Failed to parse the database file at '{}'. The file appears to be corrupted.\n\
                        JSON parsing error: {}{}\n\
                        \n\
                        To fix this issue, you can:\n\
                        1. Delete the corrupted file: rm '{}'\n\
                        2. Or restore from backup if you have one\n\
                        3. The application will create a new empty database on next run",
                        path.display(),
                        e,
                        context_line,
                        path.display()
                    );
                    anyhow::bail!("{}", error_msg)
                }
            }
        }
    }

    pub fn restore_from_backup(&mut self) -> Result<()> {
        let backup_path = Self::aux_path(&self.db_path, "backup");

        if !backup_path.exists() {
            anyhow::bail!("No backup file found at '{}'", backup_path.display());
        }

        let backup_tasks = Self::load_tasks_from_path(&backup_path)?;

        if self.db_path.exists() {
            let current_backup_path = Self::aux_path(&self.db_path, "before_restore");
            match Self::load_tasks_from_path(&self.db_path) {
                Ok(_) => {
                    if let Err(e) = fs::copy(&self.db_path, &current_backup_path) {
                        warn_yellow(&format!("Warning: Failed to backup current database: {e}"));
                    } else {
                        println!(
                            "Current database backed up to: {}",
                            current_backup_path.display()
                        );
                    }
                }
                Err(_) => {
                    println!("Current database is corrupted, skipping backup");
                }
            }
        }

        fs::copy(&backup_path, &self.db_path).context("Failed to restore from backup")?;

        self.tasks = backup_tasks;

        println!(
            "Successfully restored {} tasks from backup",
            self.tasks.len()
        );
        println!("Backup file: {}", backup_path.display());

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
}
