<h1 align="center" id="rusk">rusk</h1>
<p align="center">A minimal cross-platform terminal task manager</p>

<p align="center">
  <a href="https://github.com/tagirov/rusk/actions"><img src="https://img.shields.io/github/actions/workflow/status/tagirov/rusk/.github/workflows/rust.yml?logo=github-actions" alt="build"></a>&nbsp;
  <a href="https://github.com/tagirov/rusk/releases"><img src="https://img.shields.io/github/v/release/tagirov/rusk?logo=github" alt="release"></a>&nbsp;
  <a href="https://aur.archlinux.org/packages/rusk"><img src="https://img.shields.io/aur/version/rusk?logo=archlinux" alt="AUR Version"></a>
</p>

<p align="center">
  <img src="rusk.gif" alt="rusk demo" width="860">
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

# Leading zero for day and month is optional; a two-digit year is 20xx:
rusk add Finish project report --date 1-3-25

# The month may be a name (English, short or long):
rusk add Tax return --date 30-apr-26
rusk add Renew passport --date 1-September-2027

# Plain words work too:
rusk add Call mom --date today
rusk add Buy milk --date tomorrow

# Relative deadline from today (local date): chain number + suffix with no spaces.
# d=days, w=weeks, m=months, q=quarters (3 months), y=years. The months are added
# first (a shorter month ends the date at its last day: 31-01 + 1m = 28-02), then
# the days. Years run from 1000 to 9999.
rusk add Follow up --date 2w
rusk add Review --date 10d5w

# View all tasks. A new task that gets the id of a deleted one is listed in
# its place (in a hand-edited file with an order of your own: at the end)
rusk list

# or simply
rusk

# Compact view: one line per task (no wraps, trailing punctuation trimmed;
# `…` marks a task that goes on)
rusk list --compact

# Full view for one run, even with `compact = true` in the config
rusk list --no-compact

# Search tasks by text, matches highlighted. The words are one phrase: case
# does not matter (ΟΔΟΣ finds οδος, STRASSE finds Straße), nor does the
# whitespace between them (the list shows a run of spaces as one); accents do
# (cafe does not find café)
rusk search omega
rusk s buy milk

# Print only the IDs of matching tasks (script-friendly)
rusk s --id omega

# Mark a task as done (`mark` says what it changed: done, undone, priority
# or priority removed)
rusk mark 1

# Mark a task as undone (toggle)
rusk mark 1

# Mark a task as priority. Toggle again to remove.
rusk mark 1 --priority

# Add a task that depends on other tasks: shown as `deploy (19,22)` in the
# list. An ordering hint for agents and tooling (the task should be done no
# earlier than its dependencies); manual `rusk mark` is never blocked.
rusk add deploy --after 19,22
rusk a deploy -a 19,22

# Change or clear the dependency list of an existing task. The list is a
# set: the same ids in another order change nothing
rusk edit 1 --after 19,22
rusk edit 1 --after _

# Keywords: a task text starting with TEMP, INFO, FIXME or WIP gets that
# first word highlighted in the list. The set is configurable (`keywords`
# in the config; empty value disables), the color too (`keyword` theme key)
rusk add TEMP debug flag for the release
rusk add INFO deploy notes are in the wiki

# Edit task text in one shot
rusk edit 1 Complete the project documentation

# Optional flag for date without opening the TUI
rusk edit 1 --date 1-1-25
rusk edit 1 --date 2w

# Relative with leading +: add that offset to the task's current due date (today if it had none)
rusk edit 1 --date +2w

# Everything after `--` is text, word for word: words that start with a
# dash (before `--` they are options, and `-h` prints the help), or a text
# made of numbers only
rusk edit 1 -- -x means exclude
rusk edit 1 -- 42

# Delete a task: asks for confirmation on the terminal first. The question
# names the tasks that depend on it, and the deletion says which of them no
# longer do (`Task 3 no longer depends on 1.`)
rusk del 1

# Delete all completed tasks
rusk del --done

# Delete without asking. Without a terminal to ask on (a script, a pipe)
# `rusk del` refuses instead of guessing, and `--yes` is how to delete there
rusk del 1 --yes
rusk del --done -y

# Get help
rusk --help

# Help for a specific command
rusk add --help

# Or flag 
rusk add --date --help
```

## Working with Multiple Tasks

Multiple task IDs are one comma-separated list (`1,2,3`; `1, 2, 3` is read the
same way). Anything that is not an id in the list is an error, and a repeated
id counts once. For `mark` and `del` a second word after the list is an error
(`rusk mark 1 2`); for `edit` the first word that is not glued to the list by
a comma starts the new text (`rusk edit 3 1,000 units` edits task 3 only),
unless everything after the list is numbers: `rusk edit 1,2 3 -d 2w` changes
nothing and says so (a text of numbers goes after `--`).

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
# Generate a self-contained read-only HTML page with all tasks (never over
# the database or a file rusk keeps beside it)
rusk gen -o index.html

# Serve the interactive UI (full task editing from a phone)
rusk serve                  # http://127.0.0.1:7272
rusk serve --host 0.0.0.0   # requires web_token in the config
```

## Sync

Synchronize the database with your VPS over SSH, or with a running
`rusk serve` over HTTP(S). Conflicts are detected against what each side
held after the last sync — nothing is silently overwritten, a change made on
either side right after a sync is a change for the next one, and a first
sync into an empty database simply fills it. Details in
[WEB.md](WEB.md#rusk-sync)

```bash
# in the config: sync_remote = user@vps:/srv/tasks/tasks.json
rusk sync              # fast-forward in whichever direction changed
rusk sync push         # upload local tasks
rusk sync pull --force # discard local changes in favor of the remote
```

## Data Safety & Backup
#### Automatic Backups
- Every save first copies the previous state to a `.backup` sibling
  (`tasks.json.backup`), as private as the database itself (`backup = false`
  turns this off)
- Backups are stored in the same directory as your database — including an
  ssh database or sync remote, where the copy is made on the other side
  (`rusk restore` cannot reach it, and says where it is)
- Every file is replaced atomically: written to a uniquely named temp file,
  fsynced and renamed into place. A crash, a full disk or a parallel `rusk`
  leaves the old content or the new one, never a truncated file; the file
  mode (e.g. `0600`) and a symlinked database path survive the save
- An empty (zero-byte) database file is never copied over an existing backup
- A database that cannot be read is never taken for an empty one: no
  permission, a directory in its place, a symlink whose target is gone,
  content that is not UTF-8 — the command stops instead of starting from
  zero tasks and saving that over your data. The same over SSH. Only a file that is not there at all is a database yet to be
  created, and a file that holds nothing loads as zero tasks with a warning
  (the formats that write an empty database as an empty file — Markdown,
  todo.txt, NDJSON — excepted). A UTF-8 BOM in front of the file, as a
  Windows editor saves it, is not corruption

#### Concurrent Writers
- Commands running at the same time — two terminals, a cron job, `rusk serve`
  — all take effect: every change is applied to the database as it is at the
  moment of the save, never to the copy the command loaded when it started.
  Saves of a file database take turns on a writer lock, `tasks.json.lock`
  (an empty file that stays next to the database)
- A command that changes nothing writes nothing (no backup rotation either)
- `rusk edit` in the editor saves each task as soon as you press Ctrl+S. If
  another process changed or deleted that task while the editor was open,
  nothing is overwritten: the command stops and your text is kept as a
  draft, offered again by the next `rusk edit <id>` (if not even the draft
  can be written — a read-only directory — the text is printed with the
  error instead)
- `rusk del` deletes what you confirmed: a task that was changed, or deleted
  and its id reused, while the prompt was waiting is reported instead
- Anything else that writes the file (an editor, a sync tool) is noticed as
  well, but without the lock there is a window of a few milliseconds

#### Manual Restore
```bash
# Restore from the automatic backup
rusk restore

# This will:
# 1. Validate the backup file (an empty file, or one that is not a task
#    list, is refused)
# 2. Copy the current database, byte for byte, to `.before_restore` — or to
#    `.before_restore.1`, `.2`, … so an earlier copy is never overwritten
#    (a SQLite database that opens is copied by SQLite, WAL included)
# 3. Put the backup in place of the database: atomically and byte for byte
#    (with `git_backend` as a commit of its own); SQLite inside a transaction
```
`restore` shows when the backup was saved, and warns when it is much older
than the database (`backup = false`, or the file was edited by hand). For a
database synced with the configured remote it adds what the next `rusk sync`
will do, when the restored tasks are not what the last sync left: send them
there, as a change made here (`rusk sync pull --force` takes the remote's
back instead).


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
rusk s (search)
rusk c (completions)

# Global flags
-h (--help)
-V (--version)

# Command flags
-d (--date)
-a (--after)
-c (--compact)
   --no-compact
-p (--priority)
-y (--yes)

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
- Task text completion: `rusk edit <id><tab>` appends the task text for that ID, quoted so that running the line stores exactly that text: when it contains shell-special characters (``| ; & > < ( ) [ ] { } $ " ' \` * ? ~ # @ ! % ^ = + - / : ,``), line breaks, tabs, runs of spaces or spaces at either end, it is wrapped in single quotes (in nu a raw string `r#'…'#` if the text has `'`); a text that starts with `-` is put after `--`, so it is not read as options. In fish the id completes first and the next `<tab>` inserts the text, escaped by fish itself (see [completions/README.md](completions/README.md#fish))
- Flag completion: Autocomplete `--date` / `--after` (add, edit after an id), `--done`, etc.

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

When running in debug mode (`cargo run` or debug builds), Rusk uses a temporary database location to avoid affecting your production data — a directory of your own below the temp directory, since that directory is shared with every user of the machine:
- Linux/MacOS: `$TMPDIR/rusk-<uid>/debug/tasks.json` (usually `/tmp/rusk-1000/debug/tasks.json`)
- Windows: `%TEMP%\rusk-<user>\debug\tasks.json` (usually `C:\Users\<user>\AppData\Local\Temp\rusk-<user>\debug\tasks.json`)

On Unix `rusk-<uid>` is created so that only you can look inside (mode 700), and if something else is there under that name — a link, a file, a directory of another user's — rusk refuses to use it and says so: remove it, or point `TMPDIR` at a directory of your own. On Windows `%TEMP%` is yours already.

In debug mode, the `RUSK_DB` environment variable is ignored, and the database path is printed to the console when the program starts.

### Database Formats

The format is chosen by the last extension of the database file name (JSON by default; `notes.txt.json` is JSON):

```bash
export RUSK_DB="$HOME/tasks/tasks.csv"    # or rusk_db = ~/tasks/tasks.csv in the config
```

| Extension | Format | Interop | Feature |
|---|---|---|---|
| `.json` (or anything else) | pretty JSON | the default | built-in |
| `.csv` | RFC 4180, `id,text,date,done,priority,after` schema | LibreOffice / Excel / Google Sheets | built-in |
| `.md` / `.markdown` | GitHub-style task list | GitHub, Obsidian, any editor | `fmt-markdown` |
| `.txt` | [todo.txt](http://todotxt.org) | todo.sh, Simpletask, … | `fmt-todotxt` |
| `.ndjson` / `.jsonl` | one JSON task per line | git diffs, `grep`, `jq` | `fmt-ndjson` |
| `.ics` | iCalendar VTODO | Thunderbird, Nextcloud Tasks, Apple Reminders | `fmt-ics` |
| `.db` / `.sqlite` / `.sqlite3` | SQLite | concurrent writers, SQL tooling | `backend-sqlite` |

With SQLite, concurrent commands serialize on the database's own write lock:
each one re-reads the table inside its transaction when another process wrote
after it loaded, and applies its change to that (see
[Concurrent Writers](#concurrent-writers)). Reading never changes the file
(a write-protected database is listed fine); a database of an older rusk is
brought up to date by the first save, which also marks the file as rusk's in
its header (`application_id`, and the schema in `user_version`). A SQLite
file of another program — one with tables but no `tasks` table, or one its
header marks as another program's — is refused rather than written to, and so
is one a newer rusk wrote in a later schema. Deleted
tasks are overwritten in the file (`secure_delete`), and a save that frees
much of it compacts it.

All features except `backend-sqlite` (which bundles a C library) are enabled
by default; distro builds can trim them (`--no-default-features --features …`).

Notes on the text formats:

- **CSV**: multiline task text, commas and quotes are handled; dates are ISO
  `YYYY-MM-DD`; the `after` cell takes ids separated by spaces or commas.
  LibreOffice edits the file in place (it keeps ISO dates as they are);
  Google Sheets can import it, but edits made in Sheets have to be exported
  back manually. Excel opens it, but its "Save as CSV" may write the dates
  in the locale's format and separate the cells with `;` in locales that
  use it, and the file is then not a rusk database any more — and the file
  carries no byte order mark, so Excel may show non-ASCII text garbled
  unless the file is imported rather than opened (not verified in Excel
  itself). A row typed in with an empty id cell is a new task; rows of
  empty cells are skipped.
- **Markdown**: `- [x] text @2026-07-15 <!-- id:3 -->`, a leading `!` marks
  priority, continuation lines are indented by two spaces (or a tab). Items
  added by hand without the id comment get the lowest free id on the next
  run. Numbered items (`1. [ ] …`) are tasks too, and so are the other
  one-character checkbox marks of Obsidian and its themes: `[-]` (cancelled)
  reads as done, `[/]`, `[>]`, `[?]`, `[b]` and the rest as not done; every
  task is written back as `- [ ]` or `- [x]`. That is for items at the start
  of a line: an indented line is a continuation of the task above unless it
  is a `- [ ]`/`- [x]` item. rusk owns the file: headers and prose around
  the list are dropped on save. A text comes back as it was — a CRLF line
  break inside it comes back as LF, and a first line that is nothing but
  spaces and a date token loses the spaces — unless it looks like the
  markup: a first line that starts with `! ` or ends with ` @YYYY-MM-DD`, or
  a later line that is a `- [ ]` item.
- **todo.txt**: `x (A) text due:2026-07-15 id:3`; projects/contexts stay in
  the task text; newlines are stored as a literal `\n`. A word of the text
  that would read as metadata — `x`, `(B)` or a date first, a `due:`/`id:`/
  `after:` tag anywhere — is written with a `\` in front (`\x marks the
  spot`), so the text comes back as it was. rusk has one priority flag: any
  priority `(A)`–`(Z)` reads as priority and is written back as `(A)`, and
  completion and creation dates (`x 2026-07-09 2026-07-01 …`) are dropped.
- **iCalendar**: one VTODO per task; foreign components (VEVENT, VALARM),
  properties and relations are ignored and not preserved. `STATUS:COMPLETED`
  or `CANCELLED` is done — without a `STATUS`, so is a `COMPLETED` time or
  `PERCENT-COMPLETE:100`. A dependency is `RELATED-TO;RELTYPE=DEPENDS-ON`
  (or `FINISHTOSTART`); a `RELATED-TO` without `RELTYPE` is one only to a UID
  of the form rusk wrote up to 0.7.3 (`rusk-3@rusk`), otherwise it is the
  parent of a subtask. A save keeps the UID rusk gave each task and, while
  the task is unchanged, its `DTSTAMP`, so a calendar client sees only what
  changed as changed. A task new to the file gets a UID made of its id and
  its text (`rusk-3@<hash>.rusk`): a new task that got the id of a deleted
  one has a UID of its own — unless one save does both, as a `rusk sync`
  that brings both changes can: the file knows tasks by id only, and the new
  task is then an edit of the old one. A task that came with the UID of
  another client gets a rusk UID on the first save, which pins its id. A
  text has no carriage returns in iCalendar (a CRLF line break comes back as
  LF).

Every format can be edited by hand or by another tool; whatever rusk reads is
held to the same rules before anything uses it:

- A task needs a text. An item with neither text nor id — an empty `- [ ]`,
  a todo.txt line of metadata only, a VTODO without SUMMARY — is no task: it
  is skipped with a warning, and the next save leaves it out. A task that
  has an id but no text (`{"id": 2, "text": ""}`) keeps its date, flags and
  dependents and shows as `(no text)`, with a warning, so that `rusk edit 2`
  or `rusk del 2` can take care of it.
- Every task has an id of its own. One without an id (or with id 0) gets the
  lowest free id; one that repeats the id of an earlier task gets a new id,
  and a warning names it. `rusk list` shows the ids the next save writes.
- A dependency on a task that does not exist is dropped with a warning — so
  it can never come to mean the next task that gets that id; for the same
  reason `after` can only name ids the file already gives (an item added
  without an id has none until the next save). A dependency on the task
  itself, or one listed twice, is dropped too.
- A due date is given with a year from 1000 to 9999. One outside that — a
  typo an older rusk took (`1-1-205`) — is read as it is, shown with all
  its digits (`1-jan-0205`) and named in a warning, so that
  `rusk edit 2 -d <date>` or `-d _` can correct it. `rusk serve` does not
  take a whole list with one (`PUT`, as `rusk sync push` sends), as it takes
  no list a load would have to repair.
- In JSON only `text` is required: any other field may be left out or `null`.

The file itself changes only with the next save, and the `.backup` keeps what
that save replaced.

Backups and atomic writes work the same in every file format
(`tasks.csv.backup`); SQLite gets the same `.backup` copy per save, made by
SQLite itself (so it holds what a WAL holds too).

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
  reads and writes through the server. The list comes with a revision
  (`ETag`) and is saved back with `If-Match`, so a command never overwrites
  what another machine, the web UI or a cron script stored in the meantime:
  it fetches the list again and applies its change to that (both sides need
  a rusk with this feature; an older server is written unconditionally).
  Requires the network and the server to be up; for offline-first use
  `rusk sync` instead. A `user:password@` in the URL is basic authentication
  for a reverse proxy that asks for it; the password never shows in what
  rusk prints (curl gets it in a file, and reads no `~/.curlrc`: proxy
  settings come from the environment, a client certificate from
  `RUSK_CURL_CONFIG`, see [WEB.md](WEB.md#rusk-sync)). A `rusk serve` whose
  `rusk_db` names its own address — or a server whose database it is —
  says so on the first request instead of waiting on itself. An answer
  larger than 64 MiB — no task list is that big — is refused rather than
  read whole, over ssh as well.
- **ssh** reads/writes the remote file over the system `ssh` (keys, agent
  and `~/.ssh/config` apply); the location is `[user@]host:path` as `scp`
  reads it (`~/` is the remote home directory, IPv6 goes in brackets:
  `user@[::1]:/srv/tasks.json`); the remote extension picks the format
  (SQLite is local only), writes
  are atomic (temp + `mv`). Note this loads/saves per command — on flaky
  links prefer `rusk sync`. There is no lock on the remote side: after an
  editor or a prompt the file is read again and compared before it is
  replaced, which leaves a window of one round trip.
- **git_backend** gives full history (`git log`, `git revert`) beyond the
  single `.backup` copy. An existing enclosing repository is used as-is,
  with git's own identity for it (`rusk <rusk@localhost>` stands in only
  for what git has not got); otherwise a repo is initialized in the
  database directory with a `.gitignore` for the auxiliary files, and rusk
  says so. It is never initialized in your home directory, the filesystem
  root or a directory other users can write to (the temp directory), nor
  when git cannot use a repository it finds above (another owner's, for
  one) — that is reported instead. The auxiliary files (`.backup`, `.lock`,
  drafts, …) are kept out of `git status` through the repository's
  `.git/info/exclude`. No hook runs — neither a hook script of the
  repository nor a hook its config defines — and rusk does not run git in a
  repository other users can change (its config could run anything: a
  filter, `gpg.program`); in your own repository git runs with its config,
  as `git commit` there would. A symlinked database is committed where the
  file is. Works on a database file on this machine (not SQLite, not a
  remote one — rusk says so). Uses the system `git`, 2.9 or newer.

### Disabling Colors

Set `RUSK_NO_COLOR` to any non-empty value to disable ANSI colors in all output (dialogs, task list, errors, `--help` and argument errors too):

```bash
export RUSK_NO_COLOR=1
```

The standard `NO_COLOR` environment variable (see [no-color.org](https://no-color.org)) is also respected;
like `RUSK_NO_COLOR`, it counts only when it is not empty. The configuration file offers the same switch
as `no_color = true`. Once colors are off this way, nothing turns them back on for the run — not even
`CLICOLOR_FORCE`, which otherwise forces colors into a pipe; `CLICOLOR=0` turns them off as well.

### Output in Scripts

What `rusk` prints goes to a pipe as well as to a terminal. A reader that stops
reading (`rusk list | head -2`) ends the command quietly with exit code 0 —
whatever the command changed is saved before anything is printed — while an
output that cannot be written (a full disk) is an error, exit code 1.
Questions are only asked on a terminal (standard input and output both): `rusk
del` without one refuses and asks for `--yes`, and the editors (`rusk add`,
`rusk edit <id>`) need one too. `TERM=dumb` turns colors off like `NO_COLOR`,
unless `CLICOLOR_FORCE` is set.

<br />


<p align="center"><a href="#rusk">Back to top</a></p>
