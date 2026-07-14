use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
#[cfg(feature = "completions")]
use colored::*;
use rusk::{
    BareEditDateFlag, TaskManager,
    args::{Cli, Command},
    cli::HandlerCLI,
    config,
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
    args.windows(2)
        .any(|w| (w[0] == "-d" || w[0] == "--date") && is_cli_date_help_value(&w[1]))
}

/// Prints a CLI error with one blank line before and after (stderr).
fn eprint_cli_error(msg: impl std::fmt::Display) {
    eprintln!("\n{}\n", msg);
}

/// Prints `Error: <msg>` in the theme error color and exits with code 1.
fn exit_with_error(msg: impl std::fmt::Display) -> ! {
    eprint_cli_error(config::theme().error.paint(&format!("Error: {msg}")));
    std::process::exit(1);
}

/// Drops argv words that look like flags (leading `-`) before flexible ID parsing.
fn non_flag_args(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|arg| !arg.trim_start().starts_with('-'))
        .cloned()
        .collect()
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(err) => match err.downcast_ref::<AppError>() {
            Some(AppError::UserCancel) | Some(AppError::SkipTask) => std::process::exit(0),
            Some(AppError::UserAbort) => std::process::exit(130),
            None => {
                eprint_cli_error(config::theme().error.paint(&format!("Error: {err}")));
                std::process::exit(1);
            }
        },
    }
}

fn run() -> Result<()> {
    windows_console::enable_ansi_support();

    let outcome = config::load();
    config::init(outcome.config);

    // RUSK_NO_COLOR: disable ANSI colors when set to any non-empty value
    // (mirrors NO_COLOR semantics, which `colored` also respects on its own).
    // The environment wins over `no_color` from the config file; the config
    // value can only disable colors, never re-enable them.
    if std::env::var_os("RUSK_NO_COLOR").is_some_and(|v| !v.is_empty())
        || config::config().no_color
    {
        colored::control::set_override(false);
    }

    // After the color override so warnings respect no_color.
    for warning in &outcome.warnings {
        eprintln!(
            "{}",
            config::theme().warning.paint(&format!("Warning: {warning}"))
        );
    }

    let cli = Cli::parse();

    // `sync` loads the database itself (and must not create sample tasks),
    // so intercept it before the shared TaskManager is created.
    #[cfg(feature = "sync")]
    if let Some(Command::Sync { direction }) = &cli.command {
        use rusk::args::SyncDirection;
        let direction = match direction {
            None => rusk::sync::Direction::Auto,
            Some(SyncDirection::Push { force }) => rusk::sync::Direction::Push { force: *force },
            Some(SyncDirection::Pull { force }) => rusk::sync::Direction::Pull { force: *force },
        };
        return rusk::sync::run(direction);
    }

    // `serve` never uses a shared TaskManager: the server re-reads the
    // database on every request (see web::server), so intercept it early
    // like `completions`.
    #[cfg(feature = "web")]
    if let Some(Command::Serve { host, port }) = &cli.command {
        let cfg = config::config();
        return rusk::web::server::run(rusk::web::server::ServeOptions {
            host: host.clone().unwrap_or_else(|| cfg.web_host.clone()),
            port: port.unwrap_or(cfg.web_port),
            token: cfg.web_token.clone(),
        });
    }

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
            exit_with_error(
                "`-d _` cannot be used when adding a task with no text: there is no date to clear. \
                 Omit `--date` or use `rusk add` with a non-empty first line in the editor; see `rusk add --help`.",
            );
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
                exit_with_error("No arguments provided for edit command");
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
                        exit_with_error(
                            "interactive `rusk add` requires a terminal. \
                             Pass the task on the command line, e.g. `rusk add buy milk`.",
                        );
                    }
                    HandlerCLI::handle_add_task_interactive(&mut tm, date)?;
                }
                #[cfg(not(feature = "interactive"))]
                {
                    HandlerCLI::handle_add_task(&mut tm, text, date)?;
                }
            } else {
                HandlerCLI::handle_add_task(&mut tm, text, date)?;
            }
        }
        Some(Command::Del { ids, done }) => {
            let parsed_ids = parse_flexible_ids(&non_flag_args(&ids));
            HandlerCLI::handle_delete_tasks(&mut tm, parsed_ids, done)?;
        }
        Some(Command::Mark { ids, priority }) => {
            let parsed_ids = parse_flexible_ids(&non_flag_args(&ids));
            if parsed_ids.is_empty() {
                exit_with_error("No valid task IDs provided");
            }
            HandlerCLI::handle_mark_tasks(&mut tm, parsed_ids, priority)?;
        }
        Some(Command::Edit { args }) => {
            let (args, opt_date) = match strip_edit_date_flag(args) {
                Ok(p) => p,
                Err(BareEditDateFlag) => {
                    exit_with_error(
                        "`rusk edit` does not support `-d` / `--date` without a value. \
                         Use `rusk edit <id>` to set the due date on the first line of the task text in the editor, \
                         or pass a date: `rusk edit <id> -d 31-12-2025` or `rusk edit <id> -d 2w` (see `rusk add --help` for syntax).",
                    );
                }
            };

            let (ids, text_option) = parse_edit_args(args);

            if ids.is_empty() {
                exit_with_error("No valid task IDs provided");
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
                            config::theme()
                                .error
                                .paint("Interactive editing requires the 'interactive' feature"),
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
                HandlerCLI::handle_list_tasks(tm.tasks(), compact || config::config().compact);
            }
        }
        Some(Command::Search { query, id }) => {
            HandlerCLI::handle_search_tasks(tm.tasks(), &query.join(" "), id);
        }
        None => {
            HandlerCLI::handle_list_tasks(tm.tasks(), config::config().compact);
        }
        Some(Command::Restore) => {
            let mut restore_tm = TaskManager::new_for_restore()?;
            HandlerCLI::handle_restore(&mut restore_tm)?;
        }
        #[cfg(feature = "web")]
        Some(Command::Gen { output }) => {
            let html = rusk::web::render_static_page(tm.tasks())?;
            if output == "-" {
                print!("{html}");
            } else {
                std::fs::write(&output, &html)
                    .with_context(|| format!("Failed to write {output}"))?;
                println!(
                    "{} {} ({} tasks)",
                    config::theme().success.paint("Generated"),
                    output,
                    tm.tasks().len()
                );
            }
        }
        #[cfg(feature = "web")]
        Some(Command::Serve { .. }) => {
            unreachable!("serve is handled before TaskManager::new()");
        }
        #[cfg(feature = "sync")]
        Some(Command::Sync { .. }) => {
            unreachable!("sync is handled before TaskManager::new()");
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
        exit_with_error("At least one shell must be specified");
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
            config::theme().success.paint("✓"),
            config::theme()
                .success
                .paint(&format!("{} completion installed to:", shell_name(shell))),
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
                config::theme().info.paint("Setup instructions for"),
                config::theme().info.paint(&shell_name(shell)).bold()
            );
        }
        println!("{}", config::theme().info.paint(&instructions));
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
