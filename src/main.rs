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

fn read_user_input(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush().context("Failed to flush stdout")?;
    let mut input = String::new();
    io::stdin().read_line(&mut input).context("Input error")?;
    Ok(input.trim().to_string())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut tasks = load_tasks()?;
    match cli.command {
        Some(Command::Add { text, date }) => {
            let text = text.join(" ");
            if text.trim().is_empty() {
                anyhow::bail!("Task text cannot be empty");
            }
            let date = date.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
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
            tasks.push(Task {
                id,
                text: text.clone(),
                date,
                done: false,
            });
            save_tasks(&tasks)?;
            println!("{} {}: {}", "Added task:".green(), id, text.bold());
        }
        Some(Command::Del { ids, all }) => {
            if all && ids.is_empty() {
                let done_count = tasks.iter().filter(|t| t.done).count();
                if done_count == 0 {
                    println!("{}", "No done tasks to delete.".yellow());
                } else {
                    let input = read_user_input(&format!(
                        "Delete all done tasks ({} total)? [y/N]: ",
                        done_count
                    ))?;
                    if input.eq_ignore_ascii_case("y") {
                        tasks.retain(|t| !t.done);
                        save_tasks(&tasks)?;
                        println!("Deleted {} done tasks.", done_count);
                    } else {
                        println!("Canceled.");
                    }
                }
            } else if !ids.is_empty() {
                let mut deleted_count = 0;
                let mut not_found: Vec<u8> = Vec::new();

                // Sort the IDs in reverse order so that deletion doesn't affect the indexes
                let mut sorted_ids: Vec<u8> = ids.clone();
                sorted_ids.sort_by(|a, b| b.cmp(a));

                for &id in &sorted_ids {
                    let pos = tasks.iter().position(|t| t.id == id);
                    match pos {
                        Some(idx) => {
                            let task = &tasks[idx];
                            let input =
                                read_user_input(&format!("Delete '{}'? [y/N]: ", task.text))?;
                            if input.eq_ignore_ascii_case("y") {
                                tasks.remove(idx);
                                deleted_count += 1;
                            } else {
                                println!("Canceled deletion of task {}.", id);
                            }
                        }
                        None => not_found.push(id),
                    }
                }

                if deleted_count > 0 {
                    save_tasks(&tasks)?;
                    println!("Deleted {} task(s).", deleted_count);
                }

                if !not_found.is_empty() {
                    println!("{} {:?}", "Tasks not found:".yellow(), not_found);
                }
            } else {
                println!("{}", "Please specify id(s) or --all.".yellow());
            }
        }
        Some(Command::Mark { ids }) => {
            let mut found_count = 0;
            let mut not_found: Vec<u8> = Vec::new();

            for &id in &ids {
                let mut found = false;
                for t in &mut tasks {
                    if t.id == id {
                        t.done = !t.done;
                        let status = if t.done {
                            "Marked as done".cyan()
                        } else {
                            "Marked as undone".yellow()
                        };
                        println!("{} {}: {}", status, id, t.text.bold());
                        found = true;
                        found_count += 1;
                        break;
                    }
                }
                if !found {
                    not_found.push(id);
                }
            }

            if found_count > 0 {
                save_tasks(&tasks)?;
            }

            if !not_found.is_empty() {
                println!("{} {:?}", "Tasks not found:".yellow(), not_found);
            }
        }
        Some(Command::Edit { ids, text, date }) => {
            let mut found_count = 0;
            let mut not_found: Vec<u8> = Vec::new();

            for &id in &ids {
                let mut found = false;
                for t in &mut tasks {
                    if t.id == id {
                        if let Some(words) = text.clone() {
                            let joined = words.join(" ");
                            t.text = joined;
                        }
                        if let Some(ref new_date) = date {
                            t.date = NaiveDate::parse_from_str(&new_date, "%Y-%m-%d").ok();
                        }
                        println!("{} {}: {}", "Edited task:".blue(), id, t.text.bold());
                        found = true;
                        found_count += 1;
                        break;
                    }
                }
                if !found {
                    not_found.push(id);
                }
            }

            if found_count > 0 {
                save_tasks(&tasks)?;
            }

            if !not_found.is_empty() {
                println!("{} {:?}", "Tasks not found:".yellow(), not_found);
            }
        }
        Some(Command::List) | None => {
            if tasks.is_empty() {
                println!("{}", "No tasks".yellow());
            } else {
                println!("\n  #  id    date       task");
                println!("  ──────────────────────────────────────────────");
                for t in &tasks {
                    let status = if t.done {
                        "✔".green()
                    } else {
                        "•".normal()
                    };
                    let date_str = t
                        .date
                        .map(|d| d.format("%Y-%m-%d").to_string())
                        .unwrap_or_default();
                    let date_colored = if let Some(d) = t.date {
                        if d < chrono::Local::now().date_naive() && !t.done {
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
                        t.id.to_string().bold(),
                        date_colored,
                        t.text
                    );
                }
                println!("\n");
            }
        }
    }
    Ok(())
}
