use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use colored::*;
use rusk::{
    BareEditDateFlag, TaskManager,
    args::{Cli, Command},
    cli::HandlerCLI,
    error::AppError,
    is_cli_date_help_value, parse_edit_args, parse_flexible_ids,
    parser::date::is_cli_date_clear_value,
    strip_edit_date_flag, windows_console,
};
#[cfg(feature = "completions")]
use rusk::{args::CompletionAction, completions::Shell};

fn print_subcommand_help(name: &str) -> anyhow::Result<()> {
    let mut cmd = Cli::command();
    let sub = cmd
        .find_subcommand_mut(name)
        .with_context(|| format!("missing subcommand {name}"))?;
    sub.print_long_help()?;
    Ok(())
}

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

/// Prints a CLI error with one blank line before and after (stderr).
fn eprint_cli_error(msg: impl std::fmt::Display) {
    eprintln!("\n{}\n", msg);
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(err) => match err.downcast_ref::<AppError>() {
            Some(AppError::UserCancel) | Some(AppError::SkipTask) => std::process::exit(0),
            Some(AppError::UserAbort) => std::process::exit(130),
            None => {
                eprint_cli_error(format!("Error: {err}").red());
                std::process::exit(1);
            }
        },
    }
}

fn run() -> Result<()> {
    windows_console::enable_ansi_support();

    // RUSK_NO_COLOR: disable ANSI colors when set to any non-empty value
    // (mirrors NO_COLOR semantics, which `colored` also respects on its own).
    if std::env::var_os("RUSK_NO_COLOR").is_some_and(|v| !v.is_empty()) {
        colored::control::set_override(false);
    }

    let cli = Cli::parse();

    #[cfg(feature = "completions")]
    if let Some(Command::Completions { action }) = &cli.command {
        match action {
            CompletionAction::Install { shells } => {
                handle_completions_install(shells.clone())?;
            }
            CompletionAction::Show { shell } => {
                handle_completions_show(*shell)?;
            }
        }
        return Ok(());
    }

    // Help-only and validation paths that must not open the database. Debug/test builds use a
    // fixed tasks.json path; parallel integration tests would otherwise race on TaskManager::new()
    // before we detect trailing `-h` / `--help` on `edit`.
    match &cli.command {
        Some(Command::Add {
            text,
            date: Some(d),
        }) if text.is_empty() && is_cli_date_clear_value(d) => {
            eprint_cli_error(
                "Error: `-d _` cannot be used when adding a task with no text: there is no date to clear. \
                 Omit `--date` or use `rusk add` with a non-empty first line in the editor; see `rusk add --help`."
                    .red(),
            );
            std::process::exit(1);
        }
        Some(Command::Add { date: Some(d), .. }) if is_cli_date_help_value(d) => {
            print_subcommand_help("add")?;
            return Ok(());
        }
        Some(Command::Edit { args }) => {
            if args_have_date_then_help(args) {
                print_subcommand_help("edit")?;
                return Ok(());
            }
            if args.last().is_some_and(|a| is_cli_date_help_value(a)) {
                print_subcommand_help("edit")?;
                return Ok(());
            }
            if args.is_empty() {
                eprint_cli_error("Error: No arguments provided for edit command".red());
                std::process::exit(1);
            }
        }
        _ => {}
    }

    let mut tm = TaskManager::new()?;

    match cli.command {
        Some(Command::Add { text, date }) => {
            if text.is_empty() {
                #[cfg(feature = "interactive")]
                {
                    use std::io::IsTerminal;
                    if !std::io::stdout().is_terminal() {
                        eprint_cli_error(
                            "Error: interactive `rusk add` requires a terminal. \
                             Pass the task on the command line, e.g. `rusk add buy milk`."
                                .red(),
                        );
                        std::process::exit(1);
                    }
                    if let Err(e) = HandlerCLI::handle_add_task_interactive(&mut tm, date) {
                        eprint_cli_error(format!("Error: {e}").red());
                        std::process::exit(1);
                    }
                }
                #[cfg(not(feature = "interactive"))]
                {
                    if let Err(e) = HandlerCLI::handle_add_task(&mut tm, text, date) {
                        eprint_cli_error(format!("Error: {e}").red());
                        std::process::exit(1);
                    }
                }
            } else if let Err(e) = HandlerCLI::handle_add_task(&mut tm, text, date) {
                eprint_cli_error(format!("Error: {e}").red());
                std::process::exit(1);
            }
        }
        Some(Command::Del { ids, done }) => {
            let filtered_ids: Vec<String> = ids
                .iter()
                .filter(|arg| !arg.trim_start().starts_with('-'))
                .cloned()
                .collect();

            let parsed_ids = parse_flexible_ids(&filtered_ids);
            HandlerCLI::handle_delete_tasks(&mut tm, parsed_ids, done)?;
        }
        Some(Command::Mark { ids, priority }) => {
            let filtered_ids: Vec<String> = ids
                .iter()
                .filter(|arg| {
                    let trimmed = arg.trim();
                    !trimmed.starts_with('-')
                })
                .cloned()
                .collect();

            if filtered_ids.is_empty() {
                eprint_cli_error("Error: No valid task IDs provided".red());
                std::process::exit(1);
            }

            let parsed_ids = parse_flexible_ids(&filtered_ids);
            if parsed_ids.is_empty() {
                eprint_cli_error("Error: No valid task IDs provided".red());
                std::process::exit(1);
            }
            HandlerCLI::handle_mark_tasks(&mut tm, parsed_ids, priority)?;
        }
        Some(Command::Edit { args }) => {
            let (args, opt_date) = match strip_edit_date_flag(args) {
                Ok(p) => p,
                Err(BareEditDateFlag) => {
                    eprint_cli_error(
                        "Error: `rusk edit` does not support `-d` / `--date` without a value. \
                         Use `rusk edit <id>` to set the due date on the first line of the task text in the editor, \
                         or pass a date: `rusk edit <id> -d 31-12-2025` or `rusk edit <id> -d 2w` (see `rusk add --help` for syntax)."
                            .red(),
                    );
                    std::process::exit(1);
                }
            };

            let (ids, text_option) = parse_edit_args(args);

            if ids.is_empty() {
                eprint_cli_error("Error: No valid task IDs provided".red());
                std::process::exit(1);
            }

            match (text_option, opt_date) {
                (None, Some(d)) => HandlerCLI::handle_edit_tasks(&mut tm, ids, None, Some(d))?,
                (Some(text), Some(d)) => {
                    HandlerCLI::handle_edit_tasks(&mut tm, ids, Some(text), Some(d))?
                }
                (None, None) => {
                    #[cfg(feature = "interactive")]
                    {
                        HandlerCLI::handle_edit_tasks_interactive(&mut tm, ids)?
                    }
                    #[cfg(not(feature = "interactive"))]
                    {
                        eprint_cli_error(
                            "Interactive editing requires the 'interactive' feature".red(),
                        );
                        std::process::exit(1);
                    }
                }
                (Some(text), None) => {
                    HandlerCLI::handle_edit_tasks(&mut tm, ids, Some(text), None)?
                }
            }
        }
        Some(Command::List {
            for_completion,
            compact,
        }) => {
            if for_completion {
                HandlerCLI::handle_list_tasks_for_completion(tm.tasks());
            } else {
                HandlerCLI::handle_list_tasks(tm.tasks(), compact);
            }
        }
        None => {
            HandlerCLI::handle_list_tasks(tm.tasks(), false);
        }
        Some(Command::Restore) => {
            let mut restore_tm = match TaskManager::new_for_restore() {
                Ok(tm) => tm,
                Err(e) => {
                    eprint_cli_error(format!("Error: {e}").red());
                    std::process::exit(1);
                }
            };

            if let Err(e) = HandlerCLI::handle_restore(&mut restore_tm) {
                eprint_cli_error(format!("Error: {e}").red());
                std::process::exit(1);
            }
        }
        #[cfg(feature = "completions")]
        Some(Command::Completions { .. }) => {
            unreachable!("completions are handled before TaskManager::new()");
        }
    }

    Ok(())
}

#[cfg(feature = "completions")]
fn handle_completions_install(shells: Vec<Shell>) -> Result<()> {
    if shells.is_empty() {
        eprint_cli_error("Error: At least one shell must be specified".red());
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

    if shells_count > 1 {
        println!();
    }

    for (idx, (shell, path)) in installed_paths.iter().enumerate() {
        let instructions = shell.get_instructions(path);
        if shells_count > 1 {
            println!(
                "{} {}:",
                "Setup instructions for".cyan(),
                shell_name(shell).cyan().bold()
            );
        }
        println!("{}", instructions.cyan());
        if idx < installed_paths.len() - 1 {
            println!();
        }
    }

    Ok(())
}

#[cfg(feature = "completions")]
fn shell_name(shell: &Shell) -> String {
    match shell {
        Shell::Bash => "Bash",
        Shell::Zsh => "Zsh",
        Shell::Fish => "Fish",
        Shell::Nu => "Nu Shell",
        Shell::PowerShell => "PowerShell",
    }
    .to_string()
}

#[cfg(feature = "completions")]
fn handle_completions_show(shell: Shell) -> Result<()> {
    let script = shell.get_script();
    print!("{}", script);
    Ok(())
}
