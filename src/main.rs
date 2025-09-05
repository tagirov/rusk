use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use rusk::{TaskManager, cli::HandlerCLI, windows_console};

/// Parse flexible ID input (space-separated, comma-separated, or mixed)
/// Returns vector of valid IDs
fn parse_flexible_ids(args: &[String]) -> Vec<u8> {
    let mut ids = Vec::new();
    
    for arg in args {
        // Try to parse as single ID first
        if let Ok(id) = arg.parse::<u8>() {
            ids.push(id);
        } else if arg.contains(',') {
            // Handle comma-separated IDs like "1,2,3"
            for part in arg.split(',') {
                let trimmed = part.trim();
                if let Ok(id) = trimmed.parse::<u8>() {
                    ids.push(id);
                }
                // Skip invalid parts silently
            }
        }
        // Skip non-numeric arguments silently
    }
    
    ids
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
        about = "Delete tasks by id(s). Use --all to delete all completed tasks. Examples: rusk del 3, rusk del 1 2 3, rusk del 1,2,3"
    )]
    Del {
        #[arg(trailing_var_arg = true)]
        ids: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    #[command(
        alias = "m",
        about = "Mark tasks as done/undone by id(s). Examples: rusk mark 3, rusk mark 1 2 3, rusk mark 1,2,3"
    )]
    Mark {
        #[arg(trailing_var_arg = true)]
        ids: Vec<String>,
    },
    #[command(
        alias = "e",
        about = "Edit tasks by id(s). Text can be provided without quotes. Examples: rusk e 3 new task text -d 2025-11-1, rusk e 1 2 3 shared text"
    )]
    Edit {
        /// All arguments (IDs and text mixed)
        #[arg(trailing_var_arg = true, allow_hyphen_values = false)]
        args: Vec<String>,
        #[arg(short, long)]
        date: Option<String>,
    },
    #[command(
        alias = "l",
        about = "List all tasks with their status, id, date, and text"
    )]
    List,
    #[command(
        alias = "r",
        about = "Restore database from backup file (.json.backup)"
    )]
    Restore,
}

fn main() -> Result<()> {
    // Enable ANSI color support on Windows
    windows_console::enable_ansi_support();
    
    let cli = Cli::parse();
    let mut tm = TaskManager::new()?;
    
    match cli.command {
        Some(Command::Add { text, date }) => {
            if let Err(e) = HandlerCLI::handle_add_task(&mut tm, text, date) {
                eprintln!("{}", format!("Error: {}", e).red());
                std::process::exit(1);
            }
        }
        Some(Command::Del { ids, all }) => {
            let parsed_ids = parse_flexible_ids(&ids);
            HandlerCLI::handle_delete_tasks(&mut tm, parsed_ids, all)?;
        }
        Some(Command::Mark { ids }) => {
            let parsed_ids = parse_flexible_ids(&ids);
            if parsed_ids.is_empty() {
                eprintln!("{}", "Error: No valid task IDs provided".red());
                std::process::exit(1);
            }
            HandlerCLI::handle_mark_tasks(&mut tm, parsed_ids)?;
        }
        Some(Command::Edit { args, date }) => {
            if args.is_empty() {
                eprintln!("{}", "Error: No arguments provided for edit command".red());
                std::process::exit(1);
            }
            
            // Parse arguments: first consecutive numbers are IDs, rest is text
            let mut ids = Vec::new();
            let mut text_parts = Vec::new();
            let mut parsing_ids = true;
            
            let mut i = 0;
            while i < args.len() {
                let arg = &args[i];
                
                // Skip date flags and their values
                if arg == "-d" || arg == "--date" {
                    i += 2; // Skip flag and its value
                    continue;
                }
                
                if parsing_ids {
                    // Try to parse as ID (number)
                    if let Ok(id) = arg.parse::<u8>() {
                        ids.push(id);
                    } else if arg.contains(',') {
                        // Handle comma-separated IDs like "1,2,3"
                        let mut found_any_valid_id = false;
                        for part in arg.split(',') {
                            let trimmed = part.trim();
                            if let Ok(id) = trimmed.parse::<u8>() {
                                ids.push(id);
                                found_any_valid_id = true;
                            }
                            // Skip invalid parts silently, but continue parsing
                        }
                        
                        if !found_any_valid_id {
                            // No valid IDs found in comma-separated string, switch to text
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
            
            if ids.is_empty() {
                eprintln!("{}", "Error: No valid task IDs provided".red());
                std::process::exit(1);
            }
            
            let text_option = if text_parts.is_empty() { None } else { Some(text_parts) };
            HandlerCLI::handle_edit_tasks(&mut tm, ids, text_option, date)?;
        }
        Some(Command::List) | None => {
            HandlerCLI::handle_list_tasks(tm.tasks());
        }
        Some(Command::Restore) => {
            // For restore, create a TaskManager without loading the potentially corrupted database
            let mut restore_tm = match TaskManager::new_for_restore() {
                Ok(tm) => tm,
                Err(e) => {
                    eprintln!("{}", format!("Error: {}", e).red());
                    std::process::exit(1);
                }
            };
            
            if let Err(e) = HandlerCLI::handle_restore(&mut restore_tm) {
                eprintln!("{}", format!("Error: {}", e).red());
                std::process::exit(1);
            }
        }
    }
    
    Ok(())
}
