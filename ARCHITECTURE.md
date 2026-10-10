# Architecture

Cross-platform terminal task manager. Single Rust crate, one library + one binary.

- [Module map](#module-map)
- [Dependency graph (internal)](#dependency-graph-internal)
- [Feature flags](#feature-flags)
- [Interactive editor](#interactive-editor)
- [External dependencies](#external-dependencies)
- [Data flow](#data-flow)
- [Persistence](#persistence)
  - [Location and format](#location-and-format)
  - [Atomic writes](#atomic-writes)
  - [Backups, git and restore](#backups-git-and-restore)
  - [SQLite](#sqlite)
  - [Normalization](#normalization)
  - [What the encoders keep](#what-the-encoders-keep)
  - [Missing, empty, unreadable](#missing-empty-unreadable)
  - [Concurrent writers](#concurrent-writers)
  - [The web API and the page](#the-web-api-and-the-page)
  - [Errors](#errors)
  - [Task ids and dependencies](#task-ids-and-dependencies)
- [Output](#output)
- [Configuration](#configuration)
  - [Dates](#dates)
- [Shell completion](#shell-completion)

## Module map

```
src/
├── main.rs              # Entry point: CLI dispatch, clap parsing
├── lib.rs               # Crate root, re-exports
├── args.rs              # Clap structs: Cli, Command, CompletionAction, SyncDirection
├── model.rs             # Task struct (serde, chrono); normalize: the rules every list read from storage follows
├── storage.rs           # TaskManager: CRUD over a Backend; every change is an `update` (read-modify-write)
├── atomic.rs            # The one local write path: unique temp + fsync + rename, mode/owner kept
├── lock.rs              # Advisory writer lock of a file database (`<db>.lock`)
├── revision.rs          # Content identities: file fingerprint, task-list revision (ETag, sync hash)
├── width.rs             # Text measured in terminal cells: grapheme clusters, wide chars, cut points
├── printable.rs         # Task text on its way to a terminal: control characters escaped
├── output.rs            # stdout/stderr of the commands: out!/outln!/errln!, a closed pipe (Closed), the color decision (Colors)
├── search.rs            # rusk search: case and whitespace folded alike on both sides, matches as byte ranges
├── codec/               # File formats, chosen by the db extension
│   ├── mod.rs           # DbFormat: detection, encode/decode dispatch (records as written)
│   ├── csv.rs           # RFC 4180, spreadsheet interop
│   ├── markdown.rs      # GitHub task list        [feature = "fmt-markdown"]
│   ├── todotxt.rs       # todo.txt                [feature = "fmt-todotxt"]
│   ├── ndjson.rs        # one JSON task per line  [feature = "fmt-ndjson"]
│   └── ics.rs           # iCalendar VTODO         [feature = "fmt-ics"]
├── backend/             # Storage backends, chosen by the shape of rusk_db
│   ├── mod.rs           # Backend enum: resolve/parse, load/save/update, StaleDatabase, backup/restore
│   ├── file.rs          # Local file: load/save via atomic.rs under the writer lock, JSON corruption reports
│   ├── sqlite.rs        # SQLite file             [feature = "backend-sqlite"]
│   ├── http.rs          # rusk serve API client   [feature = "backend-http"]
│   ├── ssh.rs           # File over ssh           [feature = "backend-ssh"]
│   └── git.rs           # Auto-commit layer for file dbs [feature = "backend-git"]
├── transport.rs         # System ssh/curl processes: fenced remote read, verified remote write [backend-http/-ssh]
├── location.rs          # One reading of rusk_db / RUSK_DB / sync_remote: local path (~ expanded), http base URL, [user@]host:path
├── config.rs            # Config file (~/.config/rusk/cfg): parser, Theme, global access
├── sync.rs              # rusk sync: conflict detection over the remote backends [feature = "sync"]
├── error.rs             # AppError enum (typed errors for anyhow downcast)
├── parser/
│   ├── mod.rs           # Re-exports
│   ├── date.rs          # Date parsing: absolute (DD-MM-YYYY), relative (2d, 3w, 1q)
│   └── ids.rs           # Task id lists (mark/del/edit, --after): one comma-separated list, glued back across shell-split commas; edit: ids, then text
├── cli/
│   ├── mod.rs           # HandlerCLI struct, submodule declarations
│   ├── handlers.rs      # Command handlers: add, del, mark, edit, list, restore
│   ├── formatter.rs     # The list (format_task_list): rows as byte ranges (by cells), search highlight, compact row; ANSI stripping, terminal width
│   ├── dialogs.rs       # Confirmation prompts (crossterm raw mode, only while a key is read)
│   └── editor/          # Interactive full-screen editor (crossterm): task text + first-line date
│       ├── mod.rs       # Session loop: setup → poll → dispatch → render; save/cancel exits
│       ├── state.rs     # EditorState: buffer, cursor, selection, snapshots
│       ├── view.rs      # Layout, word-aware soft wrap by cells, date highlighting, footer
│       ├── input.rs     # Key / mouse / paste dispatch into Actions
│       ├── terminal.rs  # TerminalGuard (alt screen / raw mode / signals / panic hook), help, discard confirm
│       ├── history.rs   # Undo/redo stacks with single-char coalescing
│       ├── text_ops.rs  # Pure text helpers: char boundaries, word jumps, multi-line edits
│       ├── draft.rs     # Per-task draft slot: autosave, recovery, private directory
│       ├── clipboard.rs # System clipboard (arboard + OSC 52) + process-local fallback
│       └── mouse.rs     # Click tracking, screen → buffer mapping
├── web/                 # Web frontend [feature = "web"]
│   ├── mod.rs           # Single-file template rendering (gen + serve share it), theme → CSS vars
│   ├── template.html    # Mobile-first UI (vanilla HTML/CSS/JS, embedded via include_str!)
│   ├── api.rs           # JSON API handlers over TaskManager (transport-agnostic, unit-tested)
│   └── server.rs        # tiny_http server, a thread per request: routing, cookie/Bearer auth, Host check, login page
├── completions.rs       # Shell completion scripts (include_str!), Shell enum
└── windows_console.rs   # Windows ANSI support via windows-sys
```

## Dependency graph (internal)

```
main.rs
  ├── args        (Cli, Command)
  ├── cli         (HandlerCLI)
  ├── config      (load, init, theme)
  ├── parser      (parse_id_args, parse_edit_args, parse_id_list, is_cli_date_help_value)
  ├── storage     (TaskManager)
  ├── atomic      (rusk gen -o)     [feature = "web"]
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
  ├── atomic    (database, .backup, .before_restore writes)
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
| `backend-ssh`    | yes     | `rusk_db = [user@]host:path` (system ssh)                    |
| `backend-git`    | yes     | `git_backend = true` auto-commit (system git)                |
| `backend-sqlite` | **no**  | `.db` / `.sqlite` / `.sqlite3` databases (`rusqlite`, bundled C) |

Builds without a feature:

- A database location whose format/backend feature is compiled out fails
  with an error naming the missing feature instead of mis-parsing the file.
- Without `interactive`: edit commands only work with inline text
  (`rusk edit 1 new text`); `rusk add` and `rusk edit <id>` without text name
  the missing feature; delete skips confirmation; the terminal width falls
  back to 80 columns.
- The help names only what the build has (`args::root_after_long_help`, the
  add/edit texts): no editor, SQLite, ssh or sync variables where they are
  compiled out.
- Without `completions`: the `rusk completions` subcommand is unavailable.
- A minimal binary: `cargo build --release --no-default-features`.

`rusk del` asks before it deletes (`dialogs::read_confirmation`), on a
terminal only. With stdin or stdout not a terminal it refuses before asking
anything and names `--yes`, which deletes without asking in every build.
Either way the deletion goes through `TaskManager::delete_confirmed`, so a
task that changed in the meantime is reported, not deleted.

## Interactive editor

With `interactive`, `rusk edit <id>` opens the full-screen editor for task
text and an optional due date on the first line: `Enter` inserts a newline,
`Ctrl+S` saves, `Esc` skips, `Ctrl+G` / `F1` shows the in-editor help
(including the date syntax). The same editor is used for `rusk add` with no
inline task text (an optional `rusk add -d …` pre-fills the first line;
drafts use the `new-task` key). Both need a TTY and say so when there is
none.

**Giving the terminal back.** However a session ends (`Ctrl+S`, `Esc`, an
error, a panic, or a terminating signal), `terminal::TerminalGuard` gives
the terminal back: raw mode off, alternate screen left, mouse reporting and
bracketed paste off, cursor shown.

- On unix it also catches SIGTERM / SIGHUP / SIGQUIT: a handler that does
  nothing but set an atomic, which the loop notices within half a second.
  The buffer is written out as a draft and the process exits
  `128 + signal`.
- The panic hook is chained once per process and acts only while the
  terminal is the editor's, so a panic message is printed onto the restored
  screen rather than the one being torn down.

**Drafts.**

- A draft is one file per task (`editor-task-3.draft`) beside the database,
  or in `$XDG_RUNTIME_DIR/rusk` / `<temp>/rusk-<uid>` (0700) when the
  database is remote.
- That `<temp>/rusk-<uid>` (`scratch.rs`: made closed to others, refused
  when it is not the user's own directory) also holds the database of debug
  and test builds, `debug/tasks.json`, and those of the unit tests, since
  the temp directory itself is every user's.
- A draft is pinned to the identity of the text it was typed against, so it
  is never offered for a task that reused the id.
- It outlives the editor and is removed by whoever stored the text, so a
  save that fails leaves it where the next edit looks.
- One that cannot be parsed is kept as `.corrupt` rather than deleted.

**Task text and the terminal.** Task text is data from anywhere, so no
control character from it reaches the terminal.

- Everything that prints a task (`list`, `search`, `add`/`mark`/`edit`
  reports, the delete dialog, warnings quoting a task, the draft prompt)
  escapes C0/DEL/C1 as `\xNN` via `printable::escape` before wrapping and
  styling it.
- The editor drops them on every way into its buffer
  (`text_ops::clean_input`: opened text, restored draft, paste, a key
  event); tabs become spaces there.
- Opening a task that holds such characters and saving it untouched leaves
  the stored text as it was.

**Keys.**

- Ctrl+←/Ctrl+W go back one word (`text_ops::jump_prev_word`: separators,
  then one word, to the line start at most).
- Every edit ends the selection, an empty one included
  (`EditorState::delete_selection`).
- A Ctrl or Alt shortcut nothing is bound to does nothing
  (`input::types_text`; Ctrl+Alt types only on Windows, where it is AltGr).
- An edit is an undo step only when the text changed (`input::edit`).
- Under Ctrl a letter is its key in either case (Caps Lock on Windows).
  Ctrl+Shift+K deletes the line where the terminal reports the Shift
  (kitty's `CSI 107;6u`); elsewhere it is the byte of Ctrl+K.
- The editor does not switch the kitty keyboard protocol on: with it, a
  non-Latin layout reports Ctrl+S as the letter of that layout and no
  shortcut works.

**The date on the first line.**

- The first word of the buffer is the date, `_` the empty date, and a `_`
  word right after either is a mark that the word after it is text
  (`handlers::extract_leading_date`).
- `handlers::edit_prefill` puts the mark in front of a text whose first word
  (as the editor shows it, control characters dropped) reads as a date or
  is `_`, so the word stays text however the date is edited.
- `rusk add -d` on a restored draft that begins with `_` puts the date in
  its place (`handlers::with_seed_date`).
- A date alone on the first line gives no leading empty line.

**The view.**

- The editor's view is state of its own (`EditorState::view_top` +
  `follow_cursor`). The wheel moves only the view: cursor and selection
  stay, and the terminal cursor is hidden while it is out of sight. The
  next key, click or paste brings the view back to the cursor.
- Every frame clamps the view to the text, so a window that grows back
  shows text again.
- Clicks land on visible rows only; a drag past an edge reaches one row
  beyond, which scrolls the view.

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
| `unicode-width` | Display width of a character in terminal cells |
| `unicode-segmentation` | Grapheme clusters (the smallest unit a cut may fall between) |
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
    → formatter/editor/dialogs → output (out!/outln!) → stdout

rusk serve: browser ←→ tiny_http (web/server.rs) → auth → api.rs handlers
    → fresh TaskManager per request, one request at a time ←→ database

rusk sync: sync.rs → the ssh/http backends as transports
    → conflict check against the .sync state file
    → local Backend::save or remote replace
```

**The http backend.** curl runs with `-q` (no `~/.curlrc`). A stall of 30 s
ends it; a slow transfer does not. The `Authorization` header (a token, or
the URL's `user:password`) goes in a private file.

**Output.** Colors come from `config::theme`. Task text goes through
`printable::escape` first, so a control character in it is shown, not
obeyed.

**`rusk serve`.**

- tiny_http is vendored under `vendor/` with rusk's changes (see
  `vendor/README.md`). Each request is read, checked and answered on a
  thread of its own: a body up to 32 MiB, the sign-in form up to 4 KiB.
- Auth: any presented token (cookie, Bearer) matches; without a token only
  a loopback `Host`; a change from another origin by the browser's
  `Sec-Fetch-Site` is 403.
- The work on the tasks is done one request at a time (`server::DATABASE`:
  an ssh database has no lock of its own), against the database and the
  writer lock shared with CLI commands.
- Limits (`ServeOptions::timeout`, `web_timeout` in the config, a minute by
  default): a connection may wait the timeout, idle, for the rest of a
  request's head, or for a body or an answer to move. Past that a body or
  an answer keeps `server::MIN_RATE` (1 KiB/s) or the connection is closed,
  with 408 when part of a request had come. At most
  `server::MAX_CONNECTIONS` (256, fewer when the file descriptors are not
  to be had) are open at once; the next waits in the listening socket's
  backlog. A connection's requests are read one at a time, so the threads
  in flight are bounded by the same number. `web_timeout = 0` turns all of
  it off.
- An unread body up to `server::DISCARDED_BODY_BYTES` (1 MiB) is thrown
  away by its request before the answer. A bigger one, or a chunked one, by
  tiny_http after it, through a small buffer, and the answer says
  `Connection: close` when the next request could not be found in it.
- A request the server makes to reach an http database carries
  `X-Rusk-Serve: <its id>` (`crate::SERVE_ID`). One that comes back to the
  server itself (its `rusk_db` names its own address) is answered 508 at
  once, before anything else.

**`rusk sync`.**

- Each side is compared with what the `.sync` state file says it held after
  the last sync with this remote (keyed by its canonical form).
- After a local `Backend::save` or a remote replace, the hash of what that
  side holds now is recorded: its format's stored form of the list
  (`DbFormat::stored_form`: encoded, decoded, normalized), worked out rather
  than read back, which would take in another writer's change. A server
  reports its own in the `PUT` reply.
- `--force` needs only the source side. A target that reads is read: with
  the same tasks nothing is written, and its save checks staleness.

## Persistence

### Location and format

The database location comes from `$RUSK_DB`, `rusk_db` in the config, or
the default relative path.

- `location.rs` reads the value, the same way for all three sources and for
  `sync_remote`: `~` expanded, the scheme in any case, an http URL reduced
  to the server base, `[user@]host:path` as `scp` reads it, anything
  unknown refused rather than turned into a local path.
- `backend/mod.rs` picks the backend for it.
- The last extension of the file name, after the suffix of a
  `.backup`/`.before_restore` copy, picks the file format (`codec/mod.rs`).

### Atomic writes

SQLite saves run in a transaction. Every other local file rusk writes (the
database, `.backup`, `.before_restore`, `rusk gen -o`, the `.sync` state,
the editor draft) goes through `atomic.rs`: a uniquely named temp sibling
created with `create_new` (O_EXCL), fsync, rename, directory fsync.
Concurrent writers never share a temp file, a crash leaves the old or the
new content, and the mode of the replaced file is kept.

Two kinds of destination are told apart:

- **Paths the user named** (the database, `gen -o`). The user's own symlink
  is followed and stays a symlink (a dangling one, or one owned by another
  user, is an error). A device or FIFO is written into directly. Where a
  rename cannot replace the file (a bind mount) it is rewritten in place
  with a warning, never after a failure a rewrite would hit too (disk
  full). Only `gen -o` also rewrites in place when no temp file can be
  created next to the page.
- **Names rusk derives itself** (`.backup`, `.before_restore`, `.sync`,
  drafts). Whatever sits there may have been planted, so a symlink is
  replaced, never followed, and there is no in-place fallback.

### Backups, git and restore

- Local backends auto-backup to `<db>.backup` on every save. `backup =
  false` disables it; a zero-byte database is never copied over an existing
  backup.
- `git_backend = true` additionally commits every save to a git repository
  in the database directory (`backend::git`):
  - git runs with the repository's config, as a `git commit` there would,
    but no hook runs (hook scripts via `core.hooksPath`, config hooks by
    name, fsmonitor);
  - git does not run at all in a repository other users can change, since
    its config could run anything;
  - git's own identity is kept;
  - a repository is created only where git says there is none and none git
    would find is above (its ceilings and filesystem boundaries respected),
    never in the home directory, the root or a directory others can write
    to. The auxiliary files are excluded through `info/exclude`;
  - one `rev-parse` tells everything about the repository.
- `rusk restore` reads the backup strictly and read-only (an empty file,
  unrecognized content or a SQLite file without a `tasks` table is
  refused), keeps a copy of the current database in
  `<db>.before_restore[.N]` (never overwriting an earlier copy) and then
  puts the backup in place: a file atomically and byte for byte, a separate
  commit with `git_backend`, SQLite through a transaction.

### SQLite

- A SQLite database is copied by SQLite, not by its bytes. `.backup` and
  the `.before_restore` of a database that loads are the image
  `sqlite3_serialize` makes inside a transaction (what is committed only in
  a WAL is in it; the header is switched to the rollback journal, so the
  copy is read on its own), written through `atomic.rs` with the mode of the
  database. Only a file SQLite rejects is kept byte for byte, with its
  journal/WAL next to it.
- Reading a SQLite database changes nothing in it: the file is opened
  without being created, no table is created and no column added, a
  write-protected file reads.
- A file without tables is an empty database. One with tables but no
  `tasks` is another program's and is refused on read and on write.
- The save that needs it brings an older table up to date inside its
  transaction (`after` added; the one-byte `CHECK (id BETWEEN 1 AND 255)`
  table made anew, its rows being replaced right after anyway) and marks
  the header: the `application_id` is "rusk" in ASCII, `user_version` the
  schema of `SCHEMA` (1).
- A file whose header names another application, or a later schema, is
  refused on read and on write. One without an application id is taken on
  its tables (an older rusk wrote none), and a `user_version` somebody set
  without one is left as it is.
- Every connection sets `secure_delete`, so a deleted text is overwritten
  in the file and in every copy made of it. A save that leaves a quarter of
  the file free runs a `VACUUM`, skipped at once when another process is
  reading.

### Normalization

Every backend implements the same whole-database load/save contract, so the
formats stay interchangeable and `rusk sync` hashes content canonically
(parsed tasks re-encoded as compact JSON) regardless of representation.

A decoder only maps the stored form to records, as written: an item without
an id comes back with id 0, one without text with an empty text. Every list
a backend reads (a load, the re-read inside `update`, a `.backup` for
`restore`, the answer of `GET /api/tasks`, a remote file) then passes
`backend::normalized` → `model::normalize`, so no other code ever sees a
list that breaks the rules:

- A record with neither text nor id is no task and is left out.
- One with an id but no text keeps everything else and gets the text
  `(no text)` (`model::NO_TEXT`, so that `edit`/`del` can reach it).
- The first task with an id keeps it; a later one with the same id, and one
  without an id, get the lowest free ids, in list order.
- `after` keeps only other tasks that exist, each once. Dependencies are
  settled against the ids the list itself gives before any id is handed
  out, so a dangling one never comes to mean the task that gets the free
  id.
- The result is deterministic (the same file gives the same ids on every
  run until a save writes them), and a list that follows the rules passes
  unchanged.
- What changes a task (a new id, a task without text, a skipped item, a
  dependency on a task that is not there) is reported on stderr, once per
  process. Ids for items added by hand, and self or repeated dependencies,
  are routine and silent.
- The file itself changes only with the next save.
- `PUT /api/tasks` stores a list exactly as sent (sync compares what it
  pushed with what it reads back), so there the same rules are a check: a
  list `normalize` would change is refused with 400.
- In JSON only `text` is required; any other field may be absent or `null`.

The encoders keep their side of it: what rusk writes decodes to the same
tasks (todo.txt marks a word of the text that would read as metadata with a
`\`; Markdown never takes a text's only word for its date), so a list rusk
wrote needs no repairs.

### What the encoders keep

A save replaces the whole file, and one format needs to know what it
replaces: iCalendar clients know a task by its UID and see a new DTSTAMP as
a change.

- A backend hands the encoder the content it is replacing
  (`DbFormat::encode_replacing`; the file backend reads it under the writer
  lock, the ssh backend keeps what it fetched).
- `codec::ics` keeps each task's UID (by id) and, for an unchanged task, its
  DTSTAMP from there. A task new to the file gets
  `rusk-<id>@<hash of id and text>.rusk`: not the UID of a deleted task
  that had the id (unless one save both deletes and reuses it), the same in
  a local file and its sync remote, and readable by rusk up to 0.7.3, which
  takes the digits before the `@`.
- The other formats have nothing to keep.
- Markdown writes a text as it is and takes off exactly what it writes
  around it (the one space before the date token and the id comment), so
  spaces, an empty first line and the `! ` in front of it come back as they
  were. Numbered items and one-character checkbox marks at the start of a
  line are tasks, written back as `- [ ]`/`- [x]`. An indented line is a
  new item only as `- [ ]`/`- [x]`, so the continuation lines of a text
  stay text.
- A broken NDJSON line is named by its line and column in the file. The
  report of a broken JSON file shows at most 80 characters of each line
  around the error (`backend::file::clip_line`), a minified database
  included.

### Missing, empty, unreadable

Three different answers, never folded into one, because the contract
replaces the whole database: a database read as "no tasks" is one save away
from being no tasks.

- **Missing.** Only "nothing exists at that path" is a database that is not
  there yet. Locally the read itself says so (ENOENT with no symlink at the
  path). Over ssh a small script does: `cat` answers whenever there is
  anything to read or to complain about, and only a path that holds
  nothing, in a directory that can be searched, exits with the status that
  means "not there". So no banner or `echo` in a login rc file can be
  mistaken for content, and no failing `cat` for an absence. The script
  runs under `sh` and the path travels as its argument, which leaves the
  remote login shell (fish, csh, whatever the user has) one level of
  quoting to undo, the one level every shell agrees about.
- **Unreadable.** Everything that exists but cannot be read is an error and
  stops the command: no permission on the file or its directory, a
  directory in its place, an I/O failure, bytes that are no UTF-8 (never
  decoded lossily: U+FFFD in place of what could not be read is what the
  next save would write), a symlink whose target is gone. Local and remote
  answer the same way.
- **Empty.** A file that is there and holds nothing is a third case. Every
  format decodes blanks to zero records, and only the formats that write an
  empty database as an empty file (Markdown, todo.txt, NDJSON) count that
  as a stored state. Elsewhere it is what an interrupted write or a full
  disk leaves behind, so it loads as zero tasks with a warning that the
  next save replaces it, and as a `.backup` it is refused outright.
- A UTF-8 BOM is stripped once for every format (`codec::content`), before
  any decoder: a database saved by a Windows editor is still a database.

`rusk sync` is where the difference decides something. Both sides report
which of the three they are (`backend::Loaded`, `sync::Side`), and a
fast-forward that would replace tasks with nothing is refused without
`--force` whichever the reason. The message says whether the side does not
exist or holds no tasks, so the answer to "where did my tasks go" is in it.

### Concurrent writers

The contract replaces the whole database, so a writer must never put back a
list that is older than what is there. Each backend remembers what it
loaded (`revision.rs`: a fingerprint of the file bytes; SQLite's
`data_version`; the `ETag` of `GET /api/tasks`) and checks it right before
writing:

- `Backend::save` is a compare-and-swap: if the database changed since the
  load it writes nothing and fails with `StaleDatabase`. A backend that
  never loaded (tools writing a fresh list) replaces unconditionally.
- `Backend::update` is the read-modify-write every command uses through
  `TaskManager::update`: the change is a closure over the task list. While
  the database still is what the caller loaded, it runs on the caller's
  list; otherwise the database is read again and it runs on that. Ids are
  allocated, dependencies validated and tasks looked up (by id) inside the
  closure, that is, against the current state. A closure that changes
  nothing writes nothing (no rewrite, no backup rotation, no directory
  created). A list that carries edits made directly in memory is never
  swapped for a fresh read (`StaleDatabase` instead), and such edits count
  as saved only once they were written (`Updated::stored`).

Check and write are one step:

- A file database is written under an exclusive advisory lock on
  `<db>.lock` (`lock.rs`): an empty file that stays in place, released by
  the OS with the process, skipped with a warning after 10 s or where
  locking is unsupported (the content check still applies).
- SQLite uses `BEGIN IMMEDIATE`, and notices when the path was made to name
  another file than the one its kept connection has open. The `.backup`
  copy is SQLite's own image of the database, made through the connection
  that holds the write lock: no other descriptor of the file is opened
  (closing one would drop the POSIX locks of the process on it, SQLite's
  write lock included).
- The http backend sends `If-Match` and starts over on 412, with a random
  growing pause.
- ssh has neither lock nor compare-and-swap: content read more than ~2 s
  ago (an editor or prompt sat in between) is read again and compared
  before the write.
- The `.backup` rotation and the git commit happen inside that step, after
  the staleness check, so a refused save leaves the backup alone.

Interactive commands add their own condition, because the user decided
looking at a snapshot:

- `rusk edit` stores an editor session only if the task still has the text
  and date the editor showed (`TaskChanged` otherwise; the typed text goes
  back into the draft), one task at a time, right after Ctrl+S.
- `rusk del` deletes only tasks whose text still is what the prompt showed.

### The web API and the page

- The web API runs every request through `TaskManager::update` and honours
  `If-Match` (412): against the revision of the list for requests on the
  list, against the revision of the task (or of the list) for requests on
  one task (`GET /api/tasks/{id}` serves it as `ETag`).
- The web UI computes the task revision itself from the JSON it was served
  (`taskTag` in `template.html`, the same FNV-1a as
  `revision::task_revision`) and sends it with every PATCH/DELETE. That
  keeps a stale page from touching a task that was changed elsewhere or
  whose id was reused, while changes to other tasks do not get in its way.
- Everything the page changes goes through one `mutate`: changes run one
  after another; a row (or the add form) with a change under way ignores
  further taps until it is re-rendered; the control that started a change
  is reset only once the server has taken it.
- The edit dialog stays open (and cannot be closed) with what was typed,
  shows a refusal or a lost connection inside itself (a modal dialog covers
  the banner), and after a 412 moves its untouched fields on to the task as
  it is now and pauses Save/Delete for a moment, so that a burst of taps
  under way does not run on into the task that is there now.
- Save sends only the fields the user changed, compared with what the form
  controls made of the task (`asShown`), not with the task itself: a field
  that was only looked at is never sent back.
- Every request has a time limit (a request that is never answered would
  otherwise hold up the whole queue in silence). Every answer is checked
  for its shape before it counts as a success. A failure reported in the
  banner survives the success of a change queued after it, and a change
  that went through but whose re-read failed is reported as saved.

### Errors

- Errors are `anyhow` chains and are printed whole (`{err:#}` in `main`, in
  the JSON error bodies of the web API, and wherever one error is folded
  into the message of another). The outermost context names the step and
  the location; the cause under it says what is wrong (the OS error, what
  `curl` or `ssh` wrote to stderr, SQLite's message). A context therefore
  never repeats its cause in its own text.
- SQLite failures go through one function (`explain` in
  `backend/sqlite.rs`), which names what the user can act on, whichever
  call happened to hit it: locked, read-only, cannot be opened, not a
  database (with the way out: `rusk restore`; for the backup itself the
  same condition is a refusal to restore from it).

### Task ids and dependencies

- Task ids are `u32` (`TaskId` in `model.rs`): the smallest free id is
  reused.
- A task may list other task ids in `after` (`--after`). The dependencies
  are shown after the text as `(19,22)` and are stripped when the
  referenced tasks are deleted: `rusk del` names the dependents in its
  question and says whose lists it changed (`storage::strip_deps` returns
  them). For lists edited outside rusk they are stripped when the list is
  read (see [Normalization](#normalization)).
- A list is a set: the same ids in another order are no change
  (`storage::deps_change`).
- The ordering is advisory, a hint for agents and tooling. `rusk mark` is
  never blocked.

## Output

- What a command prints goes through `output` (`out!` / `outln!`), never
  `println!`, which panics when a write fails. A failed write is an error
  like any other: a reader that stopped reading is `output::Closed`, on
  which `main` exits quietly with 0 (`rusk list | head`); anything else (a
  full disk) is reported, exit 1.
- A command changes what it changes first and reports it after, so an
  output that fails never leaves a change half done. Where a message has to
  come on the way (`restore`'s copy of the current database, the `serve`
  banner), its failure is ignored. Messages on stderr (`errln!`) cannot fail
  the command.
- Colors are decided once at start-up (`output::Colors::decide`), for
  `colored` and for clap (`ColorChoice::Always`/`Never`: clap paints
  `--help` and argument errors on its own and would otherwise decide per
  stream by rules of its own). `RUSK_NO_COLOR`, a non-empty `NO_COLOR` or
  `no_color = true` turn them off, and nothing turns them back on for the
  run. Otherwise `CLICOLOR_FORCE` (not empty, not `0`) turns them on,
  `CLICOLOR=0` or `TERM=dumb` off, and a terminal on stdout decides.
- The list is laid out by `HandlerCLI::format_task_list`, a pure function
  of the tasks and the width. Wrapping (`wrap_rows`) yields rows as byte
  ranges of the escaped text, so a search match (a byte range from
  `search::Query::find_all`) is painted wherever its words land: across a
  row break, in both pieces of a cut word, over the one space printed for a
  run of whitespace.
- `search` folds case per character on both sides (ς and σ, ß and ss),
  matches any run of whitespace with any other, and begins and ends a match
  between grapheme clusters only. Accents are not folded.
- The compact row (`compact_row`) is the first row of the first line with
  sentence punctuation cut (not quotes: a closing one belongs to an opening
  one), and ends in `…` when the task goes on.
- Reports on a task start with `<what>: <id>:` and the text below. `mark`
  names what its flag became (done, undone, priority, priority removed).
  `edit` prints dates as the list does (`1-jan-27`) and "cleared" only when
  a value went away.
- In a list in id order a new task is put in front of the first task with a
  higher id (`storage::insert_by_id`), so an id reused after a delete is
  listed in its place. A list in an order of its own gets it at the end.
- Task text is stored without whitespace at its edges
  (`storage::clean_text`: CLI, editor and web API alike), and a new text
  that differs from the stored one only there is no change
  (`storage::text_changes`).
- Every interactive part (the editors, the `del` prompt) needs a terminal
  on stdin and stdout (`HandlerCLI::on_a_terminal`) and says so without
  one.

## Configuration

- `config.rs` parses a hand-rolled `key = value` format (see CONFIG.md): no
  TOML dependency, user-defined variables, never-fatal warnings with line
  numbers.
- The parsed `Config` lives in a process-wide `OnceLock`
  (`config::config()` / `config::theme()`), initialized once in
  `main::run`. Library and unit-test use falls back to defaults.
- Test/debug isolation mirrors the database rules: the default config path
  is only read by release binaries outside test mode, and `RUSK_CONFIG`
  overrides everywhere (empty = disabled).
- Test mode itself (`is_test_mode`: cargo's test variables, "test" in the
  name of the binary) never applies to the `rusk` command built for release
  (`main` calls `rusk::running_as_the_command()` first): that binary is the
  user's rusk whatever its name and environment. The library linked into a
  test binary is in test mode in any profile, so `cargo test --release`
  does not reach the user's database either. Tests that need test mode in
  the binary itself are `#[cfg(debug_assertions)]`.
- The file is read as bytes (`config::parse_bytes`): a BOM in front of a
  key is dropped, and bytes that are no UTF-8 cost their line only when they
  are in its key or value.
- A value is bare or quoted. In a bare value a comment starts at a `#`
  after whitespace, or at the `#`s that open the value when whitespace or
  nothing follows them (`key = # note` is an empty value). A quoted value
  is literal up to the next `"`, with nothing but a comment after it. A
  quote left open is an error for that line, and the warning does not echo
  the value (it may be a token).
- The bare `default` resets a setting to its built-in value
  (`reset_setting`), whatever an earlier line set. A variable named
  `default` shadows it like any variable.
- Warnings carry their line number and are sorted by it. The one about
  `git_backend = true` in a build without git is made after the last line,
  of the value the file ends with.

### Dates

- Dates are held to four-digit years (`model::YEARS`, 1000–9999) wherever
  they come in: the CLI parser, the editor (which also checks the shape of
  the year: two digits or four), and the web API (`model::parse_iso_date`:
  exactly `YYYY-MM-DD`; an error names the value, apart from JSON that does
  not parse).
- What is stored is read as chrono reads it, in every codec and in SQLite:
  a date outside the years is kept, and `model::normalize` reports it
  (`Repairs::out_of_range`), so a database an older rusk wrote stays open
  to correct it. The editor keeps a task's own date when its token is left
  untouched. A `PUT` refuses a list with one, as it refuses every list a
  load would repair.
- A two-digit year on the command line is 20xx. The list shows `D-mon-yy`
  for 2000–2099 and all four digits otherwise (`1-jan-0205`).
- Relative offsets add all months (m, q, y) in one step, then the days (d,
  w), so the end of a short month is applied once (`+1m1m` is `+2m`). A sum
  past what a date can hold is the same out-of-range error as a year past
  9999.

## Shell completion

- The five scripts in `completions/` (embedded by `completions.rs`) read
  the tasks through the hidden `rusk list --for-completion-lines`: one line
  per task, `<id>\t<text>`, with `\\`, `\n`, `\r` and `\t` escaped in the
  text (`handlers::completion_line`). No line of a text can pass for a
  task. `printf %b` (bash, zsh, fish), `[regex]::Unescape` (PowerShell) or
  a split at `\\` (nu) gives the text back. A text with a NUL, which no
  argument can carry, is listed empty and offered nowhere.
- `rusk edit <id><TAB>` puts the text on the line so that running it stores
  the very same text. bash, zsh and PowerShell single-quote it (PowerShell
  doubling its typographic quotes too), nu uses single quotes or a raw
  string `r#'…'#`, whenever the text holds a special character, a control
  character or whitespace other than single spaces between words. A text
  that starts with `-` goes after `--`.
- fish escapes every candidate itself and inserts one token per Tab, so
  there the id completes first and the next Tab gives the text (`--` first
  when needed, and one more Tab for the space fish does not add after a
  `-`). A text with a tab or a leading `~`, which fish cannot insert as it
  is, is not offered.
- No script binds keys.
- The hidden `--for-completion` keeps the listing of rusk 0.7.3 (further
  lines of a text unmarked) byte for byte: scripts installed by an older
  rusk stay behind when the binary is upgraded, and would misread the
  escaped form.
