# Configuration file

rusk reads an optional configuration file at startup:

| Platform | Default path |
|---|---|
| Linux | `~/.config/rusk/cfg` |
| macOS | `~/Library/Application Support/rusk/cfg` |
| Windows | `%APPDATA%\rusk\cfg` |

The file is **auto-created** on first run with every setting listed as a
commented-out default, so it doubles as documentation. It is safe to edit or
delete at any time.

- `RUSK_CONFIG=<path>` overrides the location (the file is auto-created there
  if missing).
- `RUSK_CONFIG=` (empty) disables the config system entirely.
- Debug builds and test runs never touch the default config file; only an
  explicit `RUSK_CONFIG` is honored there.

Problems in the file are **never fatal**: every unparseable line produces a
yellow warning with its line number (`cfg:12: ...`) and the built-in default
is kept — a broken theme cannot lock you out of your tasks.

## Syntax

```
# Comments start with `#`; blank lines are ignored.
key = value              # inline comments too (a leading `#` is a hex color, not a comment)
title = "quoted value"   # quotes make the value literal (no variable resolution)
```

- One `key = value` per line. Keys are lowercase `a-z`, digits, `_`.
- **Variables**: any key that is not a recognized setting defines a variable.
  A later bare value that matches a variable name resolves to it:

  ```
  my_accent = #d75f00
  priority_marker = my_accent
  ```

- A **theme key can reference another theme key** — `priority_marker = accent`
  copies accent's current value.
- The literal value `default` keeps the built-in default.
- Duplicate keys: the last one wins.
- A variable that is never referenced produces a warning (this catches typos
  in setting names).

## Settings

Environment variables always win over config values.

| Key | Type | Default | Env override | Meaning |
|---|---|---|---|---|
| `rusk_db` | location | — | `RUSK_DB` | Database location. A file or directory path (`~/` expands to the home directory) — the extension picks the format: `.csv`, `.md`, `.txt` (todo.txt), `.ndjson`/`.jsonl`, `.ics`, `.db`/`.sqlite`/`.sqlite3` (SQLite), anything else is JSON. Or a remote: `https://host` (the API of a running `rusk serve`) or `user@host:/path` (a file over ssh). See README → Database Formats; non-JSON/CSV variants need their build feature |
| `db_token` | string | — | `RUSK_DB_TOKEN` | Bearer token when `rusk_db` is an http(s) location (the serve's `web_token`) |
| `git_backend` | bool | `false` | — | Commit every save of a local file database to a git repository in the database directory (needs `git` and the `backend-git` feature) |
| `no_color` | bool | `false` | `RUSK_NO_COLOR` / `NO_COLOR` | Disable ANSI colors. The config can only disable colors, never re-enable them over the environment |
| `compact` | bool | `false` | — | Compact `rusk list` view by default (`-c` still forces it per run) |
| `backup` | bool | `true` | — | Write a `.backup` copy next to the database on every save |
| `web_host` | string | `127.0.0.1` | — | Bind address for `rusk serve` (see [WEB.md](WEB.md)) |
| `web_port` | u16 | `7272` | — | Port for `rusk serve` |
| `web_token` | string | — | — | Access token for `rusk serve`; required for non-loopback hosts |
| `sync_remote` | string | — | `RUSK_SYNC_REMOTE` | Remote for `rusk sync`: `user@host:/path/tasks.json` (ssh) or `https://host` (serve API) |
| `sync_token` | string | — | `RUSK_SYNC_TOKEN` | Bearer token for http(s) sync remotes |
| `keywords` | list | `TEMP INFO FIXME WIP` | — | Keywords highlighted when a task text starts with one (only the first word is matched; space- or comma-separated, case-sensitive; `""` disables). Color comes from the `keyword` theme key |

## Theme

Each key colors one semantic group of elements. Values are either one of the
16 ANSI color names — `black`, `red`, `green`, `yellow`, `blue`, `magenta`
(alias `purple`), `cyan`, `white` and their `bright_*` variants — or a hex
value like `#ffa500`. ANSI names follow your terminal palette; hex values
render as truecolor.

| Key | Default | Used for |
|---|---|---|
| `error` | `red` | Error messages |
| `success` | `green` | "Added task", "Edited task", confirmation checkmarks |
| `warning` | `yellow` | Warnings, "No tasks", skipped/not-found notices |
| `notice` | `magenta` | "Task unchanged", "Canceled ..." |
| `info` | `cyan` | Date-change details, completions instructions |
| `emphasis` | `white` | IDs and counts inside prompts |
| `accent` | `#ffa500` | Confirmation prompts, delete dialogs |
| `list_header` | `blue` | The `id / date / task` header of `rusk list` |
| `done_marker` | `green` | The `✔` in the list |
| `priority_marker` | `#ffa500` | The `p` in the list |
| `task_id` | terminal default | Task IDs in the list (always bold) |
| `search_match` | `yellow` | Matched text in `rusk search` output (always bold) |
| `keyword` | `cyan` | `TEMP` / `INFO` keyword tokens in task text |
| `date_overdue` | `red` | Overdue dates on not-done tasks |
| `date_today` | `cyan` | Dates due today (same as upcoming by default) |
| `date_upcoming` | `cyan` | All other dates |
| `editor_date` | `green` | Valid date token on the editor's first line |
| `editor_footer` | `#808080` | Editor footer and scroll arrows |
| `editor_dirty` | `#b9b9be` | Editor unsaved-changes dot `●` |
| `editor_clean` | `#646464` | Editor saved dot `○` |

Out of the box the theme reproduces the previous hardcoded colors exactly.

### Web mapping

`rusk gen` and `rusk serve` expose the theme to the web page as CSS custom
properties (`--rusk-date-overdue`, `--rusk-priority-marker`, ...). Hex values
pass through unchanged; ANSI names map through the canonical xterm palette:

| Name | CSS | Name | CSS |
|---|---|---|---|
| black | `#000000` | bright_black | `#7f7f7f` |
| red | `#cd0000` | bright_red | `#ff0000` |
| green | `#00cd00` | bright_green | `#00ff00` |
| yellow | `#cdcd00` | bright_yellow | `#ffff00` |
| blue | `#0000ee` | bright_blue | `#5c5cff` |
| magenta | `#cd00cd` | bright_magenta | `#ff00ff` |
| cyan | `#00cdcd` | bright_cyan | `#00ffff` |
| white | `#e5e5e5` | bright_white | `#ffffff` |

Note that terminal rendering of named colors follows *your* terminal palette,
while the web export uses this fixed table.

## Example

```
# palette
ink = #e8e8ea
fire = bright_red

# behavior
compact = true
rusk_db = ~/tasks/tasks.json

# theme
accent = #d75f00
priority_marker = accent
date_overdue = fire
emphasis = ink
```
