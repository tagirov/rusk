use clap::{Parser, Subcommand};

#[cfg(feature = "completions")]
use crate::completions::Shell;

pub const DATE_FORMAT_LONG_HELP: &str = "\
Date value for -d / --date (and interactive date entry when `edit … --date` with no value):
  Absolute    DD-MM-YYYY (slashes ok; short year ok, e.g. 1-3-25).
  Relative    Offset from today's local date. Chain segments with no spaces.
              Suffixes: d=days, w=weeks, m=months, q=quarters (3 months), y=years.
              Examples: 2d, 2w, 5m, 3q, 2y, 10d5w, 12d2q1y.
  Clear       Pass _ to remove the date from a task (e.g. -d _).
  Subcommand  Pass -h or --help as the date value for this command's help (e.g. -d -h).\n";

#[derive(Parser)]
#[command(
    version,
    about,
    after_help = "Without COMMAND, lists all tasks (same as `rusk list`).\n\nEnvironment:\n  RUSK_DB           Optional path to the tasks database file or directory.\n  RUSK_NO_COLOR      Disable ANSI colors when set to any non-empty value (NO_COLOR is also respected).\n\nShell tab completion:\n  rusk completions install <shell> [<shell> ...]\n  rusk completions show <shell>",
    after_long_help = "Due dates: calendar form (DD-MM-YYYY) or relative (e.g. 2w, 10d5w) on add/edit. See `rusk add --help` for the full date syntax.\n\nEnvironment:\n  RUSK_DB           Optional path to the tasks database file or directory.\n  RUSK_NO_COLOR      Disable ANSI colors when set to any non-empty value (NO_COLOR is also respected).\n"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
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
        #[arg(trailing_var_arg = true, value_name = "IDS", help = "Task IDs: comma-separated (e.g. 1,2,3); without commas only the first ID is used")]
        ids: Vec<String>,
        #[arg(long, help = "Delete all completed tasks")]
        done: bool,
    },
    #[command(
        visible_alias = "m",
        about = "Toggle task completion status by ID. With -p toggles priority (shown as orange `p` instead of `•`). Supports multiple IDs and comma-separated input. Examples: rusk mark 3, rusk mark 1,2,3, rusk mark 1 -p"
    )]
    Mark {
        #[arg(short, long, help = "Toggle the priority flag instead of the done flag. Priority is preserved when the task is toggled done/undone.")]
        priority: bool,
        #[arg(value_name = "IDS", help = "Task IDs: comma-separated (e.g. 1,2,3); without commas only the first ID is used")]
        ids: Vec<String>,
    },
    #[command(
        visible_alias = "e",
        about = "Edit tasks by ID. Without text: `rusk edit 1` starts interactive text edit; `rusk edit 1 --date` edits text and date interactively (same date formats as -d, including relative). Examples: rusk e 3 new text -d 01-11-2025, rusk e 1 -d 2w",
        help_template = "{about-section}\n\nUsage: rusk edit [ARGS]... [OPTIONS]\n\n{all-args}\n\n{after-help}",
        after_long_help = DATE_FORMAT_LONG_HELP
    )]
    Edit {
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
        #[arg(
            short = 'f',
            long,
            help = "Show only the first line of each task (no wrap/paragraph continuations); strip trailing punctuation on that line"
        )]
        first_line: bool,
    },
    #[command(
        visible_alias = "r",
        about = "Restore task database from backup (.json.backup)"
    )]
    Restore,
    #[cfg(feature = "completions")]
    #[command(
        visible_alias = "c",
        about = "Manage shell completions. Example: rusk completions install bash, or rusk completions install fish nu"
    )]
    Completions {
        #[command(subcommand)]
        action: CompletionAction,
    },
}

#[cfg(feature = "completions")]
#[derive(Subcommand)]
pub enum CompletionAction {
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
