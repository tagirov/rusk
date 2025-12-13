<h1 align="center" id="shell-completions-tests">Shell Completion Tests</h1>
<br />

This directory contains tests for shell completion scripts. These tests are separate from the main application tests in `tests/` and focus specifically on validating completion behavior.

## Structure

```
tests/completions/
├── README.md              # This file
├── run_all.sh             # Run all completion tests for all shells
├── rust/                  # Rust unit tests for completion code
│   ├── completion_tests.rs           # Tests for parsing rusk list output
│   ├── completions_install_tests.rs  # Tests for completion installation
│   └── nu_completion_tests.rs        # Nu Shell-specific completion tests
├── powershell/            # PowerShell completion tests
│   ├── README.md          # PowerShell-specific test documentation
│   ├── run_all.ps1        # PowerShell test runner
│   ├── helpers.ps1        # Helper functions
│   ├── test_basic_completion.ps1
│   ├── test_all_commands.ps1
│   └── test_edit_after_id.ps1
├── bash/                  # Bash completion tests
│   ├── run_all.sh
│   ├── helpers.sh
│   ├── test_basic.sh
│   ├── test_all_commands.sh
│   └── test_edit_after_id.sh
├── zsh/                   # Zsh completion tests
│   ├── run_all.sh
│   ├── helpers.zsh
│   ├── test_basic.zsh
│   ├── test_all_commands.zsh
│   └── test_edit_after_id.zsh
├── fish/                  # Fish shell completion tests
│   ├── run_all.fish
│   ├── test_basic.fish
│   ├── test_all_commands.fish
│   └── test_edit_after_id.fish
└── nu/                    # Nu Shell completion tests
    ├── run_all.nu
    ├── test_basic.nu
    ├── test_all_commands.nu
    └── test_edit_after_id.nu
```

**Note**: Rust tests are included via `tests/completions.rs` which references the files in `rust/` subdirectory.

## Running Tests

### Rust Tests
Run Rust unit tests for completion functionality:
```bash
cargo test --test completions
```

### All Shell Tests (Recommended)
Run tests for all available shells:
```bash
./tests/completions/run_all.sh
```

### Individual Shell Tests

#### PowerShell
```powershell
pwsh -File tests/completions/powershell/run_all.ps1
```

#### Bash
```bash
bash tests/completions/bash/run_all.sh
```

#### Zsh
```zsh
zsh tests/completions/zsh/run_all.sh
```

#### Fish
```fish
fish tests/completions/fish/run_all.fish
```

#### Nu Shell
```nu
nu tests/completions/nu/run_all.nu
```

## Test Structure

Each shell's test directory contains:
- `run_all.{ext}` - Main test runner that executes all test files
- `test_*.{ext}` - Individual test files for specific scenarios:
  - `test_basic.{ext}` - Basic completion functionality tests
  - `test_all_commands.{ext}` - Comprehensive tests for all commands
  - `test_edit_after_id.{ext}` - Critical tests ensuring task text (not dates) after task ID
- `helpers.{ext}` - Helper functions for tests (if applicable)

## Test Scenarios

Common test scenarios across all shells:

1. Command completion - `rusk <tab>` should suggest available commands
2. Subcommand completion - `rusk edit <tab>` should suggest task IDs
3. Task ID completion - `rusk edit <tab>` should list available task IDs
4. Task text completion after ID - `rusk edit 1 <tab>` should suggest task text (NOT dates)
5. Date completion after date flag - `rusk add --date <tab>` should suggest dates
6. Flag completion - `rusk edit 1 -<tab>` should suggest available flags
7. Multiple ID completion - `rusk edit 1 2 <tab>` should handle multiple IDs correctly

## Command Coverage


- add (a) - Flag completion, date completion after `--date` flag
- edit (e) - Task ID completion, task text after ID, flag completion, date after flag
- mark (m) - Task ID completion, multiple IDs
- del (d) - Task ID completion, flag completion (`--done`), multiple IDs
- list (l) - No arguments (empty completion)
- restore (r) - No arguments (empty completion)
- completions (c) - Subcommand completion, shell completion

All command aliases are tested: `a`, `e`, `m`, `d`, `l`, `r`, `c`


## Adding New Tests

To add a new test:

1. Create a new test file following the naming pattern `test_*.{ext}`
2. Use the appropriate helper functions if available
3. Follow the test structure used in existing tests
4. Ensure the test file is executable (for shell scripts)

## Integration with CI/CD

These tests can be integrated into CI/CD pipelines:

```yaml
# Example GitHub Actions step
- name: Run completion tests
  run: ./tests/completions/run_all.sh
```

## Notes

- Completion tests require the completion scripts to be installed or available in the expected location
- Some tests may require actual task data in the rusk database
- Tests are designed to be run after building the project: `cargo build --release`
- The `run_all.sh` script will automatically skip shells that are not installed on the system
- Rust tests can be run independently: `cargo test --test completions`
- Each shell's test runner can be executed individually for debugging specific shell issues

<br />
<p align="right"><a href="#shell-completion-tests">Back to top</a></p>