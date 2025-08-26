use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand};
use colored::*;
use dirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Task {
    id: u8,
    text: String,
    date: Option<NaiveDate>,
    done: bool,
}

#[derive(Parser)]
#[command(about, version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(
        alias = "a",
        about = "Add a new task. Example: rusk add buy groceries. With a specific date: rusk add buy groceries --date 2025-07-01"
    )]
    Add {
        text: Vec<String>,
        #[arg(short, long)]
        date: Option<String>,
    },
    #[command(
        alias = "d",
        about = "Delete tasks by id(s). Use --all to delete all completed tasks. Example: rusk del 3, rusk del 1 2 3"
    )]
    Del {
        #[arg(value_delimiter = ',')]
        ids: Vec<u8>,
        #[arg(long)]
        all: bool,
    },
    #[command(
        alias = "m",
        about = "Mark tasks as done/undone by id(s). Example: rusk mark 3, rusk mark 1 2 3"
    )]
    Mark {
        #[arg(value_delimiter = ',')]
        ids: Vec<u8>,
    },
    #[command(
        alias = "e",
        about = "Edit tasks by id(s). Use --text to change the text, --date to change the date. Example: rusk edit 3 --text new text, rusk edit 1 2 3 --text new text"
    )]
    Edit {
        #[arg(value_delimiter = ',')]
        ids: Vec<u8>,
        #[arg(short, long, num_args = 1..)]
        text: Option<Vec<String>>,
        #[arg(short, long)]
        date: Option<String>,
    },
    #[command(
        alias = "l",
        about = "List all tasks with their status, id, date, and text"
    )]
    List,
}

// Database operations
fn db_file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rusk.json")
}

fn load_tasks() -> Result<Vec<Task>> {
    let path = db_file();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path).context("Failed to read the database file")?;
    let tasks = serde_json::from_str(&data).context("Failed to parse the database file")?;
    Ok(tasks)
}

fn save_tasks(tasks: &[Task]) -> Result<()> {
    let path = db_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create directory for the database file")?;
    }
    let data = serde_json::to_string_pretty(tasks).context("Failed to serialize tasks")?;
    fs::write(&path, data).context("Failed to write the database file")?;
    Ok(())
}

// Utility functions
fn read_user_input(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush().context("Failed to flush stdout")?;
    let mut input = String::new();
    io::stdin().read_line(&mut input).context("Input error")?;
    Ok(input.trim().to_string())
}

fn generate_next_id(tasks: &[Task]) -> Result<u8> {
    let mut used: Vec<u8> = tasks.iter().map(|t| t.id).collect();
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

fn find_task_by_id(tasks: &[Task], id: u8) -> Option<usize> {
    tasks.iter().position(|t| t.id == id)
}

fn find_tasks_by_ids(tasks: &[Task], ids: &[u8]) -> (Vec<usize>, Vec<u8>) {
    let mut found_indices = Vec::new();
    let mut not_found = Vec::new();
    
    for &id in ids {
        if let Some(idx) = find_task_by_id(tasks, id) {
            found_indices.push(idx);
        } else {
            not_found.push(id);
        }
    }
    
    (found_indices, not_found)
}

// Command handlers
fn handle_add(tasks: &mut Vec<Task>, text: Vec<String>, date: Option<String>) -> Result<()> {
    let text = text.join(" ");
    if text.trim().is_empty() {
        anyhow::bail!("Task text cannot be empty");
    }
    
    let date = date.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
    let id = generate_next_id(tasks)?;
    
    tasks.push(Task {
        id,
        text: text.clone(),
        date,
        done: false,
    });
    
    save_tasks(tasks)?;
    println!("{} {}: {}", "Added task:".green(), id, text.bold());
    Ok(())
}

fn handle_delete(tasks: &mut Vec<Task>, ids: Vec<u8>, all: bool) -> Result<()> {
    if all && ids.is_empty() {
        handle_delete_all_done(tasks)
    } else if !ids.is_empty() {
        handle_delete_by_ids(tasks, ids)
    } else {
        println!("{}", "Please specify id(s) or --all.".yellow());
        Ok(())
    }
}

fn handle_delete_all_done(tasks: &mut Vec<Task>) -> Result<()> {
    let done_count = tasks.iter().filter(|t| t.done).count();
    if done_count == 0 {
        println!("{}", "No done tasks to delete.".yellow());
        return Ok(());
    }
    
    let input = read_user_input(&format!(
        "Delete all done tasks ({} total)? [y/N]: ",
        done_count
    ))?;
    
    if input.eq_ignore_ascii_case("y") {
        tasks.retain(|t| !t.done);
        save_tasks(tasks)?;
        println!("Deleted {} done tasks.", done_count);
    } else {
        println!("Canceled.");
    }
    
    Ok(())
}

fn handle_delete_by_ids(tasks: &mut Vec<Task>, ids: Vec<u8>) -> Result<()> {
    let mut deleted_count = 0;
    let mut not_found: Vec<u8> = Vec::new();
    
    // Sort IDs in reverse order so deletion doesn't affect indexes
    let mut sorted_ids: Vec<u8> = ids.clone();
    sorted_ids.sort_by(|a, b| b.cmp(a));
    
    for &id in &sorted_ids {
        if let Some(idx) = find_task_by_id(tasks, id) {
            let task = &tasks[idx];
            let input = read_user_input(&format!("Delete '{}'? [y/N]: ", task.text))?;
            if input.eq_ignore_ascii_case("y") {
                tasks.remove(idx);
                deleted_count += 1;
            } else {
                println!("Canceled deletion of task {}.", id);
            }
        } else {
            not_found.push(id);
        }
    }
    
    if deleted_count > 0 {
        save_tasks(tasks)?;
        println!("Deleted {} task(s).", deleted_count);
    }
    
    if !not_found.is_empty() {
        println!("{} {:?}", "Tasks not found:".yellow(), not_found);
    }
    
    Ok(())
}

fn handle_mark(tasks: &mut Vec<Task>, ids: Vec<u8>) -> Result<()> {
    let (found_indices, not_found) = find_tasks_by_ids(tasks, &ids);
    
    for &idx in &found_indices {
        let task = &mut tasks[idx];
        task.done = !task.done;
        
        let status = if task.done {
            "Marked as done".cyan()
        } else {
            "Marked as undone".yellow()
        };
        
        println!("{} {}: {}", status, task.id, task.text.bold());
    }
    
    if !found_indices.is_empty() {
        save_tasks(tasks)?;
    }
    
    if !not_found.is_empty() {
        println!("{} {:?}", "Tasks not found:".yellow(), not_found);
    }
    
    Ok(())
}

fn handle_edit(tasks: &mut Vec<Task>, ids: Vec<u8>, text: Option<Vec<String>>, date: Option<String>) -> Result<()> {
    let (found_indices, not_found) = find_tasks_by_ids(tasks, &ids);
    
    for &idx in &found_indices {
        let task = &mut tasks[idx];
        
        if let Some(words) = &text {
            let joined = words.join(" ");
            task.text = joined;
        }
        
        if let Some(ref new_date) = date {
            task.date = NaiveDate::parse_from_str(new_date, "%Y-%m-%d").ok();
        }
        
        println!("{} {}: {}", "Edited task:".blue(), task.id, task.text.bold());
    }
    
    if !found_indices.is_empty() {
        save_tasks(tasks)?;
    }
    
    if !not_found.is_empty() {
        println!("{} {:?}", "Tasks not found:".yellow(), not_found);
    }
    
    Ok(())
}

fn handle_list(tasks: &[Task]) {
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut tasks = load_tasks()?;
    
    match cli.command {
        Some(Command::Add { text, date }) => {
            handle_add(&mut tasks, text, date)?;
        }
        Some(Command::Del { ids, all }) => {
            handle_delete(&mut tasks, ids, all)?;
        }
        Some(Command::Mark { ids }) => {
            handle_mark(&mut tasks, ids)?;
        }
        Some(Command::Edit { ids, text, date }) => {
            handle_edit(&mut tasks, ids, text, date)?;
        }
        Some(Command::List) | None => {
            handle_list(&tasks);
        }
    }
    
    Ok(())
}
