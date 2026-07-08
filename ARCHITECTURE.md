# Architecture

Cross-platform terminal task manager. Single Rust crate, one library + one binary.

## Module map

```
src/
├── main.rs              # Entry point: CLI dispatch, clap parsing
├── lib.rs               # Crate root, re-exports
├── args.rs              # Clap structs: Cli, Command, CompletionAction, SyncDirection
├── model.rs             # Task struct (serde, chrono)
├── storage.rs           # TaskManager: CRUD, persistence, backup/restore
├── codec.rs             # On-disk formats: JSON (default) / CSV by db extension
├── config.rs            # Config file (~/.config/rusk/cfg): parser, Theme, global access
├── sync.rs              # rusk sync: ssh/curl transports, conflict detection [feature = "sync"]
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
  ├── codec     (DbFormat, to_csv/from_csv)
  ├── config    (rusk_db, backup, warning color)
  └── parser/date (parse_cli_date)

config
  ├── colored   (Color for theme values)
  └── dirs      (config_dir, home_dir)

web
  ├── storage   (TaskManager: per-request reload, CRUD)
  ├── config    (theme → CSS variables, web_host/port/token)
  └── tiny_http (server loop)      [feature = "web"]

sync
  ├── storage   (resolve_db_path, load, save)
  ├── codec     (remote .csv support)
  └── config    (sync_remote, sync_token)

completions
  └── dirs      (home_dir)
```

Nearly every module also reads `config::theme()` for output colors.

## Feature flags

| Feature       | Default | Gates                                                        |
|---------------|---------|--------------------------------------------------------------|
| `completions` | yes     | `completions` module, `Completions` CLI command              |
| `interactive` | yes     | `crossterm` + `arboard` deps, editor (clipboard), dialogs      |
| `web`         | yes     | `tiny_http` dep, `web` module, `gen` + `serve` CLI commands  |
| `sync`        | yes     | `sync` module, `sync` CLI command (shells out to ssh/curl)   |

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
| `windows-sys`| Windows console API (cfg(windows))     |

## Data flow

```
User input → clap (args.rs) → main.rs (config::load → init) dispatch
  → HandlerCLI (cli/handlers.rs)
    → TaskManager (storage.rs) ←→ codec (JSON/CSV) ←→ database file
    → formatter/editor/dialogs → stdout (colors from config::theme)

rusk serve: browser ←→ tiny_http loop (web/server.rs)
    → api.rs handlers → fresh TaskManager per request ←→ database file

rusk sync: sync.rs → ssh (cat / tmp+mv) or curl (GET/PUT /api/tasks)
    → conflict check vs .sync state file → TaskManager::save or remote replace
```

## Persistence

Database at `$RUSK_DB`, `rusk_db` from the config, or the default relative
path. Format by extension: JSON (default) or CSV (`codec.rs`). Atomic write
via temp+rename with copy fallback. Auto-backup to `.<ext>.backup` on every
save (`backup = false` disables).

Task ids are `u8`: the smallest free id is reused, and the database holds at
most 255 tasks (adding beyond that returns an error).

## Configuration

`config.rs` parses a hand-rolled `key = value` format (see CONFIG.md): no
TOML dependency, user-defined variables, never-fatal warnings with line
numbers. The parsed `Config` lives in a process-wide `OnceLock`
(`config::config()` / `config::theme()`), initialized once in `main::run`;
library/unit-test use falls back to defaults. Test/debug isolation mirrors
the database rules: the default config path is only read by release binaries
outside test mode, and `RUSK_CONFIG` overrides everywhere (empty = disabled).
