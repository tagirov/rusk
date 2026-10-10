<h1 align="center" id="rusk-tests">Rusk Tests</h1>
<br />

Unit and integration tests for rusk: core functionality, edge cases, data
persistence and CLI behavior.

- [Structure](#structure)
- [Running the tests](#running-the-tests)
- [Test files](#test-files)
- [Test utilities](#test-utilities)
- [Where the review clusters are tested](#where-the-review-clusters-are-tested)
- [Adding new tests](#adding-new-tests)
- [CI](#ci)
- [Notes](#notes)

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
├── config_tests.rs                 # The config file: parsing, variables, theme, warnings, RUSK_CONFIG
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
├── review_known_bugs.rs            # REVIEW.md repros by refactor cluster: #[ignore]d while open, regressions once closed (R1–R30)
├── unchanged_detection_tests.rs    # Unchanged task detection tests
├── web_tests.rs                    # `rusk serve` over HTTP: pages, the API, auth, sync against it
└── completions.rs                  # Completion test entry point (tests/completions/rust/)
```

## Running the tests

```bash
cargo test                                          # everything
cargo test --test cli_tests                         # one test file
cargo test test_add_task                            # one test function
cargo test edit                                     # every test whose name matches
cargo test -- --nocapture                           # with the output of passing tests
cargo test --test review_known_bugs -- --ignored    # the repros of open review findings
```

## Test files

### Core functionality

- **`lib_tests.rs`**: core library functions. Id generation
  (`generate_next_id`), task management operations, task filtering and
  querying, task validation. Date handling: absolute (DD-MM-YYYY), relative
  from today (`2w`, `10d5w`), a leading `+` on edit for an offset from the
  task's current due date (`+1w`), the first-line due date in interactive
  `rusk edit`, the optional `rusk edit <id> -d <date>` (a bare `-d` is
  invalid; see `rusk edit --help`).
- **`cli_tests.rs`**: CLI command behavior via `TaskManager` (no
  subprocess): `add` with and without dates, `edit` of text and dates,
  `mark` done/undone, `del`, the task data used by `list` (no TTY output
  assertions here), `restore` from backups.
- **`cli_utils_tests.rs`**: CLI utility functions: text wrapping by words
  (`wrap_text_by_words`), output formatting helpers, other helpers.
- **`integration_main_tests.rs`**: the rusk binary with `RUSK_DB` set to a
  temp JSON file. Argument parsing and flag filtering; `list -c` /
  `--compact` omits the body lines after the first line of task text;
  `RUSK_NO_COLOR` / empty value behavior on stderr; the `--help` text
  mentions the date syntax (absolute, relative, and edit `+` from the
  current due date).

### Data persistence

- **`persistence_tests.rs`**: saving and loading tasks, mark and date
  persistence, task state across sessions.
- **`database_corruption_tests.rs`**: corrupted database files: invalid
  JSON, trailing content, error message clarity, recovery. An empty file is
  no corruption and no database either (R5): it reads as zero tasks, with a
  warning that the next save replaces it.
- **`restore_tests.rs`**: backup file creation, restore from backup, backup
  file naming, restore error handling.

### Paths and environment

- **`directory_structure_tests.rs`**: default directory creation, custom
  directory paths, directory creation on save, backup file location, the
  debug/test build path (`rusk-<uid>/debug`, the user's own) and its
  interaction with `RUSK_DB`.

### Command arguments

- **`edit_parsing_tests.rs`**: edit command argument parsing: id
  extraction, text extraction, date changes with an explicit date argument
  (non-interactive `handle_edit_tasks`), unchanged task detection, save
  behavior optimization.
- **`id_list_tests.rs`**: task id lists on the command line
  (`src/parser/ids.rs`). One comma-separated list (`1,2,3`), blanks and
  empty parts tolerated, repeats dropped; words glued back across a comma
  when the shell split them (`1, 2, 3`); a word without a comma boundary
  ends the list (`edit 3 1,000 units` is text); `mark` / `del` reject a
  second word, and every command rejects a part that is not an id; for
  `edit`, numbers alone after the list read as more ids (`edit 1,2 3` is an
  error), and the words after `--` are text as they are.

### Edge cases and validation

- **`edge_case_tests.rs`**: empty and whitespace-only input, special
  characters, very long task text, date validation, invalid date formats,
  task id boundaries.
- **`unchanged_detection_tests.rs`**: detecting that a text or a date has
  not changed, so that no unnecessary file write happens.
- **`mark_success_tests.rs`**: mark command success/failure reporting:
  marking done, unmarking, already-marked tasks, not-found tasks, return
  values.

### Review regressions and the web

- **`review_urgent_tests.rs`**: regression tests for the urgent REVIEW.md
  fixes (№1, №6, №7, №154, №161).
- **`review_known_bugs.rs`**: REVIEW.md repros by refactor cluster,
  `#[ignore]`d while open and regression tests once closed (see
  [below](#where-the-review-clusters-are-tested)).
- **`web_tests.rs`**: `rusk serve` over HTTP: pages, the API, auth, sync
  against it. It talks to a real `rusk serve`. The http database backend
  is tested against it through the library and needs `curl` (skipped
  without it).
- **`completions.rs`**: the entry point of the completion tests
  (`tests/completions/rust/`), see
  [completions/README.md](completions/README.md).

## Test utilities

`common/mod.rs` holds the shared helpers:

- `create_test_task(id, text, done)` and
  `create_test_task_with_date(id, text, done, date)` create test tasks.
- `Sandbox` is a private world for one test that spawns the `rusk` binary:
  its own temp dir, database, HOME and config. `Sandbox::with_db(json)`
  seeds the database, `sb.cmd()` returns a `Command` bound to the sandbox,
  and `sb.read_db()` / `sb.write_db()` / `sb.db_path()` inspect it.
  `sb.cmd_with_fake_ssh()` (unix) puts a local `ssh` shim first in PATH, so
  ssh locations can be tested without sshd or network.
- `Sandbox::in_pty` and `Sandbox::in_pty_steps` run the binary in a
  pseudo-terminal through `common/pty_driver.py` (see R24 and R30 below).

Every test that runs the binary must go through a `Sandbox`:

- Debug and test-mode binaries ignore `RUSK_DB` and pin the database to
  `$TMPDIR/rusk-<uid>/debug/tasks.json` (`Sandbox::db_path`). The sandbox
  points `TMPDIR` (and `RUSK_DB`, for release binaries) at its own
  directory, with the `rusk-<uid>` made closed to others as rusk makes it,
  and forces test mode on the child, so a debug binary never seeds its demo
  tasks into the test database.
- Test mode in the binary exists in debug builds only: the `rusk` command
  built for release never is in it (REVIEW №12), whatever its name or
  environment. A test that needs it there (`RUSK_DB` ignored) is
  `#[cfg(debug_assertions)]`, and `r25_a_release_binary_is_never_in_test_mode`
  runs under `cargo test --release` only. The library linked into a test
  binary is in test mode in any profile.

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

## Where the review clusters are tested

`review_known_bugs.rs` holds failing repros of open findings behind
`#[ignore]`: `cargo test --test review_known_bugs -- --ignored` shows them.
When a fix lands, drop the `#[ignore]` of the tests it turns green. The
tests of a closed cluster stay in the file as its regression suite. Closed
so far: R1 error causes, R2 argv of edit/mark/del, R3 the rules every loaded
list is held to, R4 text measured in terminal cells, R5 "cannot read" is not
"zero tasks", R6 the terminal comes back, R7 task text does not drive the
terminal, R8 the draft lifecycle, R9 restore, R10/R13 reading a location
value, R11 quoting in the completion scripts, R12 lost updates, R14 atomic
writes, R15 the ssh protocol, R16 web UI, R18 the SQLite connection, R19
output, terminals and messages, R20 `rusk serve`, R21 `rusk sync`, R22
`git_backend`, R24 the editor's keys. R27 onwards close the leads of
REVIEW.md section 4.

Where each cluster's checks live, black-box (`r<N>_*` tests here) and unit
(in `src/`):

- **R2**: the completion tests drive each installed shell (bash, zsh, fish,
  nu, pwsh; a missing one is skipped) with a fake `rusk` first on PATH and
  without the user's shell config.
- **R3**: the rules themselves are unit-tested in `src/model.rs`
  (`normalize`), the CSV rows and lines in `src/codec/csv.rs`.
- **R4**: the measuring in `src/width.rs`, its editor half in
  `src/cli/editor/view.rs`. What a terminal receives from the editor (cursor
  column, click target, expanded tabs) was checked in a pty, see the R4
  status in REVIEW.md.
- **R5**: per format in `src/codec/mod.rs` (a BOM belongs to no format;
  blanks are a stored state only where rusk writes an empty database as an
  empty file) and in `src/transport.rs` (the remote read command tells a
  missing file from a failing `cat`). The ssh half runs black-box against
  the fake `ssh` shim of `tests/common`.
- **R6, R8, R17 and the editor half of R7**: no in-repo harness of their
  own. Their repros need a pty, and the parts that matter (a terminating
  signal, a panic, a restored draft, `Ctrl+C`) cannot be driven from
  `Command`. What Rust can check lives here: the TTY refusal, and the draft
  unit tests in `src/cli/editor/draft.rs`. The rest was driven by a
  `pty.fork` script against a **release** binary (see the R6/R8 and R7/R17
  status in REVIEW.md). R17's view rules (size fallback, view clamping,
  footer hint, click reach) are unit-tested in `src/cli/editor/view.rs` and
  `mouse.rs`.
- **R7**: the escaping in `src/printable.rs`, the editor's input filter in
  `src/cli/editor/text_ops.rs`.
- **R11**: like R2, and then the completed line is run through that shell,
  so what counts is the text the fake `rusk` receives. The
  `--for-completion-lines` encoder is unit-tested in `src/cli/handlers.rs`;
  fish's Tab-by-Tab insertion was checked in a pty (see the R11 status in
  REVIEW.md).
- **R12**: concurrency tests come in three kinds: threads of one process
  (cheap, but SQLite arbitrates those in memory), processes of the `rusk`
  binary (file databases only: a debug binary ignores `RUSK_DB`), and, for
  SQLite across processes, the test binary starting itself as a worker
  (`r12_sqlite_worker`, a no-op unless `RUSK_R12_WORKER_DB` is set).
- **R14**: the write path in `src/atomic.rs`, the writer lock in
  `src/lock.rs`.
- **R16**: the page script of `rusk serve` (`src/web/template.html`) has no
  in-repo harness. The API under it is covered by `web_tests.rs` and the
  unit tests of `src/web/api.rs`; the page itself was checked black-box in
  headless Chromium (see the R16 status in REVIEW.md).
- **R18**: the SQLite backend through the library (a debug binary is pinned
  to a JSON file), its pure parts in `src/backend/sqlite.rs`.
- **R19**: closes stdout under a command with `std::io::pipe` (the reader
  dropped before the spawn, so every write gets EPIPE), writes it to
  `/dev/full`, and takes a terminal away with `setsid` (no controlling
  terminal for crossterm to fall back on). Pure parts: `src/output.rs` (the
  color decision, closed-pipe errors), `src/search.rs` (folding, byte
  ranges), `src/cli/formatter.rs` (rows as ranges, highlights, the compact
  row). What needs a pty (an empty `NO_COLOR` on a terminal, clap's colors,
  the `del` prompt, the editor's report after a failed save) was checked
  against a release binary (see the R19 status in REVIEW.md).
- **R20**: starts `rusk serve --port 0` (`serve()`, which reads the port
  from the first line and kills the server when the test ends) and speaks
  HTTP/1.1 to it over one keep-alive connection (`Http`, chunked bodies put
  together). A server that cannot accept runs under `ulimit -n`. Pure parts
  (hosts, tokens, cookies, bodies, the one-at-a-time work on the tasks) in
  `src/web/server.rs` through `tiny_http::TestRequest`.
- **R21**: syncs through the fake `ssh` of `tests/common` (one that counts
  its calls and lets another writer change the remote right after rusk's
  write), through a real `rusk serve`, and through a fake `curl` that shows
  its arguments. The decision table and the state file are unit-tested in
  `src/sync.rs`, the header file in `src/transport.rs`.
- **R22**: saves with `git_backend = true` into repositories of the sandbox
  and asks the system `git` what happened (skipped without git).
  `Sandbox::cmd` and the tests' own git run without the machine's git
  configuration and identity. What needs a repository of a particular shape
  (hooks of its config, filters, permissions, ceilings, symlinks) is
  unit-tested in `src/backend/git.rs`.
- **R23**: the `r23_*` tests drive the file formats through the library
  (`TaskManager::open_at`; a debug binary ignores `RUSK_DB`). The codecs'
  pure parts are in `src/codec/` (`markdown`, `ics`, `ndjson`), the report
  of a broken JSON file in `src/backend/file.rs`.
- **R24**: drives the editor itself. `Sandbox::in_pty` (`tests/common`)
  runs the binary in a pseudo-terminal through python's `pty`
  (`tests/common/pty_driver.py`), which answers the terminal queries (as
  kitty, when asked), waits for a marker on the screen, the alternate screen
  or a prompt, and types the keys. The tests are skipped without python3 or
  without a pty to be had. The driver runs without `DISPLAY` and
  `WAYLAND_DISPLAY`, so the editor never reaches the desktop's clipboard. A
  draft is put in place as the file the editor writes
  (`editor-new-task.draft` beside the sandbox database). Pure parts:
  `src/cli/editor/` (`text_ops`, `state`, `input`) and
  `src/cli/handlers.rs`.
- **R25**: the config parser in `src/config.rs` (comments, quotes, bytes,
  `default`, the template), the date parser in `src/parser/date.rs` (the
  shape and range of a year, months first), stored dates outside the years
  in `src/model.rs` (`normalize`, `parse_iso_date`). The `r25_*` tests run
  the binary with a config file of their own.
- **R26**: the completion table (`tests/completions/cases.txt`) runs in
  bash, zsh, fish, nu and PowerShell, whichever are installed, through
  `tests/completions/rust/table_tests.rs`, in a `Sandbox`. A script that
  prints anything while completing fails it (see
  [completions/README.md](completions/README.md)).
  `r26_rusk_db_names_a_served_database` runs under `cargo test --release`
  only, like `r25_a_release_binary_is_never_in_test_mode`.
- **R27**: pure parts where they live: the dependency set and what a
  deletion takes off the lists in `src/storage.rs`, the painting of many
  search matches in `src/cli/formatter.rs`, the files that belong to a
  database in `src/backend/mod.rs`.
  `r27_the_delete_question_names_who_depends_on_the_task` drives the `del`
  prompt in a pty.
- **R28**: the http side runs through a fake `curl` that logs its arguments
  and header files, and a real one with a `~/.curlrc` of the test's own.
  `r28_a_server_that_is_its_own_database_says_so` runs under
  `cargo test --release` only. The URL's credentials are unit-tested in
  `src/backend/http.rs`, a quoted URL in `src/location.rs`, the answer to a
  server's own request in `src/web/server.rs`.
- **R29**: a real `rusk serve` and curl. The routing (ids in paths,
  `Sec-Fetch-Site`, the sign-in page) is unit-tested in
  `src/web/server.rs`.
- **R30**: drives the editor in a pty like R24. `Sandbox::in_pty_steps`
  gives a run variables of its own (`TERM=dumb`) and steps that resize the
  terminal (the driver sets the window size, the kernel sends SIGWINCH).
  The Cyrillic keys, block deletion, the kill keys and the OSC 52 limit are
  unit-tested in `src/cli/editor/`, the bare `y` in `src/cli/dialogs.rs`.

## Adding new tests

1. Choose the appropriate test file, or create a new one for a new feature
   area.
2. Use the helpers from `common/mod.rs` when creating test data.
3. Follow the existing test patterns.
4. Use descriptive test names starting with `test_`.
5. Test both success and failure cases.
6. Use `tempfile` for temporary directories when testing file operations.

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

## CI

The tests are designed to run in CI/CD pipelines:

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

- Tests use temporary directories for file operations, so user data is
  never affected.
- Some tests require a specific environment (see the individual test
  files).
- Tests are designed to run in parallel (use `cargo test --test-threads=1`
  if needed).
- Tests never share a database: binary tests use a per-test `Sandbox`,
  library tests use `tempfile` dirs or `TaskManager::new_empty()` (a unique
  path per call).
- The test process environment is never mutated (`std::env::set_var` races
  with parallel tests); environment variables are set on the spawned child
  instead.
- Error-message tests print the whole chain (`format!("{err:#}")`), the way
  `main` and the web API do; `to_string()` is the outermost context only.
- The completion tests are in a separate directory (`completions/`) with
  their own README.

<br />
<p align="center"><a href="#rusk-tests">Back to top</a></p>
