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

- One binary for Linux, macOS and Windows.
- Due dates, priorities, dependencies between tasks and a phrase search.
- An interactive multi-line editor with undo, clipboard and crash-safe drafts.
- A mobile-first web UI, a static HTML export and sync over SSH or HTTPS.
- The database is a file in the format you pick: JSON, CSV, Markdown,
  todo.txt, NDJSON, iCalendar or SQLite. Or a remote one, or a git repository.
- Atomic saves, a backup on every save and commands that can run at the same time.
- Tab completion for bash, zsh, fish, nu and PowerShell.

<br />

- [Install](#install)
- [Quick start](#quick-start)
- [Usage](#usage)
  - [Due dates](#due-dates)
  - [Listing and search](#listing-and-search)
  - [Marking](#marking)
  - [Editing](#editing)
  - [Dependencies and keywords](#dependencies-and-keywords)
  - [Deleting](#deleting)
  - [Several tasks at once](#several-tasks-at-once)
  - [Interactive editor](#interactive-editor)
  - [Aliases](#aliases)
- [Web UI and sync](#web-ui-and-sync)
- [Storage](#storage)
  - [Database location](#database-location)
  - [Database formats](#database-formats)
  - [Remote and git-backed databases](#remote-and-git-backed-databases)
  - [Data safety](#data-safety)
- [Configuration](#configuration)
  - [Configuration file](#configuration-file)
  - [Shell completion](#shell-completion)
  - [Colors](#colors)
  - [Scripts and pipes](#scripts-and-pipes)
- [Documentation](#documentation)

## Install

### Cargo (Linux, macOS, Windows)

```bash
cargo install --git https://github.com/tagirov/rusk
```

The binary lands in `$HOME/.cargo/bin/rusk` (Windows:
`%USERPROFILE%\.cargo\bin\rusk.exe`). Make sure that directory is on your
`PATH`.

### Arch Linux (AUR)

```bash
yay -S rusk
```

### Nix (flake)

```bash
nix run github:tagirov/rusk             # try it without installing
nix profile install github:tagirov/rusk # bash, zsh and fish completions come with it
```

### From source

```bash
git clone https://github.com/tagirov/rusk && cd rusk
cargo build --release
```

Linux/macOS:

```bash
sudo install -m 755 ./target/release/rusk /usr/local/bin
```

Windows:

```bash
copy .\target\release\rusk.exe "%USERPROFILE%\AppData\Local\Microsoft\WindowsApps\"
```

## Quick start

```bash
rusk add Buy groceries                  # add a task
rusk add Finish the report -d 31-12-25  # with a due date
rusk                                    # list the tasks (same as `rusk list`)
rusk mark 1                             # done; run it again to undo
rusk edit 2 Finish the quarterly report # new text
rusk del 1                              # asks for confirmation first
rusk --help                             # help; `rusk add --help` for one command
```

## Usage

### Due dates

`-d` / `--date` takes a date in one of these forms:

| Form | Examples | Notes |
|---|---|---|
| Absolute | `31-12-2025`, `31/12/25`, `1.3.25` | day-month-year with `-`, `/` or `.` between the parts. Leading zeros are optional; a two-digit year is 20xx |
| Month name | `30-apr-26`, `1-September-2027` | English, short or long |
| Words | `today`, `tomorrow` | |
| Relative | `2w`, `3q`, `10d5w` | an offset from today: `d` days, `w` weeks, `m` months, `q` quarters (3 months), `y` years. Chain segments with no spaces |
| From the task's date | `+2w` | `edit` only: an offset from the task's current due date (today if it has none) |
| Clear | `_` | removes the date of a task being edited |

A relative offset adds the months first, then the days, so from the 31st a
shorter month ends the date at its last day (`31-01 + 1m = 28-02`). Years run
from 1000 to 9999.

```bash
rusk add Tax return -d 30-apr-26
rusk add Follow up -d 2w
rusk edit 1 -d +2w
rusk edit 1 -d _
```

In the [interactive editor](#interactive-editor) the date is the first word
of the first line.

### Listing and search

```bash
rusk                     # or `rusk list`
rusk list --compact      # one line per task; `…` marks a task that goes on
rusk list --no-compact   # full view for one run, even with `compact = true` in the config

rusk search omega        # matches highlighted
rusk s buy milk          # the words are one phrase
rusk s --id omega        # only the ids, one per line (script-friendly)
```

The compact view trims trailing punctuation and never wraps. A new task that
gets the id of a deleted one is listed in its place (in a hand-edited file
with an order of your own: at the end).

Search ignores case (`ΟΔΟΣ` finds `οδος`, `STRASSE` finds `Straße`) and the
amount of whitespace between the words (the list shows a run of spaces as
one), but not accents (`cafe` does not find `café`).

### Marking

```bash
rusk mark 1              # done; run it again to undo
rusk mark 1 --priority   # priority on; run it again to remove it
```

`mark` says what it changed: done, undone, priority or priority removed.
Priority survives done/undone toggles.

### Editing

```bash
rusk edit 1 Complete the project documentation  # new text in one shot
rusk edit 1 -d 1-1-25                            # only the date
rusk edit 1 -- -x means exclude                  # text that starts with a dash
rusk edit 1 -- 42                                # text made of numbers only
```

Everything after `--` is text, word for word. Before `--`, a word that starts
with a dash is an option, and `-h` prints the help. Without new text,
`rusk edit 1` opens the [interactive editor](#interactive-editor).

### Dependencies and keywords

```bash
rusk add deploy --after 19,22   # shown as `deploy (19,22)` in the list
rusk edit 1 --after 19,22       # change the list (a set: the order does not matter)
rusk edit 1 --after _           # clear it
```

A dependency is an ordering hint for agents and tooling: the task should be
done no earlier than the tasks it depends on. `rusk mark` is never blocked by
it.

A task text that starts with `TEMP`, `INFO`, `FIXME` or `WIP` gets that word
highlighted in the list. The set is configurable (`keywords` in the config; an
empty value disables it), and so is the color (the `keyword` theme key).

```bash
rusk add TEMP debug flag for the release
rusk add INFO deploy notes are in the wiki
```

### Deleting

```bash
rusk del 1          # asks for confirmation first
rusk del --done     # all completed tasks
rusk del 1 --yes    # no question asked: for scripts and pipes
rusk del --done -y
```

The question names the tasks that depend on the one being deleted, and the
deletion says which of them no longer do (`Task 3 no longer depends on 1.`).
Without a terminal to ask on (a script, a pipe) `rusk del` refuses instead of
guessing: `--yes` is how to delete there.

### Several tasks at once

`mark`, `edit` and `del` take one comma-separated list of ids: `1,2,3`
(`1, 2, 3` is read the same way).

```bash
rusk mark 1,2,3
rusk edit 1,2,3 Update status to completed
rusk del 1,2,3
```

- Anything in the list that is not an id is an error; a repeated id counts once.
- For `mark` and `del` a second word after the list is an error (`rusk mark 1 2`).
- For `edit` the first word that is not glued to the list by a comma starts
  the new text: `rusk edit 3 1,000 units` edits task 3 only. When everything
  after the list is numbers (`rusk edit 1,2 3 -d 2w`) nothing changes, and
  rusk says so: a text of numbers goes after `--`.

### Interactive editor

The interactive multi-line editor supports selection, the system clipboard,
undo/redo, word navigation, the mouse, crash-safe autosave and a colored date
header on the first line. The full reference is [EDITOR.md](EDITOR.md).

```bash
rusk add            # create a task in the editor
rusk add -d 2w      # with the first line pre-filled
rusk edit 1         # edit the text and the due date on the first line
rusk edit 1,2,3     # several tasks in one session
```

### Aliases

| Command | Alias |
|---|---|
| `add` | `a` |
| `list` | `l` |
| `mark` | `m` |
| `edit` | `e` |
| `del` | `d` |
| `restore` | `r` |
| `gen` | `g` |
| `search` | `s` |
| `completions` | `c` |

| Flag | Short | Where |
|---|---|---|
| `--date` | `-d` | `add`, `edit` |
| `--after` | `-a` | `add`, `edit` |
| `--compact` | `-c` | `list` (`--no-compact` has no short form) |
| `--priority` | `-p` | `mark` |
| `--yes` | `-y` | `del` |
| `--output` | `-o` | `gen` |
| `--help` | `-h` | everywhere |
| `--version` | `-V` | `rusk -V` |

## Web UI and sync

A mobile-first web frontend, themed from your [configuration file](CONFIG.md)
and built from a single embedded template: there is no separate frontend to
install. The full guide (authentication, deployment behind Caddy or nginx,
the API, sync) is [WEB.md](WEB.md).

```bash
rusk gen -o index.html      # a self-contained read-only page with all tasks
rusk serve                  # the interactive UI at http://127.0.0.1:7272: full task editing from a phone
rusk serve --host 0.0.0.0   # requires web_token in the config
```

`rusk gen` never writes over the database or a file rusk keeps beside it.

`rusk serve` stays up whatever clients do: a connection may wait a minute
(`web_timeout` in the config, seconds), a request body or an answer has to
keep moving, and up to 256 connections are open at once. A connection that
waits longer is closed, with `408 Request Timeout` once part of a request had
come. `web_timeout = 0` turns these limits off. They keep the server up, not
fair: exposed beyond loopback or a VPN, put a reverse proxy in front (see
[WEB.md](WEB.md#rusk-serve)).

Sync works over SSH, or over HTTP(S) with a running `rusk serve`:

```bash
# in the config: sync_remote = user@vps:/srv/tasks/tasks.json
rusk sync              # fast-forward in whichever direction changed
rusk sync push         # upload local tasks
rusk sync pull --force # discard local changes in favor of the remote
```

Conflicts are detected against what each side held after the last sync:
nothing is silently overwritten, a change made on either side right after a
sync is a change for the next one, and a first sync into an empty database
simply fills it. Details in [WEB.md](WEB.md#rusk-sync).

## Storage

The short version. The full reference, including what each format stores and
how backups and concurrent writers work, is [STORAGE.md](STORAGE.md).

### Database location

By default rusk stores tasks in `./.rusk/tasks.json`, relative to the current
directory, so every project has a task list of its own:

```bash
cd ~/projects/website && rusk add Fix responsive layout
cd ~/projects/api && rusk add Add authentication endpoint
```

The `RUSK_DB` environment variable or the `rusk_db` key in the
[configuration file](CONFIG.md) point elsewhere (`RUSK_DB` wins):

```bash
export RUSK_DB="/path/to/your/db.json"    # a file
export RUSK_DB="/path/to/your/project/"   # a directory: tasks.json is created inside
```

Debug builds (`cargo run`) ignore `RUSK_DB` and use a database of their own
under the temp directory; see [STORAGE.md](STORAGE.md#debug-builds).

### Database formats

The last extension of the database file name picks the format (JSON by
default; `notes.txt.json` is JSON):

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

All features except `backend-sqlite` (which bundles a C library) are enabled
by default; distro builds can trim them (`--no-default-features --features …`).

Every format can be edited by hand or by another tool, and backups and atomic
writes work the same in each. What each format stores, and the rules every
file is held to when it is read, are in
[STORAGE.md](STORAGE.md#database-formats).

### Remote and git-backed databases

The *shape* of `rusk_db` picks the storage backend:

```bash
rusk_db = https://tasks.example.com     # the API of a running `rusk serve` (feature backend-http)
db_token = s3cret                       # its web_token, if set (or RUSK_DB_TOKEN)

rusk_db = alex@vps:/srv/tasks/tasks.md  # a file over ssh (feature backend-ssh)

git_backend = true                      # commit every save of a local file database
                                        # to a git repo in its directory (feature backend-git)
```

- **http(s)** is a thin client: there is no local copy, every command reads
  and writes through the server, and never overwrites what another client
  stored in the meantime. It needs the network and the server up; for
  offline-first use `rusk sync` instead.
- **ssh** loads and saves the remote file per command over the system `ssh`
  (keys, agent and `~/.ssh/config` apply). On flaky links prefer `rusk sync`.
- **git_backend** gives full history (`git log`, `git revert`) beyond the
  single `.backup` copy. Local file databases only; uses the system `git`,
  2.9 or newer.

Details in [STORAGE.md](STORAGE.md#remote-and-git-backed-databases).

### Data safety

- Every save first copies the previous database to a `.backup` sibling
  (`tasks.json.backup`; `backup = false` turns this off). `rusk restore`
  brings it back, and keeps what it replaces as `.before_restore`.
- Every file is replaced atomically: temp file, fsync, rename. A crash, a
  full disk or a parallel `rusk` leaves the old content or the new one,
  never a truncated file.
- A database that cannot be read is never taken for an empty one: the
  command stops instead of saving zero tasks over your data.
- Commands running at the same time (two terminals, a cron job, `rusk serve`)
  all take effect: saves take turns on a lock, and every change is applied to
  the database as it is at the moment of the save.
- A command that changes nothing writes nothing.

Details in [STORAGE.md](STORAGE.md#data-safety).

## Configuration

### Configuration file

rusk creates a configuration file on first run: `~/.config/rusk/cfg` on Linux,
other platforms in [CONFIG.md](CONFIG.md). It holds the theme colors of every
element group, the default behavior and the web and sync settings. The full
reference is [CONFIG.md](CONFIG.md).

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
# web_timeout = 60
# sync_remote = user@vps:/srv/tasks/tasks.json
```

Colors are ANSI names (`red`, `cyan`, `bright_black`, ...) or hex
(`#d75f00`). A broken line never prevents rusk from starting: it produces a
warning and the default is kept. `RUSK_CONFIG=<path>` overrides the file
location; an empty `RUSK_CONFIG` disables the config entirely.

### Shell completion

```bash
rusk completions install zsh      # bash, zsh, fish, nu, powershell; several at once
rusk completions show zsh         # print the script for a manual install
```

Tab completes the commands and their aliases, the flags (`--date`,
`--after`, `--done`, …) and, after `rusk edit <id>`, the task's own text,
quoted so that running the line stores exactly that text. Manual installation
per shell, what exactly is completed and the Windows notes are in
[completions/README.md](completions/README.md).

### Colors

`RUSK_NO_COLOR` set to any non-empty value disables ANSI colors in all output:
the task list, dialogs, errors, `--help` and argument errors.

```bash
export RUSK_NO_COLOR=1
```

The standard `NO_COLOR` (see [no-color.org](https://no-color.org)) and
`no_color = true` in the config do the same; an empty variable counts as not
set. Once colors are off this way, nothing turns them back on for the run, not
even `CLICOLOR_FORCE`, which otherwise forces colors into a pipe. `CLICOLOR=0`
turns them off as well, and so does `TERM=dumb` unless `CLICOLOR_FORCE` is set.

### Scripts and pipes

- What `rusk` prints goes to a pipe as well as to a terminal.
- A reader that stops reading (`rusk list | head -2`) ends the command quietly
  with exit code 0; whatever the command changed is saved before anything is
  printed. An output that cannot be written (a full disk) is an error, exit
  code 1.
- Questions are asked only on a terminal (standard input and output both).
  `rusk del` without one refuses and asks for `--yes`; the editors (`rusk add`,
  `rusk edit <id>`) need a terminal too.
- `rusk search --id` prints the matching ids only, one per line, no colors.

## Documentation

| | |
|---|---|
| [CONFIG.md](CONFIG.md) | the configuration file: settings, theme, variables |
| [EDITOR.md](EDITOR.md) | the interactive editor: keys, drafts, the date header |
| [WEB.md](WEB.md) | the web UI, `rusk serve`, deployment, the API, `rusk sync` |
| [STORAGE.md](STORAGE.md) | database location, formats, remote backends, backups, concurrent writers |
| [completions/README.md](completions/README.md) | shell completion, per shell |
| [ARCHITECTURE.md](ARCHITECTURE.md) | how the code is laid out |

<br />

<p align="center"><a href="#rusk">Back to top</a></p>
