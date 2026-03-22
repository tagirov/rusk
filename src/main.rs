use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use colored::*;
use rusk::{
    TaskManager, cli::HandlerCLI, completions::Shell, is_cli_date_help_value, parse_edit_args,
    parse_flexible_ids, windows_console,
};

const DATE_FORMAT_LONG_HELP: &str = "\
Date value for -d / --date (and interactive date entry when `edit … --date` with no value):
  Absolute    DD-MM-YYYY (slashes ok; short year ok, e.g. 1-3-25).
  Relative    Offset from today's local date. Chain segments with no spaces.
              Suffixes: d=days, w=weeks, m=months, q=quarters (3 months), y=years.
              Examples: 2d, 2w, 5m, 3q, 2y, 10d5w, 12d2q1y.
  Subcommand  Pass -h or --help as the date value for this command's help (e.g. -d -h).\n";

#[derive(Parser)]
#[command(
    version,
    about,
    after_help = "Without COMMAND, lists all tasks (same as `rusk list`).\n\nEnvironment:\n  RUSK_DB    Optional path to the tasks database file or directory.\n\nShell tab completion:\n  rusk completions install <shell> [<shell> ...]\n  rusk completions show <shell>",
    after_long_help = "Due dates: calendar form (DD-MM-YYYY) or relative (e.g. 2w, 10d5w) on add/edit. See `rusk add --help` for the full date syntax.\n"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    #[command(
        visible_alias = "a",
        about = "Add a new task. Example: rusk add buy groceries. With a date: rusk add buy groceries --date 01-07-2025 or --date 2w",
        help_template = "{about-section}\n\nUsage: rusk add [TEXT]... [OPTIONS]\n\n{all-args}\n\n{after-help}",
        after_long_help = DATE_FORMAT_LONG_HELP
    )]
    Add {
        #[arg(value_name = "TEXT", help = "Task text")]
        text: Vec<String>,
        #[arg(short, long, value_name = "DATE", allow_hyphen_values = true, help = "Due date: DD-MM-YYYY (1-7-25 ok), or relative from today (2d, 3q, 10d5w, …). Full syntax under `rusk add --help`. -d -h / -d --help for subcommand help")]
        date: Option<String>,
    },
    #[command(
        visible_alias = "d",
        about = "Delete tasks by ID. Supports multiple IDs and comma-separated input. Examples: rusk del 3, rusk del 1,2,3, rusk del --done",
        help_template = "{about-section}\n\nUsage: rusk del [IDS]... [OPTIONS]\n\n{all-args}"
    )]
    Del {
        #[arg(trailing_var_arg = true, value_name = "IDS", help = "Task IDs to delete (space- or comma-separated)")]
        ids: Vec<String>,
        #[arg(long, help = "Delete all completed tasks")]
        done: bool,
    },
    #[command(
        visible_alias = "m",
        about = "Toggle task completion status by ID. Supports multiple IDs and comma-separated input. Examples: rusk mark 3, rusk mark 1,2,3"
    )]
    Mark {
        #[arg(trailing_var_arg = true, allow_hyphen_values = false, value_name = "IDS", help = "Task IDs to mark/unmark (space- or comma-separated)")]
        ids: Vec<String>,
    },
    #[command(
        visible_alias = "e",
        about = "Edit tasks by ID. Without text: `rusk edit 1` starts interactive text edit; `rusk edit 1 --date` edits text and date interactively (same date formats as -d, including relative). Examples: rusk e 3 new text -d 01-11-2025, rusk e 1 -d 2w",
        help_template = "{about-section}\n\nUsage: rusk edit [ARGS]... [OPTIONS]\n\n{all-args}\n\n{after-help}",
        after_long_help = DATE_FORMAT_LONG_HELP
    )]
    Edit {
        /// All arguments (IDs and text mixed)
        #[arg(trailing_var_arg = true, allow_hyphen_values = false, value_name = "ARGS", help = "IDs and optional new text")]
        args: Vec<String>,
        #[arg(short, long, value_name = "DATE", num_args = 0..=1, allow_hyphen_values = true, help = "Set date: DD-MM-YYYY or relative (2w, 10d5w, …). Omit value for interactive edit (prompt uses the same formats). Full syntax: `rusk edit --help`. -d -h for subcommand help")]
        date: Option<Option<String>>,
    },
    #[command(
        visible_alias = "l",
        about = "List all tasks with status, ID, date, and text. Running `rusk` without a subcommand does the same"
    )]
    List {
        #[arg(long, hide = true, default_value_t = false)]
        for_completion: bool,
    },
    #[command(
        visible_alias = "r",
        about = "Restore task database from backup (.json.backup)"
    )]
    Restore,
    #[command(
        visible_alias = "c",
        about = "Manage shell completions. Example: rusk completions install bash, or rusk completions install fish nu"
    )]
    Completions {
        #[command(subcommand)]
        action: CompletionAction,
    },
}

#[derive(Subcommand)]
enum CompletionAction {
    #[command(about = "Install completions for one or more shells")]
    Install {
        #[arg(value_enum, required = true, num_args = 1..)]
        shells: Vec<Shell>,
    },
    #[command(about = "Show completion script (for manual installation)")]
    Show {
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn print_subcommand_help(name: &str) -> anyhow::Result<()> {
    let mut cmd = Cli::command();
    let sub = cmd
        .find_subcommand_mut(name)
        .with_context(|| format!("missing subcommand {name}"))?;
    sub.print_long_help()?;
    Ok(())
}

/// True if any `-d` / `--date` is immediately followed by `-h` / `--help`.
fn args_have_date_then_help(args: &[String]) -> bool {
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "-d" || args[i] == "--date" {
            if i + 1 < args.len() && is_cli_date_help_value(&args[i + 1]) {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn main() -> Result<()> {
    // Enable ANSI color support on Windows
    windows_console::enable_ansi_support();

    let cli = Cli::parse();
    let mut tm = TaskManager::new()?;

    match cli.command {
        Some(Command::Add { text, date }) => {
            if let Some(ref d) = date {
                if is_cli_date_help_value(d) {
                    print_subcommand_help("add")?;
                    return Ok(());
                }
            }
            if let Err(e) = HandlerCLI::handle_add_task(&mut tm, text, date) {
                eprintln!("{}", format!("Error: {e}").red());
                std::process::exit(1);
            }
        }
        Some(Command::Del { ids, done }) => {
            // Filter out flags (arguments starting with -)
            let filtered_ids: Vec<String> = ids.iter()
                .filter(|arg| !arg.trim_start().starts_with('-'))
                .cloned()
                .collect();
            
            let parsed_ids = parse_flexible_ids(&filtered_ids);
            HandlerCLI::handle_delete_tasks(&mut tm, parsed_ids, done)?;
        }
        Some(Command::Mark { ids }) => {
            // Filter out flags (arguments starting with -)
            // This will filter out -h, --help, and any other flags
            let filtered_ids: Vec<String> = ids.iter()
                .filter(|arg| {
                    let trimmed = arg.trim();
                    !trimmed.starts_with('-')
                })
                .cloned()
                .collect();
            
            // If after filtering we have no IDs, show error
            if filtered_ids.is_empty() {
                eprintln!("{}", "Error: No valid task IDs provided".red());
                std::process::exit(1);
            }
            
            let parsed_ids = parse_flexible_ids(&filtered_ids);
            if parsed_ids.is_empty() {
                eprintln!("{}", "Error: No valid task IDs provided".red());
                std::process::exit(1);
            }
            HandlerCLI::handle_mark_tasks(&mut tm, parsed_ids)?;
        }
        Some(Command::Edit { args, date }) => {
            if args_have_date_then_help(&args) {
                print_subcommand_help("edit")?;
                return Ok(());
            }
            if let Some(Some(ref d)) = date {
                if is_cli_date_help_value(d) {
                    print_subcommand_help("edit")?;
                    return Ok(());
                }
            }
            // Trailing -h / --help in ARGS (e.g. `rusk e 22 -h`) — clap cannot treat it as a flag here.
            if args
                .last()
                .is_some_and(|a| is_cli_date_help_value(a))
            {
                print_subcommand_help("edit")?;
                return Ok(());
            }
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
                    if i + 1 < args.len() {
                        let next = &args[i + 1];
                        if is_cli_date_help_value(next) {
                            i += 1; // skip -h/--help (help already handled above)
                        } else if !next.starts_with('-') {
                            inline_date_value = Some(next.clone());
                            i += 1; // skip value
                        } else {
                            date_flag_present = true; // interactive date
                        }
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
        Some(Command::List { for_completion }) => {
            if for_completion {
                HandlerCLI::handle_list_tasks_for_completion(tm.tasks());
            } else {
                HandlerCLI::handle_list_tasks(tm.tasks());
            }
        }
        None => {
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
                CompletionAction::Install { shells } => {
                    handle_completions_install(shells)?;
                }
                CompletionAction::Show { shell } => {
                    handle_completions_show(shell)?;
                }
            }
        }
    }

    Ok(())
}

fn handle_completions_install(shells: Vec<Shell>) -> Result<()> {
    if shells.is_empty() {
        eprintln!("{}", "Error: At least one shell must be specified".red());
        std::process::exit(1);
    }

    let shells_count = shells.len();
    let mut installed_paths = Vec::new();

    for shell in &shells {
        let script = shell.get_script();
        let path = shell.get_default_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        std::fs::write(&path, script)
            .with_context(|| format!("Failed to write completion file: {}", path.display()))?;

        println!(
            "{} {} {}",
            "✓".green(),
            format!("{} completion installed to:", shell_name(shell)).green(),
            path.display()
        );

        installed_paths.push((shell, path));
    }

    // Print setup instructions for all installed shells
    if shells_count > 1 {
        println!(); // Add blank line before instructions
    }
    
    for (idx, (shell, path)) in installed_paths.iter().enumerate() {
        let instructions = shell.get_instructions(path);
        if shells_count > 1 {
            println!("{} {}:", "Setup instructions for".cyan(), shell_name(shell).cyan().bold());
        }
        println!("{}", instructions.cyan());
        if idx < installed_paths.len() - 1 {
            println!(); // Add blank line between instructions for different shells
        }
    }

    Ok(())
}

fn shell_name(shell: &Shell) -> String {
    match shell {
        Shell::Bash => "Bash",
        Shell::Zsh => "Zsh",
        Shell::Fish => "Fish",
        Shell::Nu => "Nu Shell",
        Shell::PowerShell => "PowerShell",
    }.to_string()
}

fn handle_completions_show(shell: Shell) -> Result<()> {
    let script = shell.get_script();
    print!("{}", script);
    Ok(())
}
