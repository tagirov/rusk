<h1 align="center" id="rusk-shell-completions-tests">Rusk Shell Completion Tests</h1>
<br />

This directory contains tests for shell completion scripts. These tests are separate from the main application tests in `tests/` and focus specifically on validating completion behavior.

Zsh tests source `completions/rusk.zsh` with `_RUSK_ZSH_SKIP_ENTRY=1` so the file only defines functions and does not run the completer once on load (avoids invoking the real `rusk` binary before stubs override helpers).

## Structure

```
tests/completions/
├── README.md                    # This file
├── run_all.sh                   # Run all completion tests for all shells
├── rust/                        # Rust unit tests for completion code
│   ├── mod.rs                   # Module entry point
│   ├── completion_tests.rs           # Tests for parsing rusk list output
│   ├── completions_install_tests.rs  # Tests for completion installation
│   └── nu_completion_tests.rs        # Nu Shell-specific completion tests
├── powershell/                  # PowerShell completion tests
│   ├── run_all.ps1                   # PowerShell test runner
│   ├── helpers.ps1                   # Helper functions
│   ├── test_basic_completion.ps1     # Basic completion tests
│   ├── test_all_commands.ps1         # All commands tests
│   └── test_edit_after_id.ps1        # Edit after ID tests
├── bash/                        # Bash completion tests
│   ├── run_all.sh                    # Bash test runner
│   ├── helpers.sh                    # Helper functions
│   ├── test_basic.sh                 # Basic completion tests
│   ├── test_all_commands.sh          # All commands tests
│   └── test_edit_after_id.sh         # Edit after ID tests
├── zsh/                         # Zsh completion tests
│   ├── run_all.sh                    # Zsh test runner
│   ├── helpers.zsh                   # Helper functions
│   ├── test_basic.zsh                # Basic completion tests
│   ├── test_all_commands.zsh         # All commands tests
│   └── test_edit_after_id.zsh        # Edit after ID tests
├── fish/                        # Fish shell completion tests
│   ├── run_all.fish                  # Fish test runner
│   ├── test_basic.fish               # Basic completion tests
│   ├── test_all_commands.fish        # All commands tests
│   └── test_edit_after_id.fish       # Edit after ID tests
└── nu/                          # Nu Shell completion tests
    ├── run_all.nu                    # Nu test runner
    ├── test_basic.nu                 # Basic completion tests
    ├── test_all_commands.nu          # All commands tests
    └── test_edit_after_id.nu         # Edit after ID tests
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

Common scenarios covered across all shells (bash, zsh, fish, powershell, nu):

| Scenario | Input | Expected |
|----------|--------|----------|
| **Root commands** | `rusk <tab>` | Commands: add, del, edit, list, mark, restore, completions (and aliases a, d, e, l, m, r, c) |
| **Task ID completion** | `rusk edit <tab>`, `rusk mark <tab>`, `rusk del <tab>` | Empty (no task ID suggestions) |
| **Task text after single ID** | `rusk edit 1<TAB>` | Task text only — **not** dates (critical for edit) |
| **Multiple IDs** | `rusk edit 1,2 <tab>`, `rusk mark 1,2 <tab>`, `rusk del 1,2 <tab>` | Empty (no task ID suggestions) |
| **After date flag + space** | `rusk add x --date <tab>`, `rusk edit 1 --date <tab>` | `-h` / `--help` only (no date value suggestions) |
| **Flag completion** | `rusk add <tab>`, `rusk add -<tab>`, `rusk edit 1 -<tab>`, `rusk del -<tab>` | Flags (e.g. --date, -d, --done, --help, -h) |
| **Completions subcommands** | `rusk completions <tab>`, `rusk c <tab>` | install, show |
| **Completions shells** | `rusk completions install <tab>`, `rusk completions show <tab>` | bash, zsh, fish, nu, powershell |
| **List / Restore** | `rusk list <tab>`, `rusk restore <tab>`, `rusk l <tab>`, `rusk r <tab>` | Empty (no arguments) |
| **Aliases** | `rusk a <tab>`, `rusk e <tab>`, `rusk m <tab>`, `rusk d <tab>`, etc. | Same as full command |

**Edit-after-ID (test_edit_after_id):**
- `rusk e 1<TAB>` → task text only, **no dates**.
- `rusk e 1,2 <tab>` → empty (IDs are not suggested).
- `rusk e 1 --date <tab>` → `-h` / `--help` only (no preset dates).

## Command Coverage

- **add** (a) — flag completion (`-d` / `--date` only after task text); after `-d` / `--date` and a space, help flags only
- **edit** (e) — task text after ID, flag completion; after `-d` / `--date` and a space, help flags only
- **mark** (m) — flag completion (task IDs are typed manually)
- **del** (d) — `--done` flag, flag completion (task IDs are typed manually)
- **list** (l) — no arguments (empty completion)
- **restore** (r) — no arguments (empty completion)
- **completions** (c) — subcommands: `install` (shells: bash, zsh, fish, nu, powershell), `show` (shell)

All aliases are tested: `a`, `e`, `m`, `d`, `l`, `r`, `c`.


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
<p align="center"><a href="#rusk-shell-completions-tests">Back to top</a></p>