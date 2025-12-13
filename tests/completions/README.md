# Shell Completion Tests

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
│   ├── run_all.ps1        # PowerShell test runner
│   ├── helpers.ps1        # Helper functions
│   ├── test_basic_completion.ps1
│   └── test_edit_after_id.ps1
├── bash/                  # Bash completion tests
│   ├── run_all.sh
│   ├── helpers.sh
│   ├── test_basic.sh
│   └── test_all_commands.sh
├── zsh/                   # Zsh completion tests
│   ├── run_all.sh
│   ├── helpers.zsh
│   ├── test_basic.zsh
│   └── test_all_commands.zsh
├── fish/                  # Fish shell completion tests
│   ├── run_all.fish
│   ├── test_basic.fish
│   └── test_all_commands.fish
└── nu/                    # Nu Shell completion tests
    ├── run_all.nu
    ├── test_basic.nu
    └── test_all_commands.nu
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
- `test_*.{ext}` - Individual test files for specific scenarios
- `helpers.{ext}` - Helper functions for tests (if applicable)

## Test Scenarios

Common test scenarios across all shells:

1. **Command completion** - `rusk <tab>` should suggest available commands
2. **Subcommand completion** - `rusk edit <tab>` should suggest task IDs
3. **Task ID completion** - `rusk edit <tab>` should list available task IDs
4. **Task text completion after ID** - `rusk edit 1 <tab>` should suggest task text (NOT dates)
5. **Date completion after date flag** - `rusk add --date <tab>` should suggest dates
6. **Flag completion** - `rusk edit 1 -<tab>` should suggest available flags
7. **Multiple ID completion** - `rusk edit 1 2 <tab>` should handle multiple IDs correctly

## Command Coverage

### PowerShell Tests (Comprehensive)

PowerShell tests provide complete coverage for all commands:

- ✅ **add (a)** - Flag completion, date completion after `--date` flag
- ✅ **edit (e)** - Task ID completion, task text after ID, flag completion, date after flag
- ✅ **mark (m)** - Task ID completion, multiple IDs
- ✅ **del (d)** - Task ID completion, flag completion (`--done`), multiple IDs
- ✅ **list (l)** - No arguments (empty completion)
- ✅ **restore (r)** - No arguments (empty completion)
- ✅ **completions (c)** - Subcommand completion, shell completion

All command aliases are tested: `a`, `e`, `m`, `d`, `l`, `r`, `c`

### Other Shells

Comprehensive tests are now available for all shells:

- **Bash**: 2 test files covering all commands and functionality
- **Zsh**: 2 test files covering all commands and functionality  
- **Fish**: 2 test files covering all commands and functionality
- **Nu Shell**: 2 test files covering all commands and functionality

All shells have tests for:
- Command completion
- Subcommand completion
- Task ID completion
- Date completion
- Flag completion
- Function existence and syntax validation

## PowerShell-Specific Tests

PowerShell tests include more detailed scenarios:
- **Critical test**: `rusk e 1 <tab>` must return ONLY task text, NO dates
- Tests for edge cases with multiple IDs
- Tests for date flag handling

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
