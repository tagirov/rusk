use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use colored::*;
use rusk::{
    TaskManager, cli::HandlerCLI, error::AppError, is_cli_date_help_value, parse_edit_args,
    parse_flexible_ids, windows_console,
    args::{Cli, Command},
};
#[cfg(feature = "completions")]
use rusk::{completions::Shell, args::CompletionAction};

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

fn main() {
    match run() {
        Ok(()) => {}
        Err(err) => match err.downcast_ref::<AppError>() {
            Some(AppError::UserCancel) | Some(AppError::SkipTask) => std::process::exit(0),
            Some(AppError::UserAbort) => std::process::exit(130),
            None => {
                eprintln!("{}", format!("Error: {err}").red());
                std::process::exit(1);
            }
        },
    }
}

fn run() -> Result<()> {
    windows_console::enable_ansi_support();

    // RUSK_NO_COLORS: disable ANSI colors when set to any non-empty value
    // (mirrors NO_COLOR semantics, which `colored` also respects on its own).
    if std::env::var_os("RUSK_NO_COLORS").is_some_and(|v| !v.is_empty()) {
        colored::control::set_override(false);
    }

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
            let filtered_ids: Vec<String> = ids.iter()
                .filter(|arg| !arg.trim_start().starts_with('-'))
                .cloned()
                .collect();

            let parsed_ids = parse_flexible_ids(&filtered_ids);
            HandlerCLI::handle_delete_tasks(&mut tm, parsed_ids, done)?;
        }
        Some(Command::Mark { ids, priority }) => {
            let filtered_ids: Vec<String> = ids.iter()
                .filter(|arg| {
                    let trimmed = arg.trim();
                    !trimmed.starts_with('-')
                })
                .cloned()
                .collect();

            if filtered_ids.is_empty() {
                eprintln!("{}", "Error: No valid task IDs provided".red());
                std::process::exit(1);
            }

            let parsed_ids = parse_flexible_ids(&filtered_ids);
            if parsed_ids.is_empty() {
                eprintln!("{}", "Error: No valid task IDs provided".red());
                std::process::exit(1);
            }
            HandlerCLI::handle_mark_tasks(&mut tm, parsed_ids, priority)?;
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

            let mut date_flag_present = false;
            let mut inline_date_value: Option<String> = None;
            let mut i = 0usize;
            while i < args.len() {
                let a = &args[i];
                if a == "-d" || a == "--date" {
                    if i + 1 < args.len() {
                        let next = &args[i + 1];
                        if is_cli_date_help_value(next) {
                            i += 1;
                        } else if !next.starts_with('-') {
                            inline_date_value = Some(next.clone());
                            i += 1;
                        } else {
                            date_flag_present = true;
                        }
                    } else {
                        date_flag_present = true;
                    }
                }
                i += 1;
            }

            if ids.is_empty() {
                eprintln!("{}", "Error: No valid task IDs provided".red());
                std::process::exit(1);
            }

            let effective_date_opt = match date {
                Some(Some(d)) => Some(Some(d)),
                Some(None) => Some(None),
                None => inline_date_value
                    .map(Some)
                    .or(if date_flag_present { Some(None) } else { None }),
            };

            match (text_option, effective_date_opt) {
                (None, Some(Some(d))) => {
                    HandlerCLI::handle_edit_tasks(&mut tm, ids, None, Some(d))?
                }
                #[cfg(feature = "interactive")]
                (None, Some(None)) => HandlerCLI::handle_edit_tasks_interactive(&mut tm, ids)?,
                #[cfg(not(feature = "interactive"))]
                (None, Some(None)) => {
                    eprintln!("{}", "Interactive editing requires the 'interactive' feature".red());
                    std::process::exit(1);
                }
                #[cfg(feature = "interactive")]
                (None, None) => HandlerCLI::handle_edit_tasks_interactive_text_only(&mut tm, ids)?,
                #[cfg(not(feature = "interactive"))]
                (None, None) => {
                    eprintln!("{}", "Interactive editing requires the 'interactive' feature".red());
                    std::process::exit(1);
                }
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
        #[cfg(feature = "completions")]
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

#[cfg(feature = "completions")]
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

    if shells_count > 1 {
        println!();
    }

    for (idx, (shell, path)) in installed_paths.iter().enumerate() {
        let instructions = shell.get_instructions(path);
        if shells_count > 1 {
            println!("{} {}:", "Setup instructions for".cyan(), shell_name(shell).cyan().bold());
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
    }.to_string()
}

#[cfg(feature = "completions")]
fn handle_completions_show(shell: Shell) -> Result<()> {
    let script = shell.get_script();
    print!("{}", script);
    Ok(())
}
