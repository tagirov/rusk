use anyhow::{Context, Result};
use clap::{CommandFactory, FromArgMatches};
#[cfg(feature = "completions")]
use colored::*;
use rusk::{
    IdListError, TaskId, TaskManager,
    args::{Cli, Command},
    cli::HandlerCLI,
    config,
    error::AppError,
    is_cli_date_help_value, output, parse_edit_args, parse_id_args, parse_id_list,
    parser::date::is_cli_date_clear_value,
    validate_cli_date_edit_arg, windows_console,
};
#[cfg(feature = "completions")]
use rusk::{args::CompletionAction, completions::Shell};

/// The command line as clap reads it, painting its help and errors the way
/// the rest of the output is painted (see `output::Colors`).
fn cli_command() -> clap::Command {
    Cli::command().color(output::colors().clap())
}

fn print_subcommand_help(name: &str) -> anyhow::Result<()> {
    let mut cmd = cli_command();
    // Built, so the usage line names the subcommand as `rusk add`.
    cmd.build();
    let sub = cmd
        .find_subcommand_mut(name)
        .with_context(|| format!("missing subcommand {name}"))?;
    let help = sub.render_long_help();
    if output::colors().on {
        rusk::out!("{}", help.ansi())
    } else {
        rusk::out!("{help}")
    }
}

/// Prints a CLI error with one blank line before and after (stderr).
fn eprint_cli_error(msg: impl std::fmt::Display) {
    rusk::errln!("\n{}\n", msg);
}

/// `Error: <msg>` in the theme error color. An error may quote the database
/// (the line a JSON parser stopped at, a CSV cell, an iCalendar value), so
/// its control characters are escaped before it is styled.
fn paint_error(msg: impl std::fmt::Display) -> String {
    let text = format!("Error: {msg}");
    config::theme().error.paint(&rusk::printable::escape(&text)).to_string()
}

/// Prints `Error: <msg>` in the theme error color and exits with code 1.
fn exit_with_error(msg: impl std::fmt::Display) -> ! {
    eprint_cli_error(paint_error(msg));
    std::process::exit(1);
}

/// What `mark` / `del` / `edit` made of their task ids, or the error for
/// what they got instead, followed by `examples` of the command's usage.
fn ids_or_exit<T>(parsed: Result<T, IdListError>, examples: &str) -> T {
    parsed.unwrap_or_else(|err| match err {
        // Names the two ways out with the user's own words.
        IdListError::MoreIds { .. } => exit_with_error(err),
        _ => exit_with_error(format!("{err}; e.g. {examples}")),
    })
}

/// A `-d` value that cannot be a date: said before the database is read,
/// like every other wrong argument (see `run`). `_` passes (no date).
fn check_date_or_exit(date: Option<&str>) {
    if let Some(date) = date
        && let Err(err) = validate_cli_date_edit_arg(date)
    {
        exit_with_error(format!("{err:#}"));
    }
}

/// A text of nothing but whitespace, which no task may have.
fn check_text_or_exit(words: &[String]) {
    if rusk::storage::clean_text(&words.join(" ")).is_empty() {
        exit_with_error("Task text cannot be empty");
    }
}

/// A `--after` value: task ids, or `_` for "no dependencies" where clearing
/// makes sense (`edit`).
fn after_ids_or_exit(raw: &str, clear_allowed: bool) -> Vec<TaskId> {
    if clear_allowed && is_cli_date_clear_value(raw) {
        return Vec::new();
    }
    parse_id_list(raw).unwrap_or_else(|err| {
        let clear = if clear_allowed { " or `_` to clear" } else { "" };
        exit_with_error(format!(
            "`--after` expects comma-separated task ids (e.g. 19,22){clear}: {err}"
        ))
    })
}

fn main() {
    match run() {
        Ok(()) => {}
        // Whoever reads the output stopped reading (`rusk list | head`):
        // what was asked for is done, and nobody is left to tell.
        Err(err) if output::is_closed(&err) => std::process::exit(0),
        Err(err) => match err.downcast_ref::<AppError>() {
            Some(AppError::UserCancel) | Some(AppError::SkipTask) => std::process::exit(0),
            Some(AppError::UserAbort) => std::process::exit(130),
            None => {
                // `{:#}` prints the whole chain: the outermost context alone
                // ("Failed to read the database file") does not say what to
                // fix, the cause under it does (the OS error, what curl or
                // ssh wrote to stderr, SQLite's message).
                eprint_cli_error(paint_error(format_args!("{err:#}")));
                std::process::exit(1);
            }
        },
    }
}

fn run() -> Result<()> {
    rusk::running_as_the_command();
    windows_console::enable_ansi_support();

    let outcome = config::load();
    config::init(outcome.config);

    // One decision for all output, clap's help and errors included; the
    // config can only turn colors off, never back on.
    output::set_colors(output::Colors::decide(
        |name| std::env::var_os(name),
        config::config().no_color,
        std::io::IsTerminal::is_terminal(&std::io::stdout()),
    ));

    // After the color decision so warnings respect no_color.
    for warning in &outcome.warnings {
        rusk::errln!(
            "{}",
            config::theme().warning.paint(&format!("Warning: {warning}"))
        );
    }

    let cli = Cli::from_arg_matches(&cli_command().get_matches()).unwrap_or_else(|e| e.exit());

    // `sync` loads the database itself (and must not create sample tasks),
    // so intercept it before any TaskManager is created.
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

    // `serve` never keeps a TaskManager: the server re-reads the
    // database on every request (see web::server), so intercept it early
    // like `completions`.
    #[cfg(feature = "web")]
    if let Some(Command::Serve { host, port }) = &cli.command {
        let cfg = config::config();
        return rusk::web::server::run(rusk::web::server::ServeOptions {
            host: host.clone().unwrap_or_else(|| cfg.web_host.clone()),
            port: port.unwrap_or(cfg.web_port),
            token: cfg.web_token.clone(),
            timeout: (cfg.web_timeout > 0).then(|| std::time::Duration::from_secs(cfg.web_timeout)),
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

    // Help and argument errors that need no database. Each command below
    // also reads the rest of its arguments before it opens the database:
    // a wrong argument is reported as such, not hidden behind a database
    // that cannot be read, and a remote database is not contacted for it.
    match &cli.command {
        // `-d -h` / `-a -h`: the value slot takes `-h` (the options accept
        // values that start with `-`), so the help is asked for here.
        Some(Command::Add { date, after, .. })
            if [date, after].into_iter().flatten().any(|v| is_cli_date_help_value(v)) =>
        {
            print_subcommand_help("add")?;
            return Ok(());
        }
        Some(Command::Edit { date, after, .. })
            if [date, after].into_iter().flatten().any(|v| is_cli_date_help_value(v)) =>
        {
            print_subcommand_help("edit")?;
            return Ok(());
        }
        _ => {}
    }

    // `restore` must work exactly when the database cannot be loaded, so it
    // never goes through `TaskManager::new()` (which loads the database).
    if let Some(Command::Restore) = &cli.command {
        let mut restore_tm = TaskManager::new_for_restore()?;
        return HandlerCLI::handle_restore(&mut restore_tm);
    }

    match cli.command {
        Some(Command::Add { text, date, after }) => {
            let after_ids = after.as_deref().map_or_else(Vec::new, |raw| after_ids_or_exit(raw, false));
            check_date_or_exit(date.as_deref());
            if !text.is_empty() {
                check_text_or_exit(&text);
            }
            #[cfg(not(feature = "interactive"))]
            if text.is_empty() {
                exit_with_error(
                    "`rusk add` without text opens the editor, and this build has no \
                     'interactive' feature. Pass the task on the command line, e.g. \
                     `rusk add buy milk`.",
                );
            }
            #[cfg(feature = "interactive")]
            if text.is_empty() {
                if !HandlerCLI::on_a_terminal() {
                    exit_with_error(
                        "interactive `rusk add` requires a terminal. \
                         Pass the task on the command line, e.g. `rusk add buy milk`.",
                    );
                }
                if HandlerCLI::terminal_is_dumb() {
                    exit_with_error(
                        "the editor needs a terminal that can move the cursor, and TERM is \
                         `dumb`. Pass the task on the command line, e.g. `rusk add buy milk`.",
                    );
                }
                let mut tm = TaskManager::new()?;
                return HandlerCLI::handle_add_task_interactive(&mut tm, date, after_ids);
            }
            let mut tm = TaskManager::new()?;
            HandlerCLI::handle_add_task(&mut tm, text, date, after_ids)?;
        }
        Some(Command::Del { ids, done, yes }) => {
            // clap rejects `--done` together with ids.
            let ids = if done {
                Vec::new()
            } else {
                ids_or_exit(
                    parse_id_args(&ids),
                    "`rusk del 1,2,3`, or `rusk del --done` for all completed tasks",
                )
            };
            let mut tm = TaskManager::new()?;
            HandlerCLI::handle_delete_tasks(&mut tm, ids, done, yes)?;
        }
        Some(Command::Mark { ids, priority }) => {
            let ids = ids_or_exit(parse_id_args(&ids), "`rusk mark 1,2,3`");
            let mut tm = TaskManager::new()?;
            HandlerCLI::handle_mark_tasks(&mut tm, ids, priority)?;
        }
        Some(Command::Edit {
            ids,
            text,
            verbatim,
            date,
            after,
        }) => {
            let after = after.as_deref().map(|raw| after_ids_or_exit(raw, true));
            let args: Vec<String> = ids.into_iter().chain(text).collect();
            let (ids, text) = ids_or_exit(
                parse_edit_args(&args, &verbatim),
                "`rusk edit 1` (the editor), `rusk edit 1,2 new text`, `rusk edit 1 -- -x text`",
            );
            check_date_or_exit(date.as_deref());
            if let Some(words) = &text {
                check_text_or_exit(words);
            }

            if text.is_none() && date.is_none() && after.is_none() {
                #[cfg(not(feature = "interactive"))]
                exit_with_error(
                    "`rusk edit` without new text opens the editor, and this build has no \
                     'interactive' feature. Pass the new text on the command line, e.g. \
                     `rusk edit 1 buy oat milk`.",
                );
                #[cfg(feature = "interactive")]
                {
                    // The same check `rusk add` makes: without a terminal
                    // the editor would paint into a pipe (and die of a
                    // broken one) or into /dev/null, where nobody can see
                    // it and Esc looks like success — or, with no terminal
                    // to read keys from, fail to set one up.
                    if !HandlerCLI::on_a_terminal() {
                        exit_with_error(
                            "interactive `rusk edit` requires a terminal. \
                             Pass the new text on the command line, e.g. \
                             `rusk edit 1 buy oat milk`.",
                        );
                    }
                    if HandlerCLI::terminal_is_dumb() {
                        exit_with_error(
                            "the editor needs a terminal that can move the cursor, and TERM \
                             is `dumb`. Pass the new text on the command line, e.g. \
                             `rusk edit 1 buy oat milk`.",
                        );
                    }
                    let mut tm = TaskManager::new()?;
                    return HandlerCLI::handle_edit_tasks_interactive(&mut tm, ids);
                }
            }
            let mut tm = TaskManager::new()?;
            HandlerCLI::handle_edit_tasks(&mut tm, ids, text, date, after)?;
        }
        Some(Command::List {
            for_completion,
            for_completion_lines,
            compact,
            no_compact,
        }) => {
            let tm = TaskManager::new()?;
            if for_completion_lines {
                HandlerCLI::handle_list_tasks_for_completion(tm.tasks())?;
            } else if for_completion {
                HandlerCLI::handle_list_tasks_for_old_completion(tm.tasks())?;
            } else {
                // clap keeps only the last of `-c` and `--no-compact`.
                let compact = compact || (!no_compact && config::config().compact);
                HandlerCLI::handle_list_tasks(tm.tasks(), compact)?;
            }
        }
        Some(Command::Search { query, id }) => {
            let tm = TaskManager::new()?;
            HandlerCLI::handle_search_tasks(tm.tasks(), &query.join(" "), id)?;
        }
        None => {
            let tm = TaskManager::new()?;
            HandlerCLI::handle_list_tasks(tm.tasks(), config::config().compact)?;
        }
        Some(Command::Restore) => unreachable!("handled before the database is loaded"),
        #[cfg(feature = "web")]
        Some(Command::Gen { output }) => {
            let tm = TaskManager::new()?;
            if output != "-"
                && let Some(role) = tm
                    .local_path()
                    .and_then(|db| rusk::backend::database_file_role(db, std::path::Path::new(&output)))
            {
                exit_with_error(format!(
                    "refusing to write the page over {role} ('{output}'); \
                     name another file, e.g. `rusk gen -o index.html`"
                ));
            }
            let html = rusk::web::render_static_page(tm.tasks())?;
            if output == "-" {
                rusk::out!("{html}")?;
            } else {
                // Atomic replace where the location allows it: a web server
                // (or a failed write) never sees a truncated page.
                rusk::atomic::replace_output_file(std::path::Path::new(&output), html.as_bytes())
                    .map_err(|e| anyhow::anyhow!("Failed to write '{output}': {e}"))?;
                rusk::outln!(
                    "{} {} ({} tasks)",
                    config::theme().success.paint("Generated"),
                    output,
                    tm.tasks().len()
                )?;
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
fn handle_completions_install(mut shells: Vec<Shell>) -> Result<()> {
    // clap asks for one at least; a shell named twice is installed once.
    let mut seen = Vec::new();
    shells.retain(|shell| {
        let first = !seen.contains(shell);
        seen.push(*shell);
        first
    });

    let shells_count = shells.len();
    let mut installed_paths = Vec::new();

    // Said once the scripts are installed, so that an output that fails
    // cannot stop the installation half way; and said before a failure that
    // stops it, so that what did get installed is known.
    let report = |installed: &[(&Shell, std::path::PathBuf)]| -> Result<()> {
        for (shell, path) in installed {
            rusk::outln!(
                "{} {} {}",
                config::theme().success.paint("✓"),
                config::theme()
                    .success
                    .paint(&format!("{} completion installed to:", shell_name(shell))),
                path.display()
            )?;
        }
        Ok(())
    };
    for shell in &shells {
        let installed = shell.get_default_path().and_then(|path| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory '{}'", parent.display()))?;
            }
            std::fs::write(&path, shell.get_script())
                .with_context(|| format!("Failed to write completion file '{}'", path.display()))?;
            Ok(path)
        });
        match installed {
            Ok(path) => installed_paths.push((shell, path)),
            Err(e) => {
                report(&installed_paths).ok();
                return Err(e);
            }
        }
    }
    report(&installed_paths)?;

    if shells_count > 1 {
        rusk::outln!()?;
    }

    for (idx, (shell, path)) in installed_paths.iter().enumerate() {
        let instructions = shell.get_instructions(path);
        if shells_count > 1 {
            rusk::outln!(
                "{} {}:",
                config::theme().info.paint("Setup instructions for"),
                config::theme().info.paint(&shell_name(shell)).bold()
            )?;
        }
        rusk::outln!("{}", config::theme().info.paint(&instructions))?;
        if idx < installed_paths.len() - 1 {
            rusk::outln!()?;
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
    rusk::out!("{script}")
}
