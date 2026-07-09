<h1 align="center" id="rusk">rusk</h1>
<p align="center">A minimal cross-platform terminal task manager</p>

<p align="center">
  <a href="https://github.com/tagirov/rusk/actions"><img src="https://img.shields.io/github/actions/workflow/status/tagirov/rusk/.github/workflows/rust.yml?logo=github-actions" alt="build"></a>&nbsp;
  <a href="https://github.com/tagirov/rusk/releases"><img src="https://img.shields.io/github/v/release/tagirov/rusk?logo=github" alt="release"></a>&nbsp;
  <a href="https://aur.archlinux.org/packages/rusk"><img src="https://img.shields.io/aur/version/rusk?logo=archlinux" alt="AUR Version"></a>
</p>

<br />


- [Install](#install)
- [Usage](#usage)
  - [Working with Multiple Tasks](#working-with-multiple-tasks)
  - [Interactive Editor](#interactive-editor)
  - [Web UI](#web-ui)
  - [Sync](#sync)
  - [Data Safety & Backup](#data-safety--backup)
    - [Automatic Backups](#automatic-backups)
    - [Manual Restore](#manual-restore)
  - [Aliases](#aliases)
- [Configuration](#configuration)
  - [Configuration File](#configuration-file)
  - [Shell Completion](#shell-completion)
    - [Quick Install (Recommended)](completions/README.md#quick-install-recommended)
    - [Manual Installation](completions/README.md#manual-installation)
      - [Bash](completions/README.md#bash)
      - [Zsh](completions/README.md#zsh)
      - [Fish](completions/README.md#fish)
      - [Nu Shell](completions/README.md#nu-shell)
      - [PowerShell](completions/README.md#powershell)
  - [Database Location](#database-location)
  - [Database Formats](#database-formats)
  - [Disabling Colors](#disabling-colors)

# Install
#### Linux/MacOS/Windows
```bash
cargo install --git https://github.com/tagirov/rusk
```
The binary will be installed to:
- Linux/MacOS: `$HOME/.cargo/bin/rusk`
- Windows: `%USERPROFILE%\.cargo\bin\rusk.exe`

Make sure that these paths are added to your $PATH environment variable to use `rusk` command globally.

#### Arch Linux (AUR)
```bash
yay -S rusk
```

#### Manually
```bash
git clone https://github.com/tagirov/rusk && cd rusk
```
```bash
cargo build --release
```

Linux/MacOS

```bash
sudo install -m 755 ./target/release/rusk /usr/local/bin
```

Windows

```bash
copy .\target\release\rusk.exe "%USERPROFILE%\AppData\Local\Microsoft\WindowsApps\"
```



# Usage

```bash
# Add a new task
rusk add Buy groceries

# Add a task with a deadline
rusk add Finish project report --date 31-12-2025

# Or with short year; slash, hyphen, or dot between parts:
rusk add Finish project report --date 31/12/25
rusk add Finish project report --date 31.12.25

# Leading zero for day and month is optional:
rusk add Finish project report --date 1-3-25

# Plain words work too:
rusk add Call mom --date today
rusk add Buy milk --date tomorrow

# Relative deadline from today (local date): chain number + suffix with no spaces.
# d=days, w=weeks, m=months, q=quarters (3 months), y=years
rusk add Follow up --date 2w
rusk add Review --date 10d5w

# View all tasks
rusk list

# or simply
rusk

# Compact view: one line per task (no wraps, trailing punctuation trimmed)
rusk list --compact

# Mark a task as done
rusk mark 1

# Mark a task as undone (toggle)
rusk mark 1

# Mark a task as priority. Toggle again to remove.
rusk mark 1 --priority

# Edit task text in one shot
rusk edit 1 Complete the project documentation

# Optional flag for date without opening the TUI
rusk edit 1 --date 1-1-25
rusk edit 1 --date 2w

# Relative with leading +: add that offset to the task's current due date (today if it had none)
rusk edit 1 --date +2w

# Delete a task
rusk del 1

# Delete all completed tasks
rusk del --done

# Get help
rusk --help

# Help for a specific command
rusk add --help

# Or flag 
rusk add --date --help
```

## Working with Multiple Tasks

Multiple task IDs must be comma-separated (no spaces allowed between IDs)

```bash
# Mark multiple tasks as done
rusk mark 1,2,3

# Edit multiple tasks with the same text
rusk edit 1,2,3 Update status to completed

# Delete multiple tasks
rusk del 1,2,3
```

## Interactive Editor

The interactive multi-line editor supports selection, system clipboard,
undo/redo, word navigation, mouse, crash-safe autosave, and a colored date
header on the first line. Its full reference lives in
[EDITOR.md](EDITOR.md)

```bash
# Create a task using the TUI
rusk add

# Optional -d / --date pre-fills the first line
rusk add -d 2w

# Edit task text and optional due date on the first line.
rusk edit 1

# Edit several tasks in one session.
rusk edit 1,2,3
```

## Web UI

A mobile-first web frontend, themed from your [configuration file](CONFIG.md)
and built from a single embedded template — no separate frontend to install.
Full guide (auth, VPS deployment behind Caddy/nginx, API): [WEB.md](WEB.md)

```bash
# Generate a self-contained read-only HTML page with all tasks
rusk gen -o index.html

# Serve the interactive UI (full task editing from a phone)
rusk serve                  # http://127.0.0.1:7272
rusk serve --host 0.0.0.0   # requires web_token in the config
```

## Sync

Synchronize the database with your VPS over SSH, or with a running
`rusk serve` over HTTP(S). Conflicts are detected via the hash of the last
synced state — nothing is silently overwritten. Details in
[WEB.md](WEB.md#rusk-sync)

```bash
# in the config: sync_remote = user@vps:/srv/tasks/tasks.json
rusk sync              # fast-forward in whichever direction changed
rusk sync push         # upload local tasks
rusk sync pull --force # discard local changes in favor of the remote
```

## Data Safety & Backup
#### Automatic Backups
- Every save operation creates a `.json.backup` file
- Backups are stored in the same directory as your database
- Atomic writes prevent data corruption during saves

#### Manual Restore
```bash
# Restore from the automatic backup
rusk restore

# This will:
# 1. Validate the backup file
# 2. Create a safety backup of current database (if valid)
# 3. Restore tasks from backup
```


## Aliases
```bash
# Subcommand aliases
rusk a (add)
rusk l (list)
rusk m (mark)
rusk e (edit)
rusk d (del)
rusk r (restore)
rusk g (gen)
rusk s (serve)
rusk c (completions)

# Global flags
-h (--help)
-V (--version)

# Command flags
-d (--date)
-c (--compact)
-p (--priority)

```

# Configuration

### Configuration File

rusk auto-creates a configuration file on first run (`~/.config/rusk/cfg` on
Linux; see [CONFIG.md](CONFIG.md) for other platforms and the full
reference). It controls theme colors for every element group, default
behavior, and the web/sync settings:

```
# variables are supported: any unknown key defines one
accent = #ffa500
priority_marker = accent

# lowercase analogs of the environment variables (env always wins)
# rusk_db = ~/tasks/tasks.json
# no_color = false
# compact = true

# web & sync
# web_token = your-secret
# sync_remote = user@vps:/srv/tasks/tasks.json
```

Colors are ANSI names (`red`, `cyan`, `bright_black`, ...) or hex
(`#d75f00`). Broken lines never prevent rusk from starting — they produce
warnings and fall back to defaults. `RUSK_CONFIG=<path>` overrides the file
location; an empty `RUSK_CONFIG` disables the config entirely.

### Shell Completion

> For installation instructions, see [completions/README.md](completions/README.md).

It provides autocomplete for commands and task text during editing by pressing `<tab>` button.


**Features**
- Command completion: `add`, `edit`, `mark`, `del`, `completions`, etc. and their aliases
- Task text completion: `rusk edit <id><tab>` appends the task text for that ID. If the text contains shell-special characters  (``| ; & > < ( ) [ ] { } $ " ' \` * ? ~ # @ ! % ^ = + - / : ,``), it is automatically wrapped in single quotes (double quotes if the text has `'`)
- Flag completion: Autocomplete `--date` (add), `--done`, etc.; `edit` offers help flags only

**Windows Support**
- Git Bash: Works with `bash` completions (uses Unix-style paths)
- WSL: Works with `bash`, `zsh`, `fish`, and `nu` completions
- Nu Shell: Works natively on Windows (uses `%APPDATA%\nushell\completions\`)
- PowerShell: Works natively on Windows (uses `Documents\PowerShell\rusk-completions.ps1`)
- CMD: Basic commands work (add, list, mark, del, edit with text). Due dates: interactive editor (`rusk edit` without argv text) on the first line, or `rusk edit <id> -d <date>`. Interactive editing requires Windows 10+ and may have limited functionality. Tab completion is not supported. Colors work on Windows 10+ (build 1511 and later)

### Database Location

By default, Rusk stores tasks to: `./.rusk/tasks.json`

```bash
# Use different task lists for different projects
cd ~/projects/website
rusk add Fix responsive layout

cd ~/projects/api
rusk add Add authentication endpoint

# Each project has its own task list because Rusk uses a relative default database path
```
You can customize the database location using the `RUSK_DB` environment variable
or the `rusk_db` key in the [configuration file](CONFIG.md) (`RUSK_DB` wins):

```bash
# Use a custom database file
export RUSK_DB="/path/to/your/db.json"

# Use a custom directory (tasks.json will be created inside)
export RUSK_DB="/path/to/your/project/"
```

**Debug Mode**

When running in debug mode (`cargo run` or debug builds), Rusk uses a temporary database location to avoid affecting your production data:
- Linux/MacOS: `$TMPDIR/rusk_debug/tasks.json` (usually `/tmp/rusk_debug/tasks.json`)
- Windows: `%TEMP%\rusk_debug\tasks.json` (usually `C:\Users\<user>\AppData\Local\Temp\rusk_debug\tasks.json`)

In debug mode, the `RUSK_DB` environment variable is ignored, and the database path is printed to the console when the program starts.

### Database Formats

The format is chosen by the database file extension (JSON by default):

```bash
export RUSK_DB="$HOME/tasks/tasks.csv"    # or rusk_db = ~/tasks/tasks.csv in the config
```

| Extension | Format | Interop | Feature |
|---|---|---|---|
| `.json` (or anything else) | pretty JSON | the default | built-in |
| `.csv` | RFC 4180, `id,text,date,done,priority` schema | LibreOffice / Excel / Google Sheets | built-in |
| `.md` / `.markdown` | GitHub-style task list | GitHub, Obsidian, any editor | `fmt-markdown` |
| `.txt` | [todo.txt](http://todotxt.org) | todo.sh, Simpletask, … | `fmt-todotxt` |
| `.ndjson` / `.jsonl` | one JSON task per line | git diffs, `grep`, `jq` | `fmt-ndjson` |
| `.ics` | iCalendar VTODO | Thunderbird, Nextcloud Tasks, Apple Reminders | `fmt-ics` |
| `.db` / `.sqlite` / `.sqlite3` | SQLite | concurrent writers, SQL tooling | `backend-sqlite` |

All features except `backend-sqlite` (which bundles a C library) are enabled
by default; distro builds can trim them (`--no-default-features --features …`).

Notes on the text formats:

- **CSV**: multiline task text, commas and quotes are handled; dates are ISO
  `YYYY-MM-DD`. LibreOffice/Excel edit the file in place; Google Sheets can
  import it, but edits made in Sheets have to be exported back manually.
- **Markdown**: `- [x] text @2026-07-15 <!-- id:3 -->`, a leading `!` marks
  priority, continuation lines are indented by two spaces. Items added by
  hand without the id comment get the lowest free id on the next run. rusk
  owns the file: headers and prose around the list are dropped on save.
- **todo.txt**: `x (A) text due:2026-07-15 id:3`; projects/contexts stay in
  the task text; newlines are stored as a literal `\n`.
- **iCalendar**: one VTODO per task; foreign components (VEVENT, VALARM) are
  ignored and not preserved.

Backups and atomic writes work the same in every file format
(`tasks.csv.backup`); SQLite gets the same `.backup` copy per save.

### Remote and git-backed Databases

The database does not have to be a local file — the *shape* of `rusk_db`
picks the storage backend:

```bash
rusk_db = https://tasks.example.com     # the API of a running `rusk serve` (feature backend-http)
db_token = s3cret                       # its web_token, if set (or RUSK_DB_TOKEN)

rusk_db = alex@vps:/srv/tasks/tasks.md  # a file over ssh (feature backend-ssh)

git_backend = true                      # commit every save of a local file database
                                        # to a git repo in its directory (feature backend-git)
```

- **http(s)** is a thin client: there is no local copy at all, every command
  reads and writes through the server, so concurrent writers (another
  machine, the web UI, cron scripts) never diverge. Requires the network and
  the server to be up; for offline-first use `rusk sync` instead.
- **ssh** reads/writes the remote file over the system `ssh` (keys, agent
  and `~/.ssh/config` apply); the remote extension picks the format, writes
  are atomic (temp + `mv`). Note this loads/saves per command — on flaky
  links prefer `rusk sync`.
- **git_backend** gives full history (`git log`, `git revert`) beyond the
  single `.backup` copy. An existing enclosing repository is used as-is;
  otherwise a repo is initialized in the database directory with a
  `.gitignore` for the auxiliary files. Uses the system `git`.

### Disabling Colors

Set `RUSK_NO_COLOR` to any non-empty value to disable ANSI colors in all output (dialogs, task list, errors):

```bash
export RUSK_NO_COLOR=1
```

The standard `NO_COLOR` environment variable (see [no-color.org](https://no-color.org)) is also respected.
The configuration file offers the same switch as `no_color = true` (the
environment variables win over the config).

<br />


<p align="center"><a href="#rusk">Back to top</a></p>
