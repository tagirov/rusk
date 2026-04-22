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
  - [Data Safety & Backup](#data-safety--backup)
    - [Automatic Backups](#automatic-backups)
    - [Manual Restore](#manual-restore)
  - [Aliases](#aliases)
- [Configuration](#configuration)
  - [Shell Completion](#shell-completion)
    - [Quick Install (Recommended)](completions/README.md#quick-install-recommended)
    - [Manual Installation](completions/README.md#manual-installation)
      - [Bash](completions/README.md#bash)
      - [Zsh](completions/README.md#zsh)
      - [Fish](completions/README.md#fish)
      - [Nu Shell](completions/README.md#nu-shell)
      - [PowerShell](completions/README.md#powershell)
  - [Database Location](#database-location)
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

# Relative deadline from today (local date): chain number + suffix with no spaces.
# d=days, w=weeks, m=months, q=quarters (3 months), y=years
rusk add Follow up --date 2w
rusk add Review --date 10d5w

# View all tasks
rusk list

# or simply
rusk

# Compact view: one line per task (no wraps, trailing punctuation trimmed)
rusk list -f
rusk list --first-line

# Mark a task as done
rusk mark 1

# Mark a task as undone (toggle)
rusk mark 1

# Mark a task as priority (shows orange `p` instead of `•`). Toggle again to remove.
rusk mark 1 -p

# Priority is preserved across done toggles: marking a priority task done, then
# marking it again, returns it to the priority state rather than to normal.
rusk mark 1 -p     # `•` → `p`
rusk mark 1        # `p` → `✔`
rusk mark 1        # `✔` → `p` (not `•`)

# Edit task text in one shot (optional -d for date without opening the TUI)
rusk edit 1 Complete the project documentation
rusk edit 1 -d 2w

# Delete a task
rusk del 1

# Delete all completed tasks
rusk del --done

# Get help
rusk --help

# Help for a specific command
rusk add --help
rusk edit --help

# Date flag help (rusk add -d; rusk edit -d with a value, see rusk edit --help)
rusk add -d -h
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
[EDITOR.md](EDITOR.md).

```bash
# Edit task text and optional due date on the first line (see Ctrl+G in the editor).
rusk edit 1

# Edit several tasks in one session.
rusk edit 1,2,3
```

Set or change the due date **only on the first line, at the very start** (first
token): absolute `DD-MM-YYYY` / `DD/MM/YYYY` (short year ok) or relative (`2w`,
`10d5w`, …). A **valid** token is **highlighted in color** on that line; invalid
text is not. Use `_` alone as the first token to clear. For non-interactive date
changes use `rusk edit <id> -d <date>` (same values as `rusk add`); **bare** `-d` /
`--date` (no value) is not allowed — use the TUI to edit the first line. Quick keys: `Ctrl+S` save, `Esc`
cancel (confirms if dirty), `Ctrl+G` or `F1` in-editor help (full date syntax),
`Ctrl+R` restore original text, `Ctrl+Z` / `Ctrl+Y` undo / redo, `Ctrl+C` / `Ctrl+X` /
`Ctrl+V` copy / cut / paste, `Ctrl+A` select all. See [EDITOR.md](EDITOR.md) for the
complete list, mouse gestures, dirty-state confirmation, and draft recovery behaviour.


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
rusk c (completions)

# Global flags
-h (--help)
-V (--version)

# Command flags
-d (--date)          # add; optional on `edit` with a value (e.g. -d 2w, -d _); bare -d is invalid; TUI: first line
-f (--first-line)    # list
-p (--priority)      # mark
   --done            # del (no short form)
```

# Configuration

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
You can customize the database location using the `RUSK_DB` environment variable:

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

### Disabling Colors

Set `RUSK_NO_COLOR` to any non-empty value to disable ANSI colors in all output (dialogs, task list, errors):

```bash
export RUSK_NO_COLOR=1
```

The standard `NO_COLOR` environment variable (see [no-color.org](https://no-color.org)) is also respected.

<br />


<p align="center"><a href="#rusk">Back to top</a></p>
