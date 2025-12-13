use anyhow::{Context, Result};
use chrono::NaiveDate;
use colored::*;
use serde::{Deserialize, Serialize};
use std::fs;

use std::path::PathBuf;
use std::sync::OnceLock;

pub mod cli;
pub mod completions;
pub mod windows_console;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Task {
    pub id: u8,
    pub text: String,
    pub date: Option<NaiveDate>,
    pub done: bool,
}

/// Manages task operations and persistence
pub struct TaskManager {
    pub tasks: Vec<Task>,
    pub db_path: PathBuf,
}

struct DbReporter {
    path: PathBuf,
}

impl Drop for DbReporter {
    fn drop(&mut self) {
        // Printed at process end for test harness
        eprintln!("{}", format!("Database path: {}", self.path.display()).blue());
    }
}

impl TaskManager {
    /// Check if we're running in test mode
    fn is_test_mode() -> bool {
        let env_check = std::env::var("RUST_TEST_THREADS").is_ok()
            || std::env::var("CARGO_TEST").is_ok()
            || std::env::var("__CARGO_TEST_CHANNEL").is_ok();
        let exe_check = std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .map(|s| s.contains("test"))
            })
            .unwrap_or(false);
        env_check || exe_check || cfg!(test)
    }

    fn maybe_log_db_path(path: &std::path::Path) {
        static REPORTER: OnceLock<DbReporter> = OnceLock::new();
        if Self::is_test_mode() {
            let _ = REPORTER.get_or_init(|| DbReporter { path: path.to_path_buf() });
        } else if cfg!(debug_assertions) {
            eprintln!("{}", format!("Database path: {}", path.display()).blue());
        }
    }

    /// Create sample tasks for testing/demo purposes
    fn create_sample_tasks() -> Vec<Task> {
        let today = chrono::Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let tomorrow = today + chrono::Duration::days(1);
        let next_week = today + chrono::Duration::days(7);
        let last_week = today - chrono::Duration::days(7);
        
        // 14 tasks with different cases
        vec![
            // 1. Task without date, not done
            Task { id: 1, text: "Simple task without date".to_string(), date: None, done: false },
            // 2. Task without date, done
            Task { id: 2, text: "Completed task without date".to_string(), date: None, done: true },
            // 3. Task with date in the past, not done
            Task { id: 3, text: "Overdue task from last week".to_string(), date: Some(last_week), done: false },
            // 4. Task with date in the past, done
            Task { id: 4, text: "Completed overdue task".to_string(), date: Some(yesterday), done: true },
            // 5. Task with date today, not done
            Task { id: 5, text: "Task due today".to_string(), date: Some(today), done: false },
            // 6. Task with date today, done
            Task { id: 6, text: "Completed task due today".to_string(), date: Some(today), done: true },
            // 7. Task with date tomorrow, not done
            Task { id: 7, text: "Task due tomorrow".to_string(), date: Some(tomorrow), done: false },
            // 8. Task with date in the future, done
            Task { id: 8, text: "Completed future task".to_string(), date: Some(next_week), done: true },
            // 9. Task with short text
            Task { id: 9, text: "Short".to_string(), date: None, done: false },
            // 10. Task with long text
            Task { id: 10, text: "This is a very long task description that contains multiple words and demonstrates how the system handles longer text content".to_string(), date: Some(tomorrow), done: false },
            // 11. Task with special characters
            Task { id: 11, text: "Task with special chars: @#$%^&*()".to_string(), date: None, done: false },
            // 12. Task with numbers in text
            Task { id: 12, text: "Complete task 42 and review items 1-10".to_string(), date: Some(next_week), done: false },
            // 13. Task with multiple words
            Task { id: 13, text: "Buy groceries: milk, bread, eggs, and cheese".to_string(), date: Some(tomorrow), done: false },
            // 14. Task with date far in the future
            Task { id: 14, text: "Long-term project milestone".to_string(), date: Some(today + chrono::Duration::days(30)), done: false },
        ]
    }

    /// Create a new TaskManager instance
    pub fn new() -> Result<Self> {
        let db_path = Self::resolve_db_path();
        let mut tasks = Self::load_tasks_from_path(&db_path)?;
        Self::maybe_log_db_path(&db_path);
        
        // In debug mode, add 14 sample tasks with different cases when initializing empty DB
        if cfg!(debug_assertions) && !Self::is_test_mode() && tasks.is_empty() {
            let sample_tasks = Self::create_sample_tasks();
            tasks = sample_tasks;
            
            // Save the tasks to database
            let tm = Self { tasks, db_path: db_path.clone() };
            tm.save()?;
            return Ok(tm);
        }
        
        Ok(Self { tasks, db_path })
    }

    /// Create TaskManager for restore operations (doesn't load tasks initially)
    pub fn new_for_restore() -> Result<Self> {
        let db_path = Self::resolve_db_path();
        Self::maybe_log_db_path(&db_path);
        Ok(Self {
            tasks: Vec::new(),
            db_path,
        })
    }

    /// Create a new TaskManager instance with empty tasks (for testing)
    pub fn new_empty() -> Result<Self> {
        // Always use a temp DB for empty test managers to avoid touching real DB
        let db_path = std::env::temp_dir()
            .join("rusk_test")
            .join(format!("{}", std::process::id()))
            .join("tasks.json");
        Self::maybe_log_db_path(&db_path);
        Ok(Self {
            tasks: Vec::new(),
            db_path,
        })
    }

    /// Create a new TaskManager instance with custom path and empty tasks (for testing)
    pub fn new_empty_with_path(path: PathBuf) -> Self {
        Self {
            tasks: Vec::new(),
            db_path: path,
        }
    }

    /// Get a reference to all tasks
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// Get a mutable reference to all tasks
    pub fn tasks_mut(&mut self) -> &mut Vec<Task> {
        &mut self.tasks
    }

    /// Get the database file path
    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
    }

    /// Add a new task
    pub fn add_task(&mut self, text: Vec<String>, date: Option<String>) -> Result<()> {
        let text = text.join(" ");
        if text.trim().is_empty() {
            anyhow::bail!("Task text cannot be empty");
        }

        let date = date.and_then(|d| {
            let normalized = normalize_date_string(&d);
            NaiveDate::parse_from_str(&normalized, "%d-%m-%Y").ok()
        });
        let id = self.generate_next_id()?;

        let task = Task {
            id,
            text: text.clone(),
            date,
            done: false,
        };

        self.tasks.push(task);
        self.save()?;
        Ok(())
    }

    /// Delete tasks by IDs
    pub fn delete_tasks(&mut self, ids: Vec<u8>) -> Result<Vec<u8>> {
        let mut deleted_count = 0;
        let mut not_found = Vec::new();

        // Sort IDs in reverse order so deletion doesn't affect indexes
        let mut sorted_ids = ids;
        sorted_ids.sort_by(|a, b| b.cmp(a));

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

    /// Delete all completed tasks
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

    /// Mark tasks as done/undone by IDs
    #[allow(clippy::type_complexity)]
    pub fn mark_tasks(&mut self, ids: Vec<u8>) -> Result<(Vec<(u8, bool)>, Vec<u8>)> {
        let mut not_found = Vec::new();
        let mut marked = Vec::new();
        let ids_len = ids.len();

        for id in ids {
            if let Some(idx) = self.find_task_by_id(id) {
                let task = &mut self.tasks[idx];
                task.done = !task.done;
                marked.push((id, task.done));
            } else {
                not_found.push(id);
            }
        }

        if not_found.len() < ids_len {
            self.save()?;
        }

        Ok((marked, not_found))
    }

    /// Edit tasks by IDs
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
                    let normalized = normalize_date_string(new_date);
                    let parsed_date = NaiveDate::parse_from_str(&normalized, "%d-%m-%Y").ok();
                    if task.date != parsed_date {
                        task.date = parsed_date;
                        was_changed = true;
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

    /// Find task by ID and return its index
    pub fn find_task_by_id(&self, id: u8) -> Option<usize> {
        self.tasks.iter().position(|t| t.id == id)
    }

    /// Find tasks by IDs and return (found_indices, not_found_ids)
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

    /// Generate the next available task ID
    pub fn generate_next_id(&self) -> Result<u8> {
        let mut used: Vec<u8> = self.tasks.iter().map(|t| t.id).collect();
        used.sort_unstable();

        let mut id = 1u8;
        for &used_id in &used {
            if id == used_id {
                id += 1;
            } else {
                break;
            }
        }

        if id == 0 {
            anyhow::bail!("Maximum number of tasks (255) reached");
        }

        Ok(id)
    }

    /// Save tasks to the database
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists before any file operations
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create directory for the database file")?;
        }

        // Create backup of existing file before overwriting
        if self.db_path.exists() {
            let backup_path = self.db_path.with_extension("json.backup");
            if let Err(e) = fs::copy(&self.db_path, &backup_path) {
                // Don't fail the save operation if backup creation fails, just warn
                eprintln!(
                    "{}",
                    format!("Warning: Failed to create backup: {e}").yellow()
                );
            }
        }

        let data =
            serde_json::to_string_pretty(&self.tasks).context("Failed to serialize tasks")?;

        // Use atomic write: write to temporary file first, then rename
        let temp_path = self.db_path.with_extension("json.tmp");
        
        // Ensure parent directory exists for temp file too (should be same as db_path parent)
        if let Some(temp_parent) = temp_path.parent() {
            fs::create_dir_all(temp_parent)
                .context("Failed to create directory for temporary file")?;
        }
        
        fs::write(&temp_path, &data).context("Failed to write temporary database file")?;

        // Helper function to ensure directory exists
        let ensure_dir = || {
            if let Some(parent) = self.db_path.parent() {
                fs::create_dir_all(parent)
            } else {
                Ok(())
            }
        };

        // Try atomic rename
        match fs::rename(&temp_path, &self.db_path) {
            Ok(_) => {
                // Success - atomic write completed
            }
            Err(e) => {
                // Rename failed - ensure directory exists before trying copy
                // Directory might have been removed between operations (especially in tests)
                ensure_dir().ok();
                
                // Try copy+remove as fallback
                match fs::copy(&temp_path, &self.db_path) {
                    Ok(_) => {
                        // Copy succeeded, remove temp file
                        let _ = fs::remove_file(&temp_path);
                        if !Self::is_test_mode() {
                            eprintln!(
                                "{}",
                    format!(
                        "Warning: Atomic rename failed ({e}), used copy+remove instead"
                    )
                                .yellow()
                            );
                        }
                    }
                    Err(copy_err) => {
                        // Copy also failed, ensure directory exists before direct write
                        ensure_dir().ok();
                        // Use direct write as final fallback
                        // fs::write will create the file, but we need the directory to exist
                        let _ = fs::remove_file(&temp_path);
                        fs::write(&self.db_path, data).context("Failed to write database file")?;
                        if !Self::is_test_mode() {
                            eprintln!(
                                "{}",
                    format!(
                        "Warning: Atomic write failed ({e}), copy fallback also failed ({copy_err}), used direct write instead"
                    )
                                .yellow()
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the database file path from environment or use default
    /// RUSK_DB can point to either a directory or a specific file
    pub fn resolve_db_path() -> PathBuf {
        // During tests, always use temp DB like in debug mode: /tmp/rusk_debug/tasks.json
        // Ignore RUSK_DB in test mode
        if Self::is_test_mode() {
            std::env::temp_dir().join("rusk_debug").join("tasks.json")
        } else if cfg!(debug_assertions) {
            // Use temp DB in debug builds (cargo run), always ignore RUSK_DB in debug mode
            // Use a shared temp DB for all debug processes so IDs persist across runs
            std::env::temp_dir().join("rusk_debug").join("tasks.json")
        } else {
            // In release mode, honor RUSK_DB if set
            if let Ok(db_path) = std::env::var("RUSK_DB") {
                let path = PathBuf::from(db_path);
                if path.is_dir() || path.to_string_lossy().ends_with('/') {
                    path.join("tasks.json")
                } else {
                    path
                }
            } else {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".rusk")
                    .join("tasks.json")
            }
        }
    }

    /// Get the database directory path
    pub fn get_db_dir() -> PathBuf {
        let db_path = Self::resolve_db_path();
        db_path.parent().unwrap_or(&db_path).to_path_buf()
    }

    /// Load tasks from a specific path
    pub fn load_tasks_from_path(path: &PathBuf) -> Result<Vec<Task>> {
        if !path.exists() {
            Ok(Vec::new())
        } else {
            let data = fs::read_to_string(path).context("Failed to read the database file")?;

            match serde_json::from_str(&data) {
                Ok(tasks) => Ok(tasks),
                Err(e) => {
                    // Create a more helpful error message
                    let error_msg = format!(
                        "Failed to parse the database file at '{}'. The file appears to be corrupted.\n\
                        JSON parsing error: {}\n\
                        \n\
                        To fix this issue, you can:\n\
                        1. Delete the corrupted file: rm '{}'\n\
                        2. Or restore from backup if you have one\n\
                        3. The application will create a new empty database on next run",
                        path.display(),
                        e,
                        path.display()
                    );
                    anyhow::bail!("{}", error_msg)
                }
            }
        }
    }

    /// Restore database from backup file
    pub fn restore_from_backup(&mut self) -> Result<()> {
        let backup_path = self.db_path.with_extension("json.backup");

        if !backup_path.exists() {
            anyhow::bail!("No backup file found at '{}'", backup_path.display());
        }

        // Validate backup file before restoring
        let backup_tasks = Self::load_tasks_from_path(&backup_path)?;

        // Create backup of current database before restoring (only if it's valid)
        if self.db_path.exists() {
            let current_backup_path = self.db_path.with_extension("json.before_restore");
            // Try to validate current database first
            match Self::load_tasks_from_path(&self.db_path) {
                Ok(_) => {
                    // Current database is valid, create backup
                    if let Err(e) = fs::copy(&self.db_path, &current_backup_path) {
                        eprintln!(
                            "{}",
                            format!("Warning: Failed to backup current database: {e}").yellow()
                        );
                    } else {
                        println!(
                            "Current database backed up to: {}",
                            current_backup_path.display()
                        );
                    }
                }
                Err(_) => {
                    // Current database is corrupted, just mention it
                    println!("Current database is corrupted, skipping backup");
                }
            }
        }

        // Replace current database with backup
        fs::copy(&backup_path, &self.db_path).context("Failed to restore from backup")?;

        // Update current tasks with restored data
        self.tasks = backup_tasks;

        println!(
            "Successfully restored {} tasks from backup",
            self.tasks.len()
        );
        println!("Backup file: {}", backup_path.display());

        Ok(())
    }
}

/// Normalize date string: replace '/' with '-', and convert short year (25) to full year (2025)
/// Supports formats: DD-MM-YYYY, DD/MM/YYYY, DD-MM-YY, DD/MM/YY
pub fn normalize_date_string(date_str: &str) -> String {
    let mut normalized = date_str.replace('/', "-");
    
    // Check if year is short (1-2 digits without leading zeros) and convert to full year
    // Pattern: DD-MM-YY or DD/MM/YY -> DD-MM-2025
    // But NOT: DD-MM-0001 (4 digits, even if parsed as 1)
    let parts: Vec<&str> = normalized.split('-').collect();
    if parts.len() == 3 {
        if let Some(year_str) = parts.get(2) {
            let year_str = year_str.trim();
            // Only convert if year string is 1-2 digits (no leading zeros)
            if year_str.len() <= 2 && !year_str.is_empty() {
                if let Ok(year) = year_str.parse::<u16>() {
                    // If year is 1-2 digits (0-99), assume 2000s
                    if year < 100 {
                        let full_year = 2000 + year;
                        normalized = format!("{}-{}-{}", parts[0], parts[1], full_year);
                    }
                }
            }
        }
    }
    
    normalized
}

/// Parse ID input (comma-separated only)
/// Returns vector of valid IDs
/// Accepts comma-separated IDs in one or more arguments (e.g., "1,2,3" or "1,2" ",3")
/// Arguments starting with comma or containing comma are processed
pub fn parse_flexible_ids(args: &[String]) -> Vec<u8> {
    let mut ids = Vec::new();

    if args.is_empty() {
        return ids;
    }

    // Check if any argument contains comma
    let has_comma_args = args.iter().any(|a| a.trim().contains(',') || a.trim().starts_with(','));
    
    // Process all arguments that contain commas or start with comma (after trimming)
    // This handles cases like "1,5,4 ,6" which becomes ["1,5,4", " ,6"]
    for arg in args {
        let trimmed_arg = arg.trim();
        if trimmed_arg.contains(',') || trimmed_arg.starts_with(',') {
            // Handle comma-separated IDs like "1,2,3" or " ,6"
            for part in trimmed_arg.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    if let Ok(id) = trimmed.parse::<u8>() {
                        ids.push(id);
                    }
                    // Skip invalid parts silently
                }
            }
        } else if !has_comma_args && let Ok(id) = trimmed_arg.parse::<u8>() {
            // Single ID without comma (only if no comma-separated args exist)
            // This prevents treating space-separated IDs as multiple single IDs
            // Only process the first argument if it's a single ID
            if ids.is_empty() {
                ids.push(id);
            }
        }
        // Skip non-numeric arguments silently
    }

    ids
}

/// Parse edit command arguments to separate IDs and text
#[allow(clippy::type_complexity)]
pub fn parse_edit_args(args: Vec<String>) -> (Vec<u8>, Option<Vec<String>>) {
    let mut ids = Vec::new();
    let mut text_parts = Vec::new();
    let mut parsing_ids = true;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        // Skip date flags and their optional values
        if arg == "-d" || arg == "--date" {
            // If next token exists and is not another flag, treat it as the date value
            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                i += 2; // Skip flag and value
            } else {
                i += 1; // Skip only the flag (interactive date mode)
            }
            continue;
        }

        if parsing_ids {
            let trimmed_arg = arg.trim();
            // Check if argument contains comma or starts with comma (after trimming)
            // This handles cases like "1,5,4 ,6" which becomes ["1,5,4", " ,6"]
            if trimmed_arg.contains(',') || trimmed_arg.starts_with(',') {
                // Handle comma-separated IDs like "1,2,3" or " ,6"
                let mut found_any_valid_id = false;
                for part in trimmed_arg.split(',') {
                    let trimmed = part.trim();
                    if !trimmed.is_empty() {
                        if let Ok(id) = trimmed.parse::<u8>() {
                            ids.push(id);
                            found_any_valid_id = true;
                        }
                        // Skip invalid parts silently, but continue parsing
                    }
                }

                if !found_any_valid_id {
                    // No valid IDs found in comma-separated string, switch to text
                    parsing_ids = false;
                    text_parts.push(arg.clone());
                }
            } else if let Ok(id) = trimmed_arg.parse::<u8>() {
                // Single ID (only one ID allowed without comma)
                // If we already have IDs, this is likely text, not another ID
                if ids.is_empty() {
                    ids.push(id);
                } else {
                    // Multiple IDs without comma - treat as text
                    parsing_ids = false;
                    text_parts.push(arg.clone());
                }
            } else {
                // Not a number, switch to text parsing
                parsing_ids = false;
                text_parts.push(arg.clone());
            }
        } else {
            // We're parsing text now
            text_parts.push(arg.clone());
        }

        i += 1;
    }

    let text_option = if text_parts.is_empty() {
        None
    } else {
        Some(text_parts)
    };
    (ids, text_option)
}
