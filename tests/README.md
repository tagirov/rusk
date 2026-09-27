<h1 align="center" id="rusk-tests">Rusk Tests</h1>
<br />

This directory contains comprehensive unit and integration tests for the rusk task management application. These tests validate core functionality, edge cases, data persistence, and CLI behavior.

## Structure

```
tests/
├── README.md                       # This file
├── common/                         # Shared test utilities (Sandbox, the pty driver for the editor)
│   ├── mod.rs                      # Test tasks, the per-test Sandbox, Sandbox::in_pty
│   └── pty_driver.py               # Drives rusk in a pseudo-terminal (python pty) for the editor tests
├── completions/                    # Shell completion tests (see completions/README.md)
│   └── ...
├── cli_tests.rs                    # CLI command tests (TaskManager API)
├── cli_utils_tests.rs              # CLI utility function tests (wrap, trim, word nav)
├── lib_tests.rs                    # Core library function tests
├── database_corruption_tests.rs    # Database corruption handling tests
├── directory_structure_tests.rs    # Directory creation, default path, RUSK_DB in test mode
├── edge_case_tests.rs              # Edge cases and boundary condition tests
├── edit_parsing_tests.rs           # Edit command argument parsing tests
├── integration_main_tests.rs       # Integration tests: real `rusk` binary, flags, RUSK_DB harness
├── mark_success_tests.rs           # Mark command success/failure tests
├── id_list_tests.rs                # Task id lists on the command line (parse_id_list, split_leading_ids, parse_id_args, parse_edit_args)
├── persistence_tests.rs            # Data persistence and save/load tests
├── restore_tests.rs                # Backup restore functionality tests
├── review_urgent_tests.rs          # Regression tests for the urgent REVIEW.md fixes (№1, №6, №7, №154, №161)
├── review_known_bugs.rs            # REVIEW.md repros by refactor cluster: #[ignore]d while open, regressions once closed (R1–R20, R24)
├── unchanged_detection_tests.rs    # Unchanged task detection tests
└── completions.rs                  # Completion test entry point
```

## Running Tests

### All Tests
Run all tests in the project:
```bash
cargo test
```

### Specific Test File
Run tests from a specific file:
```bash
cargo test --test cli_tests
cargo test --test cli_utils_tests
cargo test --test lib_tests
cargo test --test integration_main_tests
cargo test --test persistence_tests
```

### Specific Test Function
Run a single test function:
```bash
cargo test test_add_task
cargo test test_mark_tasks
```

### With Output
Run tests with output from passing tests:
```bash
cargo test -- --nocapture
```

### Filter Tests
Run tests matching a pattern:
```bash
cargo test edit
cargo test persistence
```

## Test Categories

### Core Functionality Tests

#### `lib_tests.rs`
Tests for core library functions:
- ID generation (`generate_next_id`)
- Task management operations
- Task filtering and querying
- Date handling: absolute (DD-MM-YYYY), relative from today (e.g. 2w, 10d5w), leading `+` on edit for offset from the task's current due date (e.g. +1w), first-line due date in interactive `rusk edit`, optional `rusk edit <id> -d <date>` (bare `-d` is invalid; see `rusk edit --help`)
- Task validation

#### `cli_tests.rs`
Tests for CLI command behavior via `TaskManager` (no subprocess):
- `add` command - Adding tasks with and without dates
- `edit` command - Editing task text and dates
- `mark` command - Marking tasks as done/undone
- `del` command - Deleting tasks
- `list` command - Task data used by listing (no TTY output assertions here)
- `restore` command - Restoring from backups

#### `cli_utils_tests.rs`
Tests for CLI utility functions:
- Text wrapping by words (`wrap_text_by_words`)
- Output formatting helpers
- Other CLI helper functions

#### `integration_main_tests.rs`
Integration tests for the rusk binary (`RUSK_DB` set to a temp JSON file):
- Main argument parsing and flag filtering
- `list -c` / `--compact` omits body lines after the first line of task text
- `RUSK_NO_COLOR` / empty value behavior on stderr
- `--help` text mentions date syntax (absolute, relative, and edit `+` from current due date)

### Data Persistence Tests

#### `persistence_tests.rs`
Tests for data persistence:
- Saving tasks to disk
- Loading tasks from disk
- Mark operation persistence
- Date persistence
- Task state persistence across sessions

#### `database_corruption_tests.rs`
Tests for handling corrupted database files:
- Invalid JSON handling
- Trailing content detection
- Error message clarity
- Recovery mechanisms
- An empty file is no corruption and no database either (R5): it reads as
  zero tasks, with a warning that the next save replaces it

#### `restore_tests.rs`
Tests for backup and restore functionality:
- Backup file creation
- Restore from backup
- Backup file naming conventions
- Restore error handling

### Path and Environment Tests

#### `directory_structure_tests.rs`
Tests for directory structure:
- Default directory creation
- Custom directory paths
- Directory creation on save
- Backup file location
- Debug/test build path (`rusk_debug`) and interaction with `RUSK_DB`

### Edit Command Tests

#### `edit_parsing_tests.rs`
Tests for edit command argument parsing:
- ID extraction
- Text extraction
- Date changes when editing with an explicit date argument (non-interactive `handle_edit_tasks`)
- Unchanged task detection
- Save behavior optimization

### ID Parsing Tests

#### `id_list_tests.rs`
Task id lists on the command line (`src/parser/ids.rs`):
- One comma-separated list (`1,2,3`), blanks and empty parts tolerated, repeats dropped
- Words glued back across a comma when the shell split them (`1, 2, 3`)
- A word without a comma boundary ends the list (`edit 3 1,000 units` — text)
- `mark` / `del` reject a second word; every command rejects a part that is not an id
- `edit`: numbers alone after the list read as more ids (`edit 1,2 3` is an error);
  the words after `--` are text as they are

### Edge Cases and Validation

#### `edge_case_tests.rs`
Tests for edge cases and boundary conditions:
- Empty input handling
- Whitespace-only input
- Special character handling
- Very long task text
- Date validation
- Invalid date formats
- Task ID boundaries

#### `unchanged_detection_tests.rs`
Tests for unchanged task detection:
- Detecting when task text hasn't changed
- Detecting when task date hasn't changed
- Optimizing save operations
- Preventing unnecessary file writes

#### `mark_success_tests.rs`
Tests for mark command success/failure reporting:
- Marking tasks as done
- Unmarking tasks (marking as undone)
- Handling already-marked tasks
- Not found task handling
- Return value correctness

## Test Utilities

### `common/mod.rs`
Shared helper functions for tests:
- `create_test_task(id, text, done)` - Create a test task
- `create_test_task_with_date(id, text, done, date)` - Create a test task with date
- `Sandbox` - a private world for one test that spawns the `rusk` binary: its
  own temp dir, database, HOME and config. `Sandbox::with_db(json)` seeds the
  database, `sb.cmd()` returns a `Command` bound to the sandbox,
  `sb.read_db()` / `sb.write_db()` / `sb.db_path()` inspect it, and
  `sb.cmd_with_fake_ssh()` (unix) puts a local `ssh` shim first in PATH so ssh
  locations can be tested without sshd or network

Every test that runs the binary must go through a `Sandbox`. Debug and
test-mode binaries ignore `RUSK_DB` and pin the database to
`$TMPDIR/rusk_debug/tasks.json`; the sandbox points `TMPDIR` (and `RUSK_DB`,
for release binaries) at its own directory and forces test mode on the child,
so a debug binary never seeds its demo tasks into the test database.

Usage:
```rust
mod common;
use common::create_test_task;

#[test]
fn my_test() {
    let task = create_test_task(1, "Test task", false);
    // ...
}
```

## Test Coverage

The test suite covers:

- All CLI commands and their aliases
- Core library functions
- Data persistence and file I/O
- Error handling and edge cases
- Path resolution (default `.rusk`, `RUSK_DB`, debug harness)
- Backup and restore functionality
- Date parsing and validation
- ID parsing (flexible formats)
- Task state management
- Database corruption handling

## Adding New Tests

To add a new test:

1. Choose the appropriate test file or create a new one if testing a new feature area
2. Use helper functions from `common/mod.rs` when creating test data
3. Follow existing test patterns for consistency
4. Use descriptive test names starting with `test_`
5. Test both success and failure cases
6. Use `tempfile` for temporary directories when testing file operations

Example:
```rust
use rusk::TaskManager;
mod common;
use common::create_test_task;

#[test]
fn test_new_feature() {
    let mut tm = TaskManager::new_empty().unwrap();
    // ... test implementation
}
```

## Integration with CI/CD

These tests are designed to run in CI/CD pipelines:

```yaml
# Example GitHub Actions step
- name: Run tests
  run: cargo test --all-features

# With coverage
- name: Run tests with coverage
  run: |
    cargo test --all-features
    cargo test --test completions
```

## Notes

- Tests use temporary directories for file operations to avoid affecting user data
- Some tests require specific environment setup (see individual test files)
- Tests are designed to be run in parallel (use `cargo test --test-threads=1` if needed)
- Tests never share a database: binary tests use a per-test `Sandbox`, library
  tests use `tempfile` dirs or `TaskManager::new_empty()` (a unique path per call)
- The test process environment is never mutated (`std::env::set_var` races with
  parallel tests); environment variables are set on the spawned child instead
- `review_known_bugs.rs` holds failing repros of open findings behind `#[ignore]`:
  `cargo test --test review_known_bugs -- --ignored` shows them; when a fix
  lands, drop the `#[ignore]` of the tests it turns green. The tests of a
  closed cluster (so far R1 error causes, R2 argv of edit/mark/del, R3 the
  rules every loaded list is held to, R4 text measured in terminal cells, R5
  "cannot read" is not "zero tasks", R6 the terminal comes back, R7 task text
  does not drive the terminal, R8 the draft
  lifecycle, R9 restore, R10/R13 reading a location value, R11 quoting in
  the completion scripts, R12 lost updates, R14 atomic writes, R15 the ssh
  protocol, R16 web UI, R18 the SQLite connection, R19 output, terminals
  and messages, R20 `rusk serve`, R24 the editor's keys) stay in the file as
  its regression suite.
  R20 starts `rusk serve --port 0` (`serve()`, which reads the port from the
  first line and kills the server when the test ends) and speaks HTTP/1.1 to
  it over one keep-alive connection (`Http`, chunked bodies put together);
  a server that cannot accept runs under `ulimit -n`. Its pure parts —
  hosts, tokens, cookies, bodies, the one-at-a-time work on the tasks — are
  unit-tested in `src/web/server.rs` through `tiny_http::TestRequest`.
  R24 drives the editor itself: `Sandbox::in_pty` (`tests/common`) runs the
  binary in a pseudo-terminal through python's `pty` (`tests/common/pty_driver.py`,
  which answers the terminal queries — as kitty, when asked — waits for a
  marker on the screen, the alternate screen or a prompt, and types the keys);
  the tests are skipped without python3 or without a pty to be had. The
  driver runs without `DISPLAY` and `WAYLAND_DISPLAY`, so the editor never
  reaches the desktop's clipboard. A draft is put in place as the file the
  editor writes (`rusk_debug/editor-new-task.draft`). Its pure parts are unit-tested in `src/cli/editor/` (`text_ops`,
  `state`, `input`) and `src/cli/handlers.rs`.
  R19 closes stdout under a command with `std::io::pipe` (reader dropped
  before the spawn, so every write gets EPIPE), writes it to `/dev/full`, and
  takes a terminal away with `setsid` (no controlling terminal for crossterm to
  fall back on); its pure parts are unit-tested in `src/output.rs` (the color
  decision, closed-pipe errors), `src/search.rs` (folding, byte ranges) and
  `src/cli/formatter.rs` (rows as ranges, highlights, the compact row). What
  needs a pty — an empty `NO_COLOR` on a terminal, clap's colors, the `del`
  prompt, the editor's report after a failed save — was checked against a
  release binary (see the R19 status in REVIEW.md).
  R3's rules themselves are unit-tested in `src/model.rs` (`normalize`), the
  CSV rows and lines in `src/codec/csv.rs`. R4's measuring is unit-tested in
  `src/width.rs` and its editor half in `src/cli/editor/view.rs`; what a
  terminal receives from the editor (cursor column, click target, expanded
  tabs) was checked in a pty, see the R4 status in REVIEW.md. R2's completion tests drive each installed
  shell (bash, zsh, fish, nu, pwsh; a missing one is skipped) with a fake
  `rusk` first on PATH and without the user's shell config; R11's do the same
  and then run the completed line through that shell, so what counts is the
  text the fake `rusk` receives (the `--for-completion-lines` encoder is
  unit-tested in `src/cli/handlers.rs`; fish's Tab-by-Tab insertion was
  checked in a pty, see the R11 status in REVIEW.md). R18 drives the SQLite
  backend through the library (a debug binary is pinned to a JSON file) and
  its pure parts in `src/backend/sqlite.rs`; the R14 write
  path itself is unit-tested in `src/atomic.rs`, the writer lock in
  `src/lock.rs`. R5's rules are unit-tested per format in `src/codec/mod.rs`
  (a BOM belongs to no format, blanks are a stored state only where rusk
  writes an empty database as an empty file) and in `src/transport.rs` (the
  remote read command tells a missing file from a failing `cat`); its ssh
  half runs black-box against the fake `ssh` shim of `tests/common`
- Error-message tests print the whole chain (`format!("{err:#}")`), the way
  `main` and the web API do; `to_string()` is the outermost context only
- The interactive editor (clusters R6, R8 and R17; the editor half of R7)
  has no in-repo harness: its
  repros need a pty, and the parts that matter — a terminating signal, a
  panic, a restored draft, `Ctrl+C` — cannot be driven from `Command`. What
  Rust can check lives here (the TTY refusal, and the draft unit tests in
  `src/cli/editor/draft.rs`); the rest is driven by a `pty.fork` script
  against a **release** binary (see the R6/R8 and R7/R17 status in
  REVIEW.md). R7's escaping is unit-tested in `src/printable.rs` and the
  editor's input filter in `src/cli/editor/text_ops.rs`; R17's view rules
  (size fallback, view clamping, footer hint, click reach) in
  `src/cli/editor/view.rs` and `mouse.rs`.
- The page script of `rusk serve` (`src/web/template.html`, cluster R16) has
  no in-repo harness: the API under it is covered by `web_tests.rs` and the
  unit tests of `src/web/api.rs`, the page itself was checked black-box in
  headless Chromium (see the R16 status in REVIEW.md)
- Concurrency tests come in three kinds: threads of one process (cheap, but
  SQLite arbitrates those in memory), processes of the `rusk` binary (file
  databases only: a debug binary ignores `RUSK_DB`), and — for SQLite across
  processes — the test binary starting itself as a worker
  (`r12_sqlite_worker`, a no-op unless `RUSK_R12_WORKER_DB` is set)
- `web_tests.rs` talks to a real `rusk serve`; the http database backend is
  tested against it through the library and needs `curl` (skipped without it)
- Completion tests are in a separate directory (`completions/`) with their own README

<br />
<p align="center"><a href="#rusk-tests">Back to top</a></p>
