use anyhow::{Result, Context};
use colored::*;
use std::io::{self, Write};
use crate::{Task, TaskManager};

/// Interactive command handlers for CLI operations
pub struct HandlerCLI;

impl HandlerCLI {
    /// Read user input from stdin with a prompt
    fn read_user_input(prompt: &str) -> Result<String> {
        print!("{}", prompt);
        io::stdout().flush().context("Failed to flush stdout")?;
        let mut input = String::new();
        io::stdin().read_line(&mut input).context("Input error")?;
        Ok(input.trim().to_string())
    }
    /// Handle adding a new task with user interaction
    pub fn handle_add_task(tm: &mut TaskManager, text: Vec<String>, date: Option<String>) -> Result<()> {
        tm.add_task(text, date)?;
        let task = tm.tasks().last().unwrap();
        println!("{} {}: {}", "Added task:".green(), task.id, task.text.bold());
        Ok(())
    }

    /// Handle deleting tasks with user interaction
    pub fn handle_delete_tasks(tm: &mut TaskManager, ids: Vec<u8>, all: bool) -> Result<()> {
        if all && ids.is_empty() {
            Self::delete_all_done(tm)
        } else if !ids.is_empty() {
            Self::delete_by_ids(tm, ids)
        } else {
            println!("{}", "Please specify id(s) or --all.".yellow());
            Ok(())
        }
    }

    /// Delete all completed tasks with confirmation
    fn delete_all_done(tm: &mut TaskManager) -> Result<()> {
        let done_count = tm.tasks().iter().filter(|t| t.done).count();
        if done_count == 0 {
            println!("{}", "No done tasks to delete.".yellow());
            return Ok(());
        }
        
        let input = Self::read_user_input(
            &format!(
                "{}{}{}",
                "Delete all done tasks (".truecolor(255, 165, 0),
                done_count.to_string().white(),
                ")? [y/N]: ".truecolor(255, 165, 0)
            )
        )?;
        
        if input.eq_ignore_ascii_case("y") {
            let deleted = tm.delete_all_done()?;
            if deleted > 0 {
                println!(
                    "{}{}{}",
                    "Deleted ".red(),
                    deleted.to_string(),
                    " done tasks.".red()
                );
            }
            Ok(())
        } else {
            println!("Canceled.");
            Ok(())
        }
    }

    /// Delete specific tasks by IDs with confirmation
    fn delete_by_ids(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        let mut confirmed_ids = Vec::new();
        let mut not_found: Vec<u8> = Vec::new();
        
        // Get user confirmation for each task
        for &id in &ids {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let input = Self::read_user_input(
                    &format!(
                        "{}{}{}",
                        "Delete '".truecolor(255, 165, 0),
                        task.text.white(),
                        "'? [y/N]: ".truecolor(255, 165, 0)
                    )
                )?;
                if input.eq_ignore_ascii_case("y") {
                    confirmed_ids.push(id);
                } else {
                    println!("Canceled deletion of task {}.", id);
                }
            } else {
                not_found.push(id);
            }
        }
        
        // Delete confirmed tasks using TaskManager
        if !confirmed_ids.is_empty() {
            let deleted_count = confirmed_ids.len();
            let _ = tm.delete_tasks(confirmed_ids)?; // TaskManager handles saving
            println!(
                "{}{}{}",
                "Deleted ".red(),
                deleted_count.to_string(),
                " task(s).".red()
            );
        }
        
        if !not_found.is_empty() {
            let list = not_found.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(" ");
            println!("{} {}", "Tasks not found IDs:".yellow(), list);
        }
        
        Ok(())
    }

    /// Handle marking tasks as done/undone with user interaction
    pub fn handle_mark_tasks(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        let (marked, not_found) = tm.mark_tasks(ids)?;
        
        // Show success messages for marked tasks
        for (id, done) in marked {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let status = if done { "done" } else { "undone" };
                println!("{} {}: {}", format!("Marked task as {}:", status).green(), id, task.text.bold());
            }
        }
        
        if !not_found.is_empty() {
            let list = not_found.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(" ");
            println!("{} {}", "Tasks not found IDs:".yellow(), list);
        }
        
        Ok(())
    }

    /// Handle editing tasks with user interaction
    pub fn handle_edit_tasks(tm: &mut TaskManager, ids: Vec<u8>, text: Option<Vec<String>>, date: Option<String>) -> Result<()> {
        let (edited, unchanged, not_found) = tm.edit_tasks(ids, text, date)?;
        
        // Show success messages for edited tasks
        for id in edited {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                println!("{} {}: {}", "Edited task:".green(), id, task.text.bold());
            }
        }
        
        // Show messages for unchanged tasks (magenta color)
        for id in unchanged {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                println!("{} {}: {}", "Task already has this content:".magenta(), id, task.text.bold());
            }
        }
        
        if !not_found.is_empty() {
            let list = not_found.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(" ");
            println!("{} {}", "Tasks not found IDs:".yellow(), list);
        }
        
        Ok(())
    }
    
    /// List all tasks with their status, id, date, and text
    pub fn handle_list_tasks(tasks: &[Task]) {
        if tasks.is_empty() {
            println!("{}", "No tasks".yellow());
            return;
        }
        
        println!("\n  #  id    date       task");
        println!("  ──────────────────────────────────────────────");
        
        for task in tasks {
            let status = if task.done {
                "✔".green()
            } else {
                "•".normal()
            };
            
            let date_str = task
                .date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            
            let date_colored = if let Some(d) = task.date {
                if d < chrono::Local::now().date_naive() && !task.done {
                    date_str.red()
                } else {
                    date_str.cyan()
                }
            } else {
                "".normal()
            };
            
            println!(
                "  {} {:>3} {:^10} {}",
                status,
                task.id.to_string().bold(),
                date_colored,
                task.text
            );
        }
        
        println!("\n");
    }

    /// Handle restoring database from backup
    pub fn handle_restore(tm: &mut TaskManager) -> Result<()> {
        tm.restore_from_backup()
    }
}
