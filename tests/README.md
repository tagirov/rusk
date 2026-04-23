<h1 align="center" id="rusk-tests">Rusk Tests</h1>
<br />

This directory contains comprehensive unit and integration tests for the rusk task management application. These tests validate core functionality, edge cases, data persistence, and CLI behavior.

## Structure

```
tests/
├── README.md                       # This file
├── common/                         # Shared test utilities
│   └── mod.rs                      # Helper functions for creating test tasks
├── completions/                    # Shell completion tests (see completions/README.md)
│   └── ...
├── cli_tests.rs                    # CLI command tests (TaskManager API)
├── cli_utils_tests.rs              # CLI utility function tests (wrap, trim, word nav)
├── lib_tests.rs                    # Core library function tests
├── database_corruption_tests.rs    # Database corruption handling tests
├── directory_structure_tests.rs    # Directory creation, default path, RUSK_DB in test mode
├── edge_case_tests.rs              # Edge cases and boundary condition tests
├── edit_mode_tests.rs              # Edit command mode tests (parse_edit_args, strip -d)
├── edit_parsing_tests.rs           # Edit command argument parsing tests
├── integration_main_tests.rs       # Integration tests: real `rusk` binary, flags, RUSK_DB harness
├── mark_success_tests.rs           # Mark command success/failure tests
├── parse_flexible_ids_tests.rs     # ID parsing tests (comma lists; single ID without comma)
├── persistence_tests.rs            # Data persistence and save/load tests
├── restore_tests.rs                # Backup restore functionality tests
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
- `list -f` / `--first-line` omits body lines after the first line of task text
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

#### `edit_mode_tests.rs`
Tests for `parse_edit_args` (IDs vs text tokens, including former `--date` tokens as plain text).

### ID Parsing Tests

#### `parse_flexible_ids_tests.rs`
Tests for flexible ID parsing:
- Single ID parsing
- Comma-separated IDs (`1,2,3`)
- Space-separated IDs
- Invalid ID handling
- Mixed format handling

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
- Database path resolution uses test mode (`/tmp/rusk_debug/tasks.json`) to avoid conflicts
- Completion tests are in a separate directory (`completions/`) with their own README

<br />
<p align="center"><a href="#rusk-tests">Back to top</a></p>
