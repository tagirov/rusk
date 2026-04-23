# Architecture

Cross-platform terminal task manager. Single Rust crate, one library + one binary.

## Module map

```
src/
├── main.rs              # Entry point: CLI dispatch, clap parsing
├── lib.rs               # Crate root, re-exports
├── args.rs              # Clap structs: Cli, Command, CompletionAction
├── model.rs             # Task struct (serde, chrono)
├── storage.rs           # TaskManager: CRUD, JSON persistence, backup/restore
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
│       ├── mod.rs       # Session loop, save/cancel, help overlay
│       ├── state.rs     # Buffer, cursor, selection, undo steps
│       ├── view.rs      # Layout, soft wrap, date highlighting
│       ├── input.rs     # Key dispatch
│       ├── terminal.rs  # Raw mode, draw, resize
│       ├── history.rs   # Undo/redo stacks
│       ├── text_ops.rs  # Word bounds, kill-line, paste
│       ├── draft.rs     # Autosave / recovery JSON
│       ├── clipboard.rs # System clipboard (arboard) + fallback
│       └── mouse.rs     # Click, drag, wheel
├── completions.rs       # Shell completion scripts (include_str!), Shell enum
└── windows_console.rs   # Windows ANSI support via windows-sys
```

## Dependency graph (internal)

```
main.rs
  ├── args        (Cli, Command)
  ├── cli         (HandlerCLI)
  ├── parser      (parse_flexible_ids, parse_edit_args, is_cli_date_help_value)
  ├── storage     (TaskManager)
  ├── completions (Shell)          [feature = "completions"]
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
  └── parser/date (parse_cli_date)

completions
  └── dirs      (home_dir)         [feature = "completions"]
```

## Feature flags

| Feature       | Default | Gates                                                        |
|---------------|---------|--------------------------------------------------------------|
| `completions` | yes     | `completions` module, `dirs` dep, `Completions` CLI command  |
| `interactive` | yes     | `crossterm` + `arboard` deps, editor (clipboard), dialogs      |

Without `interactive`: edit commands only work with inline text (`rusk edit 1 new text`),
delete skips confirmation. Terminal width falls back to 80 columns.

With `interactive`, `rusk edit <id>` opens the full-screen editor for task text and an
optional due date on the first line: `Enter` inserts a newline, `Ctrl+S` saves, `Esc` skips,
`Ctrl+G` / `F1` shows in-editor help (including date syntax).

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
| `crossterm`  | Terminal raw mode, cursor, key events  |
| `arboard`    | System clipboard (editor copy/paste)   |
| `dirs`       | Home directory detection               |
| `windows-sys`| Windows console API (cfg(windows))     |

## Data flow

```
User input → clap (args.rs) → main.rs dispatch
  → HandlerCLI (cli/handlers.rs)
    → TaskManager (storage.rs) ←→ ~/.rusk/tasks.json
    → formatter/editor/dialogs → stdout
```

## Persistence

JSON file at `$RUSK_DB` or `.rusk/tasks.json`. Atomic write via temp+rename with
copy fallback. Auto-backup to `.json.backup` on every save.
