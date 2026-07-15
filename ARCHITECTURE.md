# Architecture

Cross-platform terminal task manager. Single Rust crate, one library + one binary.

## Module map

```
src/
├── main.rs              # Entry point: CLI dispatch, clap parsing
├── lib.rs               # Crate root, re-exports
├── args.rs              # Clap structs: Cli, Command, CompletionAction, SyncDirection
├── model.rs             # Task struct (serde, chrono)
├── storage.rs           # TaskManager: CRUD over a Backend
├── codec/               # File formats, chosen by the db extension
│   ├── mod.rs           # DbFormat: detection, encode/decode dispatch, id fixup
│   ├── csv.rs           # RFC 4180, spreadsheet interop
│   ├── markdown.rs      # GitHub task list        [feature = "fmt-markdown"]
│   ├── todotxt.rs       # todo.txt                [feature = "fmt-todotxt"]
│   ├── ndjson.rs        # one JSON task per line  [feature = "fmt-ndjson"]
│   └── ics.rs           # iCalendar VTODO         [feature = "fmt-ics"]
├── backend/             # Storage backends, chosen by the shape of rusk_db
│   ├── mod.rs           # Backend enum: resolve/parse, load/save, backup/restore
│   ├── file.rs          # Local file: atomic write, JSON corruption reports
│   ├── sqlite.rs        # SQLite file             [feature = "backend-sqlite"]
│   ├── http.rs          # rusk serve API client   [feature = "backend-http"]
│   ├── ssh.rs           # File over ssh           [feature = "backend-ssh"]
│   └── git.rs           # Auto-commit layer for file dbs [feature = "backend-git"]
├── transport.rs         # System ssh/curl process helpers [backend-http/-ssh]
├── config.rs            # Config file (~/.config/rusk/cfg): parser, Theme, global access
├── sync.rs              # rusk sync: conflict detection over the remote backends [feature = "sync"]
├── error.rs             # AppError enum (typed errors for anyhow downcast)
├── parser/
│   ├── mod.rs           # Re-exports
│   ├── date.rs          # Date parsing: absolute (DD-MM-YYYY), relative (2d, 3w, 1q)
│   └── ids.rs           # ID parsing (del/mark): comma lists for multiple IDs; without commas only first token counts; edit args
├── cli/
│   ├── mod.rs           # HandlerCLI struct, submodule declarations
│   ├── handlers.rs      # Command handlers: add, del, mark, edit, list, restore
│   ├── formatter.rs     # Text wrapping, ANSI stripping, terminal width, compact first-line trim
│   ├── dialogs.rs       # Confirmation prompts (crossterm raw mode)
│   └── editor/          # Interactive full-screen editor (crossterm): task text + first-line date
│       ├── mod.rs       # Session loop: setup → poll → dispatch → render; save/cancel exits
│       ├── state.rs     # EditorState: buffer, cursor, selection, snapshots
│       ├── view.rs      # Layout, soft wrap, date highlighting, footer
│       ├── input.rs     # Key / mouse / paste dispatch into Actions
│       ├── terminal.rs  # Alt screen, raw mode, help overlay, discard confirm
│       ├── history.rs   # Undo/redo stacks with single-char coalescing
│       ├── text_ops.rs  # Pure text helpers: char boundaries, word jumps, multi-line edits
│       ├── draft.rs     # Autosave / recovery JSON
│       ├── clipboard.rs # System clipboard (arboard + OSC 52) + process-local fallback
│       └── mouse.rs     # Click tracking, screen → buffer mapping
├── web/                 # Web frontend [feature = "web"]
│   ├── mod.rs           # Single-file template rendering (gen + serve share it), theme → CSS vars
│   ├── template.html    # Mobile-first UI (vanilla HTML/CSS/JS, embedded via include_str!)
│   ├── api.rs           # JSON API handlers over TaskManager (transport-agnostic, unit-tested)
│   └── server.rs        # tiny_http loop: routing, cookie/Bearer auth, login page
├── completions.rs       # Shell completion scripts (include_str!), Shell enum
└── windows_console.rs   # Windows ANSI support via windows-sys
```

## Dependency graph (internal)

```
main.rs
  ├── args        (Cli, Command)
  ├── cli         (HandlerCLI)
  ├── config      (load, init, theme)
  ├── parser      (parse_flexible_ids, parse_edit_args, is_cli_date_help_value)
  ├── storage     (TaskManager)
  ├── sync        (run)             [feature = "sync"]
  ├── web         (render, server)  [feature = "web"]
  ├── completions (Shell)           [feature = "completions"]
  └── windows_console

cli/handlers
  ├── model       (Task)
  ├── storage     (TaskManager)
  ├── parser/date (parse_cli_date)
  ├── cli/formatter
  ├── cli/editor                   [feature = "interactive"]
  └── cli/dialogs                  [feature = "interactive"]

cli/formatter
  └── crossterm::terminal::size    [feature = "interactive", fallback to 80]

cli/editor
  ├── crossterm (raw mode, cursor) [feature = "interactive"]
  ├── arboard   (system clipboard) [feature = "interactive"]
  └── error     (AppError::SkipTask)

cli/dialogs
  └── crossterm (raw mode)         [feature = "interactive"]

storage
  ├── model     (Task)
  ├── backend   (Backend: resolve, load, save, restore)
  ├── config    (backup, warning color)
  └── parser/date (parse_cli_date)

backend
  ├── codec     (DbFormat encode/decode by extension)
  ├── transport (ssh/curl helpers)  [backend-http / backend-ssh]
  ├── rusqlite  (SQLite)            [backend-sqlite]
  └── config    (rusk_db, db_token, git_backend, backup)

config
  ├── colored   (Color for theme values)
  └── dirs      (config_dir, home_dir)

web
  ├── storage   (TaskManager: per-request reload, CRUD)
  ├── config    (theme → CSS variables, web_host/port/token)
  └── tiny_http (server loop)      [feature = "web"]

sync
  ├── backend   (local Backend + remote ssh/http backends)
  └── config    (sync_remote, sync_token)

completions
  └── dirs      (home_dir)
```

Nearly every module also reads `config::theme()` for output colors.

## Feature flags

| Feature          | Default | Gates                                                        |
|------------------|---------|--------------------------------------------------------------|
| `completions`    | yes     | `completions` module, `Completions` CLI command              |
| `interactive`    | yes     | `crossterm` + `arboard` deps, editor (clipboard), dialogs      |
| `web`            | yes     | `tiny_http` dep, `web` module, `gen` + `serve` CLI commands  |
| `sync`           | yes     | `sync` module + CLI command; implies `backend-ssh` + `backend-http` |
| `fmt-markdown`   | yes     | `.md` database format (GitHub task list)                     |
| `fmt-todotxt`    | yes     | `.txt` database format (todo.txt)                            |
| `fmt-ndjson`     | yes     | `.ndjson` / `.jsonl` database format                         |
| `fmt-ics`        | yes     | `.ics` database format (iCalendar VTODO)                     |
| `backend-http`   | yes     | `rusk_db = https://…` (rusk serve API, system curl)          |
| `backend-ssh`    | yes     | `rusk_db = user@host:/path` (system ssh)                     |
| `backend-git`    | yes     | `git_backend = true` auto-commit (system git)                |
| `backend-sqlite` | **no**  | `.db` / `.sqlite` / `.sqlite3` databases (`rusqlite`, bundled C) |

A database location whose format/backend feature is compiled out fails with
an error naming the missing feature instead of mis-parsing the file.

Without `interactive`: edit commands only work with inline text (`rusk edit 1 new text`),
delete skips confirmation. Terminal width falls back to 80 columns.

With `interactive`, `rusk edit <id>` opens the full-screen editor for task text and an
optional due date on the first line: `Enter` inserts a newline, `Ctrl+S` saves, `Esc` skips,
`Ctrl+G` / `F1` shows in-editor help (including date syntax). The same editor is used for
`rusk add` with no inline task text (a TTY is required; optional `rusk add -d …` pre-fills
the first line; drafts use the `new-task` key).

Without `completions`: `rusk completions` subcommand is unavailable.

Build minimal binary: `cargo build --release --no-default-features`

## External dependencies

| Crate        | Purpose                                |
|--------------|----------------------------------------|
| `clap`       | CLI argument parsing (derive)          |
| `colored`    | Terminal colors                        |
| `serde`      | Serialization framework                |
| `serde_json` | JSON persistence                       |
| `chrono`     | Date types and arithmetic              |
| `anyhow`     | Error handling                         |
| `tiny_http`  | HTTP server for `rusk serve` (feature `web`) |
| `crossterm`  | Terminal raw mode, cursor, key events  |
| `arboard`    | System clipboard (editor copy/paste)   |
| `dirs`       | Config/home directory detection        |
| `rusqlite`   | SQLite backend (feature `backend-sqlite`, bundled) |
| `windows-sys`| Windows console API (cfg(windows))     |

The remote backends and `git_backend` deliberately shell out to the system
`ssh`/`curl`/`git` instead of adding TLS/ssh/git crates.

## Data flow

```
User input → clap (args.rs) → main.rs (config::load → init) dispatch
  → HandlerCLI (cli/handlers.rs)
    → TaskManager (storage.rs) → Backend (backend/)
        file:   codec (by extension) ←→ local file (+ optional git commit)
        sqlite: rusqlite ←→ .db file
        http:   curl ←→ GET/PUT /api/tasks of a running rusk serve
        ssh:    ssh ←→ remote file (codec by remote extension)
    → formatter/editor/dialogs → stdout (colors from config::theme)

rusk serve: browser ←→ tiny_http loop (web/server.rs)
    → api.rs handlers → fresh TaskManager per request ←→ database

rusk sync: sync.rs → the ssh/http backends as transports
    → conflict check vs .sync state file → local Backend::save or remote replace
```

## Persistence

Database location from `$RUSK_DB`, `rusk_db` in the config, or the default
relative path; the shape of the value picks the backend (`backend/mod.rs`),
the extension picks the file format (`codec/mod.rs`). Local writes are
atomic (temp+rename with copy fallback); SQLite saves run in a transaction.
Auto-backup to `.<ext>.backup` on every save for local backends
(`backup = false` disables); `git_backend = true` additionally commits every
save to a git repository in the database directory.

Every backend implements the same whole-database load/save contract, so the
formats stay interchangeable and `rusk sync` hashes content canonically
(parsed tasks re-encoded as compact JSON) regardless of representation.

Task ids are `u32` (`TaskId` in `model.rs`): the smallest free id is reused.
A task may list other task ids in `after` (`--after`): the dependencies are
shown after the text as `(19,22)`, block `rusk mark` until they are done,
and are stripped automatically when the referenced tasks are deleted.

## Configuration

`config.rs` parses a hand-rolled `key = value` format (see CONFIG.md): no
TOML dependency, user-defined variables, never-fatal warnings with line
numbers. The parsed `Config` lives in a process-wide `OnceLock`
(`config::config()` / `config::theme()`), initialized once in `main::run`;
library/unit-test use falls back to defaults. Test/debug isolation mirrors
the database rules: the default config path is only read by release binaries
outside test mode, and `RUSK_CONFIG` overrides everywhere (empty = disabled).
