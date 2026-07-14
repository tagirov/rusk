use clap::{Parser, Subcommand};

#[cfg(feature = "completions")]
use crate::completions::Shell;

pub const DATE_FORMAT_LONG_HELP: &str = "\
Date value for -d / --date (see `rusk add --help`):
  Absolute    DD-MM-YYYY (slashes ok; short year ok, e.g. 1-3-25).
  Words       today, tomorrow.
  Relative    Offset from today's local date. Chain segments with no spaces.
              Suffixes: d=days, w=weeks, m=months, q=quarters (3 months), y=years.
              Examples: 2d, 2w, 5m, 3q, 2y, 10d5w, 12d2q1y.
  Clear       Pass _ to remove the date from a task (e.g. -d _).
  Subcommand  Pass -h or --help as the date value for this command's help (e.g. -d -h).\n";

pub const EDIT_SUBCOMMAND_LONG_HELP: &str = "\
Interactive edit (`rusk edit <id>`) uses the TUI: the due date (if any) is only the \
first whitespace-delimited token at the start of the first line of the task text \
(absolute, relative from today, leading `+` on a relative offset from the task's \
current due date — today if none — or `_` to clear). A valid date token is highlighted in color; \
see Ctrl+G / F1 in the editor for the full date syntax. \
One-shot date or text+date without opening the TUI: `rusk edit <id> -d <date>` (same \
relative rules as in the TUI; `_` clears). Bare `-d` / `--date` (no value) is not \
supported. For new tasks, use `rusk add -d`.\n";

/// Root `--help` tail (after subcommands/options). Omits `completions` when that feature is off so
/// distro builds (`--no-default-features`) match the available CLI and static files in `completions/`.
#[cfg(feature = "completions")]
const CLI_ROOT_AFTER_LONG_HELP: &str = "Running `rusk` without a COMMAND is equivalent to `rusk list`. Use `rusk list -c` / `--compact` for a compact single-line view.\n\nDue dates: `rusk add -d ...` for new tasks, `rusk add` with no text for the TUI, or the interactive editor (`rusk edit <id>`) — first line at the start, see `rusk edit --help` and EDITOR.md. Pass `_` to clear where `-d` is supported. See `rusk add --help` for date syntax.\n\nConfiguration file (theme colors, defaults; see CONFIG.md): auto-created at ~/.config/rusk/cfg (Linux) or the platform config dir. Environment variables win over config values.\n\nEnvironment:\n  RUSK_DB           Optional database location: a file or directory path (the extension picks the\n                    format: .csv, .md, .txt, .ndjson, .ics, .db/.sqlite), https://host (rusk serve\n                    API) or user@host:/path (ssh).\n  RUSK_DB_TOKEN     Optional Bearer token for http(s) database locations.\n  RUSK_CONFIG       Optional path to the configuration file; empty value disables the config.\n  RUSK_NO_COLOR     Disable ANSI colors when set to any non-empty value (NO_COLOR is also respected).\n  RUSK_SYNC_REMOTE  Optional `rusk sync` remote (overrides sync_remote from the config).\n  RUSK_SYNC_TOKEN   Optional Bearer token for http(s) sync remotes.\n\nShell tab completion:\n  rusk completions install <shell> [<shell> ...]\n  rusk completions show <shell>\n";

#[cfg(not(feature = "completions"))]
const CLI_ROOT_AFTER_LONG_HELP: &str = "Running `rusk` without a COMMAND is equivalent to `rusk list`. Use `rusk list -c` / `--compact` for a compact single-line view.\n\nDue dates: `rusk add -d ...` for new tasks, `rusk add` with no text for the TUI, or the interactive editor (`rusk edit <id>`) — first line at the start, see `rusk edit --help` and EDITOR.md. Pass `_` to clear where `-d` is supported. See `rusk add --help` for date syntax.\n\nConfiguration file (theme colors, defaults; see CONFIG.md): auto-created at ~/.config/rusk/cfg (Linux) or the platform config dir. Environment variables win over config values.\n\nEnvironment:\n  RUSK_DB           Optional database location: a file or directory path (the extension picks the\n                    format: .csv, .md, .txt, .ndjson, .ics, .db/.sqlite), https://host (rusk serve\n                    API) or user@host:/path (ssh).\n  RUSK_DB_TOKEN     Optional Bearer token for http(s) database locations.\n  RUSK_CONFIG       Optional path to the configuration file; empty value disables the config.\n  RUSK_NO_COLOR     Disable ANSI colors when set to any non-empty value (NO_COLOR is also respected).\n  RUSK_SYNC_REMOTE  Optional `rusk sync` remote (overrides sync_remote from the config).\n  RUSK_SYNC_TOKEN   Optional Bearer token for http(s) sync remotes.\n";

#[derive(Parser)]
#[command(
    version,
    about,
    after_help = "Without COMMAND, lists all tasks (same as `rusk list`). Use `rusk list -c` for a compact single-line view.\n\nFor details on flags, dates, and environment variables run `rusk --help` or `rusk <COMMAND> --help`.",
    after_long_help = CLI_ROOT_AFTER_LONG_HELP
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(
        visible_alias = "a",
        about = "Add a new task (without TEXT opens the interactive editor)",
        long_about = "Add a new task. With TEXT: one-shot. Without TEXT: opens the interactive \
editor (set or clear a due date on the first line). Optional `-d` pre-seeds the first line \
when there is no TEXT.\n\n\
Examples:\n  \
rusk add buy groceries\n  \
rusk add report -d 31-12-2025\n  \
rusk add                          # interactive editor\n  \
rusk add -d 2w                    # editor with the date pre-seeded",
        help_template = "{about-section}\n\nUsage: rusk add [OPTIONS] [TEXT]...\n\n{all-args}\n\n{after-help}",
        after_long_help = DATE_FORMAT_LONG_HELP
    )]
    Add {
        #[arg(
            value_name = "TEXT",
            help = "Task text (one or more words). Omit to open the full-screen multi-line editor (requires a TTY; see EDITOR.md)"
        )]
        text: Vec<String>,
        #[arg(
            short,
            long,
            value_name = "DATE",
            allow_hyphen_values = true,
            help = "Due date: DD-MM-YYYY (slashes/dots ok, 1-7-25 ok), today/tomorrow, or relative from today (2d, 3q, 10d5w, …). See `rusk add --help` for full syntax. Pass `-d -h` for this command's help"
        )]
        date: Option<String>,
    },
    #[command(
        visible_alias = "d",
        about = "Delete tasks by ID, or all completed ones with --done",
        long_about = "Delete tasks by ID, or all completed ones with --done.\n\n\
Examples:\n  \
rusk del 3\n  \
rusk del 1,2,3\n  \
rusk del --done",
        help_template = "{about-section}\n\nUsage: rusk del [OPTIONS] [IDS]...\n\n{all-args}"
    )]
    Del {
        #[arg(
            trailing_var_arg = true,
            value_name = "IDS",
            help = "Task IDs: comma-separated (e.g. 1,2,3); without commas only the first ID is used"
        )]
        ids: Vec<String>,
        #[arg(long, help = "Delete all completed tasks (ignores IDS)")]
        done: bool,
    },
    #[command(
        visible_alias = "m",
        about = "Mark tasks as done/undone, or toggle priority with -p",
        long_about = "Toggle task completion by ID, or priority with -p (orange `p` instead of `•`).\n\n\
Examples:\n  \
rusk mark 3\n  \
rusk mark 1,2,3\n  \
rusk mark 1 -p"
    )]
    Mark {
        #[arg(
            short,
            long,
            help = "Toggle the priority flag instead of the done flag. Priority is preserved across done/undone toggles"
        )]
        priority: bool,
        #[arg(
            value_name = "IDS",
            help = "Task IDs: comma-separated (e.g. 1,2,3); without commas only the first ID is used"
        )]
        ids: Vec<String>,
    },
    #[command(
        visible_alias = "e",
        about = "Edit tasks by ID (without new text opens the interactive editor)",
        long_about = "Edit tasks by ID. Without new text, opens the interactive editor (set or \
clear a due date on the first line). With text, sets task text in one shot. Optional `-d <date>` \
(non-TUI) sets the due date.\n\n\
Examples:\n  \
rusk e 1                          # interactive editor\n  \
rusk e 1 -d 2w\n  \
rusk e 3 new text -d 15-06-2025\n  \
rusk e 1 -d _                     # clear the due date",
        help_template = "{about-section}\n\nUsage: rusk edit [ARGS]...\n\n{all-args}\n\n{after-help}",
        after_long_help = EDIT_SUBCOMMAND_LONG_HELP
    )]
    Edit {
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = false,
            value_name = "ARGS",
            help = "Task IDs (comma-separated) followed by optional new text. Without text, opens the interactive editor"
        )]
        args: Vec<String>,
    },
    #[command(
        visible_alias = "l",
        about = "List all tasks (same as running `rusk` without a command)",
        long_about = "List all tasks with status, ID, date, and text. Running `rusk` without a \
subcommand does the same. Use -c for a compact single-line view."
    )]
    List {
        #[arg(long, hide = true, default_value_t = false)]
        for_completion: bool,
        #[arg(
            short = 'c',
            long,
            help = "Compact view: show only the first line of each task (no wrap/paragraph continuations); strip trailing punctuation on that line"
        )]
        compact: bool,
    },
    #[command(
        visible_alias = "s",
        about = "Search tasks by text (case-insensitive) and highlight matches",
        long_about = "Search tasks whose text contains the given words (case-insensitive) and \
print them in the usual list format with matches highlighted. Multiple words are treated as \
one phrase. With --id, print only the IDs of matching tasks (one per line, no colors).\n\n\
Examples:\n  \
rusk search protein\n  \
rusk s buy groceries\n  \
rusk s --id protein",
        help_template = "{about-section}\n\nUsage: rusk search [OPTIONS] <QUERY>...\n\n{all-args}"
    )]
    Search {
        #[arg(
            required = true,
            value_name = "QUERY",
            help = "Text to search for (one or more words, matched as a phrase)"
        )]
        query: Vec<String>,
        #[arg(long, help = "Print only the IDs of matching tasks, one per line")]
        id: bool,
    },
    #[command(
        visible_alias = "r",
        about = "Restore task database from the automatic backup (.json.backup)"
    )]
    Restore,
    #[cfg(feature = "web")]
    #[command(
        visible_alias = "g",
        about = "Generate a read-only HTML page with all tasks",
        long_about = "Generate a self-contained read-only HTML page with all tasks \
(mobile-first, themed from the config).\n\n\
Examples:\n  \
rusk gen\n  \
rusk gen -o /var/www/tasks/index.html\n  \
rusk gen -o -                     # write to stdout"
    )]
    Gen {
        #[arg(
            short,
            long,
            value_name = "PATH",
            default_value = "index.html",
            help = "Output file path; pass `-` to write to stdout"
        )]
        output: String,
    },
    #[cfg(feature = "web")]
    #[command(
        about = "Serve the web UI with full task editing",
        long_about = "Serve the web UI with full task editing (mobile-first). Binds \
127.0.0.1:7272 by default; host/port/token come from the config (web_host, web_port, \
web_token).\n\n\
Examples:\n  \
rusk serve\n  \
rusk serve --port 8080\n  \
rusk serve --host 0.0.0.0"
    )]
    Serve {
        #[arg(
            long,
            value_name = "HOST",
            help = "Bind address (overrides web_host from the config; non-loopback requires web_token)"
        )]
        host: Option<String>,
        #[arg(
            long,
            value_name = "PORT",
            help = "Port (overrides web_port from the config; 0 picks a free port)"
        )]
        port: Option<u16>,
    },
    #[cfg(feature = "sync")]
    #[command(
        about = "Synchronize the task database with a remote",
        long_about = "Synchronize the task database with a remote: `sync_remote` in the config \
is either user@host:/path/tasks.json (ssh) or https://host (rusk serve API). Without a \
subcommand, fast-forwards in whichever direction changed and refuses when both sides \
changed.\n\n\
Examples:\n  \
rusk sync\n  \
rusk sync push\n  \
rusk sync pull --force"
    )]
    Sync {
        #[command(subcommand)]
        direction: Option<SyncDirection>,
    },
    #[cfg(feature = "completions")]
    #[command(
        visible_alias = "c",
        about = "Manage shell completions (bash, zsh, fish, nu, powershell)",
        long_about = "Manage shell completions (bash, zsh, fish, nu, powershell).\n\n\
Examples:\n  \
rusk completions install bash\n  \
rusk completions install fish nu\n  \
rusk completions show zsh"
    )]
    Completions {
        #[command(subcommand)]
        action: CompletionAction,
    },
}

#[cfg(feature = "sync")]
#[derive(Subcommand, Clone, Copy)]
pub enum SyncDirection {
    #[command(about = "Upload local tasks to the remote (refuses when the remote changed since the last sync)")]
    Push {
        #[arg(long, help = "Overwrite remote changes")]
        force: bool,
    },
    #[command(about = "Replace the local database with the remote tasks (refuses when local changed since the last sync)")]
    Pull {
        #[arg(long, help = "Discard local changes")]
        force: bool,
    },
}

#[cfg(feature = "completions")]
#[derive(Subcommand)]
pub enum CompletionAction {
    #[command(
        about = "Install completions for one or more shells (bash, zsh, fish, nu, powershell)"
    )]
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
