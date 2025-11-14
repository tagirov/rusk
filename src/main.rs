use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::*;
use rusk::{TaskManager, cli::HandlerCLI, completions::Shell, parse_edit_args, parse_flexible_ids, windows_console};
use std::path::PathBuf;

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
        about = "Add a new task (alias: \x1b[1ma\x1b[0m). Example: rusk add buy groceries. With a specific date: rusk add buy groceries --date 01-07-2025"
    )]
    Add {
        text: Vec<String>,
        #[arg(short, long)]
        date: Option<String>,
    },
    #[command(
        alias = "d",
        about = "Delete tasks by id(s) (alias: \x1b[1md\x1b[0m). Use --done to delete all completed tasks. Examples: rusk del 3, rusk del 1 2 3, rusk del 1,2,3"
    )]
    Del {
        #[arg(trailing_var_arg = true)]
        ids: Vec<String>,
        #[arg(long)]
        done: bool,
    },
    #[command(
        alias = "m",
        about = "Mark tasks as done/undone by id(s) (alias: \x1b[1mm\x1b[0m). Examples: rusk mark 3, rusk mark 1 2 3, rusk mark 1,2,3"
    )]
    Mark {
        #[arg(trailing_var_arg = true)]
        ids: Vec<String>,
    },
    #[command(
        alias = "e",
        about = "Edit tasks by id(s) (alias: \x1b[1me\x1b[0m). Text can be provided without quotes. Examples: rusk e 3 new task text -d 01-11-2025, rusk e 1 2 3 shared text"
    )]
    Edit {
        /// All arguments (IDs and text mixed)
        #[arg(trailing_var_arg = true, allow_hyphen_values = false)]
        args: Vec<String>,
        #[arg(short, long, value_name = "DATE", num_args = 0..=1)]
        date: Option<Option<String>>,
    },
    #[command(
        alias = "l",
        about = "List all tasks with their status, id, date, and text (alias: \x1b[1ml\x1b[0m)"
    )]
    List,
    #[command(
        alias = "r",
        about = "Restore database from backup file (.json.backup) (alias: \x1b[1mr\x1b[0m)"
    )]
    Restore,
    #[command(
        about = "Install shell completions. Example: rusk completions install bash"
    )]
    Completions {
        #[command(subcommand)]
        action: CompletionAction,
    },
}

#[derive(Subcommand)]
enum CompletionAction {
    #[command(about = "Install completions for a shell")]
    Install {
        #[arg(value_enum)]
        shell: Shell,
        #[arg(short, long, help = "Output file path (default: auto-detect based on shell)")]
        output: Option<PathBuf>,
    },
    #[command(about = "Show completion script (for manual installation)")]
    Show {
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> Result<()> {
    // Enable ANSI color support on Windows
    windows_console::enable_ansi_support();

    let cli = Cli::parse();
    let mut tm = TaskManager::new()?;

    match cli.command {
        Some(Command::Add { text, date }) => {
            if let Err(e) = HandlerCLI::handle_add_task(&mut tm, text, date) {
                eprintln!("{}", format!("Error: {e}").red());
                std::process::exit(1);
            }
        }
        Some(Command::Del { ids, done }) => {
            let parsed_ids = parse_flexible_ids(&ids);
            HandlerCLI::handle_delete_tasks(&mut tm, parsed_ids, done)?;
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

            let (ids, text_option) = parse_edit_args(args.clone());

            // Detect presence of -d/--date in raw args when clap didn't capture it
            // This handles cases where trailing var args swallow flags
            let mut date_flag_present = false;
            let mut inline_date_value: Option<String> = None;
            let mut i = 0usize;
            while i < args.len() {
                let a = &args[i];
                if a == "-d" || a == "--date" {
                    if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                        inline_date_value = Some(args[i + 1].clone());
                        i += 1; // skip value
                    } else {
                        date_flag_present = true; // interactive date
                    }
                }
                i += 1;
            }

            if ids.is_empty() {
                eprintln!("{}", "Error: No valid task IDs provided".red());
                std::process::exit(1);
            }

            // Prefer explicit clap-parsed date; otherwise fall back to inline detection
            let effective_date_opt = match date {
                Some(Some(d)) => Some(Some(d)),
                Some(None) => Some(None),
                None => inline_date_value
                    .map(Some)
                    .or(if date_flag_present { Some(None) } else { None }),
            };

            match (text_option, effective_date_opt) {
                // No text; date provided with value -> change only date, no interaction
                (None, Some(Some(d))) => {
                    HandlerCLI::handle_edit_tasks(&mut tm, ids, None, Some(d))?
                }
                // No text; -d provided without value -> interactive (text then date)
                (None, Some(None)) => HandlerCLI::handle_edit_tasks_interactive(&mut tm, ids)?,
                // No text; no -d -> interactive text-only edit
                (None, None) => HandlerCLI::handle_edit_tasks_interactive_text_only(&mut tm, ids)?,
                // Text provided -> standard non-interactive edit; pass through date if given with value
                (Some(text), Some(Some(d))) => {
                    HandlerCLI::handle_edit_tasks(&mut tm, ids, Some(text), Some(d))?
                }
                (Some(text), _) => HandlerCLI::handle_edit_tasks(&mut tm, ids, Some(text), None)?,
            }
        }
        Some(Command::List) | None => {
            HandlerCLI::handle_list_tasks(tm.tasks());
        }
        Some(Command::Restore) => {
            // For restore, create a TaskManager without loading the potentially corrupted database
            let mut restore_tm = match TaskManager::new_for_restore() {
                Ok(tm) => tm,
                Err(e) => {
                    eprintln!("{}", format!("Error: {e}").red());
                    std::process::exit(1);
                }
            };

            if let Err(e) = HandlerCLI::handle_restore(&mut restore_tm) {
                eprintln!("{}", format!("Error: {e}").red());
                std::process::exit(1);
            }
        }
        Some(Command::Completions { action }) => {
            match action {
                CompletionAction::Install { shell, output } => {
                    handle_completions_install(shell, output)?;
                }
                CompletionAction::Show { shell } => {
                    handle_completions_show(shell)?;
                }
            }
        }
    }

    Ok(())
}

fn handle_completions_install(shell: Shell, output: Option<PathBuf>) -> Result<()> {
    let script = shell.get_script();
    let path = match output {
        Some(p) => p,
        None => shell.get_default_path()?,
    };

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Write completion script
    std::fs::write(&path, script)
        .with_context(|| format!("Failed to write completion file: {}", path.display()))?;

    println!(
        "{} {} {}",
        "✓".green(),
        "Completion installed to:".green(),
        path.display()
    );

    // Print setup instructions
    let instructions = shell.get_instructions(&path);
    println!("\n{}", instructions.cyan());

    Ok(())
}

fn handle_completions_show(shell: Shell) -> Result<()> {
    let script = shell.get_script();
    print!("{}", script);
    Ok(())
}
