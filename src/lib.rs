use anyhow::{Context, Result};
use chrono::NaiveDate;
use colored::*;
use serde::{Deserialize, Serialize};
use std::fs;

use std::path::PathBuf;
use std::env;

pub mod cli;

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

impl TaskManager {
    /// Create a new TaskManager instance
    pub fn new() -> Result<Self> {
        let db_path = Self::get_db_path();
        let tasks = Self::load_tasks_from_path(&db_path)?;
        Ok(Self { tasks, db_path })
    }

    /// Create TaskManager for restore operations (doesn't load tasks initially)
    pub fn new_for_restore() -> Result<Self> {
        let db_path = Self::get_db_path();
        Ok(Self { tasks: Vec::new(), db_path })
    }

    /// Create a new TaskManager instance with empty tasks (for testing)
    pub fn new_empty() -> Result<Self> {
        let db_path = Self::get_db_path();
        Ok(Self { tasks: Vec::new(), db_path })
    }

    /// Create a new TaskManager instance with custom path and empty tasks (for testing)
    pub fn new_empty_with_path(path: PathBuf) -> Self {
        Self { tasks: Vec::new(), db_path: path }
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
        
        let date = date.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
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
            return Ok(0);
        }
        
        self.tasks.retain(|t| !t.done);
        self.save()?;
        Ok(done_count)
    }

    /// Mark tasks as done/undone by IDs
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
    pub fn edit_tasks(&mut self, ids: Vec<u8>, text: Option<Vec<String>>, date: Option<String>) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
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
                    let parsed_date = NaiveDate::parse_from_str(new_date, "%Y-%m-%d").ok();
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
        if let Some(parent) = self.db_path.parent() {
            fs::create_dir_all(parent).context("Failed to create directory for the database file")?;
        }
        
        // Create backup of existing file before overwriting
        if self.db_path.exists() {
            let backup_path = self.db_path.with_extension("json.backup");
            if let Err(e) = fs::copy(&self.db_path, &backup_path) {
                // Don't fail the save operation if backup creation fails, just warn
                eprintln!("{}", format!("Warning: Failed to create backup: {}", e).yellow());
            }
        }
        
        let data = serde_json::to_string_pretty(&self.tasks).context("Failed to serialize tasks")?;
        
        // Use atomic write: write to temporary file first, then rename
        let temp_path = self.db_path.with_extension("json.tmp");
        fs::write(&temp_path, &data).context("Failed to write temporary database file")?;
        
        // Try atomic rename, fallback to direct write if it fails
        if let Err(e) = fs::rename(&temp_path, &self.db_path) {
            // Clean up temp file
            let _ = fs::remove_file(&temp_path);
            // Fallback to direct write
            fs::write(&self.db_path, data).context("Failed to write database file")?;
            eprintln!("{}", format!("Warning: Atomic write failed ({}), used direct write instead", e).yellow());
        }
        
        Ok(())
    }

    /// Get the database file path from environment or use default
    /// RUSK_DB can point to either a directory or a specific file
    pub fn get_db_path() -> PathBuf {
        if let Ok(db_path) = env::var("RUSK_DB") {
            let path = PathBuf::from(db_path);
            if path.is_dir() || path.to_string_lossy().ends_with('/') {
                // If it's a directory, add the default filename
                path.join("tasks.json")
            } else {
                // If it's a file path, use as-is
                path
            }
        } else {
            // Default to home directory with .rusk subdirectory
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".rusk")
                .join("tasks.json")
        }
    }

    /// Get the database directory path
    pub fn get_db_dir() -> PathBuf {
        let db_path = Self::get_db_path();
        db_path.parent().unwrap_or(&db_path).to_path_buf()
    }

    /// Load tasks from a specific path
    pub fn load_tasks_from_path(path: &PathBuf) -> Result<Vec<Task>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
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
                        eprintln!("{}", format!("Warning: Failed to backup current database: {}", e).yellow());
                    } else {
                        println!("Current database backed up to: {}", current_backup_path.display());
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
        
        println!("Successfully restored {} tasks from backup", self.tasks.len());
        println!("Backup file: {}", backup_path.display());
        
        Ok(())
    }
}


