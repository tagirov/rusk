use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use rusk::{TaskManager, cli::HandlerCLI, windows_console, parse_flexible_ids, parse_edit_args};

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
            
            let (ids, text_option) = parse_edit_args(args);
            
            if ids.is_empty() {
                eprintln!("{}", "Error: No valid task IDs provided".red());
                std::process::exit(1);
            }
            
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
