use clap::{Parser, Subcommand};

#[cfg(feature = "completions")]
use crate::completions::Shell;

pub const DATE_FORMAT_LONG_HELP: &str = "\
Date value for -d / --date (see `rusk add --help`):
  Absolute    DD-MM-YYYY (slashes ok; short year ok, e.g. 1-3-25).
  Relative    Offset from today's local date. Chain segments with no spaces.
              Suffixes: d=days, w=weeks, m=months, q=quarters (3 months), y=years.
              Examples: 2d, 2w, 5m, 3q, 2y, 10d5w, 12d2q1y.
  Clear       Pass _ to remove the date from a task (e.g. -d _).
  Subcommand  Pass -h or --help as the date value for this command's help (e.g. -d -h).\n";

pub const EDIT_SUBCOMMAND_LONG_HELP: &str = "\
Interactive edit (`rusk edit <id>`) uses the TUI: the due date (if any) is only the \
first whitespace-delimited token at the start of the first line of the task text \
(absolute, relative, or `_` to clear). A valid date token is highlighted in color; \
see Ctrl+G / F1 in the editor for the full date syntax. \
One-shot date or text+date without opening the TUI: `rusk edit <id> -d <date>` (same \
values as `rusk add -d`; `_` clears). Bare `-d` / `--date` (no value) is not \
supported. For new tasks, use `rusk add -d`.\n";

#[derive(Parser)]
#[command(
    version,
    about,
    after_help = "Without COMMAND, lists all tasks (same as `rusk list`). Use `rusk list -f` for a compact single-line view.\n\nFor details on flags, dates, and environment variables run `rusk --help` or `rusk <COMMAND> --help`.",
    after_long_help = "Running `rusk` without a COMMAND is equivalent to `rusk list`. Use `rusk list -f` / `--first-line` for a compact single-line view.\n\nDue dates: use `rusk add -d ...` for new tasks, or the interactive editor (`rusk edit <id>`) — first line at the start, see `rusk edit --help`. Pass `_` to clear where `-d` is supported. See `rusk add --help` for date syntax.\n\nEnvironment:\n  RUSK_DB        Optional path to the tasks database file or directory.\n  RUSK_NO_COLOR  Disable ANSI colors when set to any non-empty value (NO_COLOR is also respected).\n\nShell tab completion:\n  rusk completions install <shell> [<shell> ...]\n  rusk completions show <shell>\n"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(
        visible_alias = "a",
        about = "Add a new task. Examples: rusk add buy groceries; rusk add report -d 31-12-2025; rusk add follow up -d 2w",
        help_template = "{about-section}\n\nUsage: rusk add [OPTIONS] [TEXT]...\n\n{all-args}\n\n{after-help}",
        after_long_help = DATE_FORMAT_LONG_HELP
    )]
    Add {
        #[arg(value_name = "TEXT", help = "Task text (one or more words)")]
        text: Vec<String>,
        #[arg(short, long, value_name = "DATE", allow_hyphen_values = true, help = "Due date: DD-MM-YYYY (slashes/dots ok, 1-7-25 ok), or relative from today (2d, 3q, 10d5w, …). See `rusk add --help` for full syntax. Pass `-d -h` for this command's help")]
        date: Option<String>,
    },
    #[command(
        visible_alias = "d",
        about = "Delete tasks by ID, or all completed ones with --done. Examples: rusk del 3; rusk del 1,2,3; rusk del --done",
        help_template = "{about-section}\n\nUsage: rusk del [OPTIONS] [IDS]...\n\n{all-args}"
    )]
    Del {
        #[arg(trailing_var_arg = true, value_name = "IDS", help = "Task IDs: comma-separated (e.g. 1,2,3); without commas only the first ID is used")]
        ids: Vec<String>,
        #[arg(long, help = "Delete all completed tasks (ignores IDS)")]
        done: bool,
    },
    #[command(
        visible_alias = "m",
        about = "Toggle task completion by ID, or priority with -p (orange `p` instead of `•`). Examples: rusk mark 3; rusk mark 1,2,3; rusk mark 1 -p"
    )]
    Mark {
        #[arg(short, long, help = "Toggle the priority flag instead of the done flag. Priority is preserved across done/undone toggles")]
        priority: bool,
        #[arg(value_name = "IDS", help = "Task IDs: comma-separated (e.g. 1,2,3); without commas only the first ID is used")]
        ids: Vec<String>,
    },
    #[command(
        visible_alias = "e",
        about = "Edit tasks by ID. Without new text, opens the interactive editor (set or clear a due date on the first line). With text, sets task text in one shot. Optional `-d <date>` (non-TUI) sets the due date. Examples: rusk e 1; rusk e 1 -d 2w; rusk e 3 new text -d 15-06-2025; rusk e 1 -d _",
        help_template = "{about-section}\n\nUsage: rusk edit [ARGS]...\n\n{all-args}\n\n{after-help}",
        after_long_help = EDIT_SUBCOMMAND_LONG_HELP
    )]
    Edit {
        #[arg(trailing_var_arg = true, allow_hyphen_values = false, value_name = "ARGS", help = "Task IDs (comma-separated) followed by optional new text. Without text, opens the interactive editor")]
        args: Vec<String>,
    },
    #[command(
        visible_alias = "l",
        about = "List all tasks with status, ID, date, and text. Running `rusk` without a subcommand does the same. Use -f for a compact single-line view"
    )]
    List {
        #[arg(long, hide = true, default_value_t = false)]
        for_completion: bool,
        #[arg(
            short = 'f',
            long,
            help = "Compact view: show only the first line of each task (no wrap/paragraph continuations); strip trailing punctuation on that line"
        )]
        first_line: bool,
    },
    #[command(
        visible_alias = "r",
        about = "Restore task database from the automatic backup (.json.backup)"
    )]
    Restore,
    #[cfg(feature = "completions")]
    #[command(
        visible_alias = "c",
        about = "Manage shell completions. Examples: rusk completions install bash; rusk completions install fish nu; rusk completions show zsh"
    )]
    Completions {
        #[command(subcommand)]
        action: CompletionAction,
    },
}

#[cfg(feature = "completions")]
#[derive(Subcommand)]
pub enum CompletionAction {
    #[command(about = "Install completions for one or more shells (bash, zsh, fish, nu, powershell)")]
    Install {
        #[arg(value_enum, required = true, num_args = 1.., value_name = "SHELL", help = "One or more target shells")]
        shells: Vec<Shell>,
    },
    #[command(about = "Print completion script to stdout (for manual installation)")]
    Show {
        #[arg(value_enum, value_name = "SHELL", help = "Target shell")]
        shell: Shell,
    },
}
