<h1 align="center" id="rusk-shell-completions-tests">Rusk Shell Completion Tests</h1>
<br />

Tests for the shell completion scripts. They are separate from the
application tests in `tests/` and focus on completion behavior alone.

What the scripts offer is one table for all five shells: `cases.txt` holds
command lines and the candidates for them (a case may name the shells it is
for, where they differ on purpose). `rust/table_tests.rs` runs every case in
bash, zsh, fish, nu and PowerShell, whichever are installed, as part of
`cargo test`, with a home, a config and a one-task database of their own and
this build's `rusk` first in PATH. A behavior of the scripts is a line of the
table. The shell suites below keep what the table cannot hold: syntax, and
stubs for the task text next to an id.

Zsh tests source `completions/rusk.zsh` with `_RUSK_ZSH_SKIP_ENTRY=1`, so
the file only defines functions and does not run the completer once on load.
That avoids invoking the real `rusk` binary before the stubs override the
helpers.

## Structure

```
tests/completions/
├── README.md                    # This file
├── cases.txt                    # What every shell offers for a command line
├── run_all.sh                   # Run all completion tests for all shells
├── rust/                        # Rust unit tests for completion code
│   ├── completions_install_tests.rs  # Tests for completion installation
│   ├── nu_completion_tests.rs        # Nu Shell-specific completion tests
│   └── table_tests.rs                # cases.txt in all five shells
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
│   └── test_edit_after_id.sh         # Edit after ID tests
├── zsh/                         # Zsh completion tests
│   ├── run_all.sh                    # Zsh test runner
│   ├── helpers.zsh                   # Helper functions
│   ├── test_basic.zsh                # Basic completion tests
│   └── test_edit_after_id.zsh        # Edit after ID tests
├── fish/                        # Fish shell completion tests
│   ├── run_all.fish                  # Fish test runner
│   ├── test_basic.fish               # Basic completion tests
│   └── test_edit_after_id.fish       # Edit after ID tests
└── nu/                          # Nu Shell completion tests
    ├── run_all.nu                    # Nu test runner
    ├── test_basic.nu                 # Basic completion tests
    └── test_edit_after_id.nu         # Edit after ID tests
```

The Rust tests are included through `tests/completions.rs`, which references
the files in the `rust/` subdirectory.

## Running the tests

```bash
cargo test --test completions         # the Rust tests, the table included
./tests/completions/run_all.sh        # the shell suites, every installed shell
```

One shell at a time:

```bash
pwsh -File tests/completions/powershell/run_all.ps1
bash tests/completions/bash/run_all.sh
zsh tests/completions/zsh/run_all.sh
fish tests/completions/fish/run_all.fish
nu tests/completions/nu/run_all.nu
```

## Test structure

Each shell's directory contains:

- `run_all.{ext}`: the runner that executes all test files of that shell.
- `test_*.{ext}`: the test files, one per scenario:
  - `test_basic.{ext}`: basic completion functionality;
  - `test_all_commands.ps1`: PowerShell's scenarios (the other shells' went
    into `cases.txt`);
  - `test_edit_after_id.{ext}`: the critical tests ensuring that task text
    (not dates) follows a task id.
- `helpers.{ext}`: helper functions, where applicable.

## Test scenarios

What each shell offers is `cases.txt`: every line there is a scenario, the
same in all five shells unless the line names the shells it is for. In short:

- `rusk <tab>`: the commands and their aliases (a, e, m, d, l, s, r, g, c),
  and `help`. fish and nu also list `-h`, `--help`, `-V`, `--version`, which
  every shell offers for `rusk -<tab>`. A command typed in full is completed
  as the command (`rusk add<tab>` → `rusk add `).
- `rusk help <tab>`: the commands. `rusk help sync <tab>` → push, pull;
  `rusk help completions <tab>` → install, show; nothing after that.
- Task ids are never offered: `rusk edit <tab>`, `rusk mark <tab>`,
  `rusk del <tab>` give the flags of the command.
- `rusk edit 1<TAB>` puts the text of task 1 next to its id. fish completes
  the id first and gives the text on the next `<TAB>`, `rusk edit 1 <TAB>`.
- `add` and `edit` offer `-d`/`--date` and `-a`/`--after` while they are not
  on the line, given alone or with their value (`--date=X`, `-dX`), and
  after task text (`add`) or an id (`edit`). After `-d`/`--date` + space,
  help only.
- After `--` everything is text: nothing is offered.
- `rusk completions install <tab>`, `rusk completions show <tab>`: the
  shells not chosen yet.
- Where a script offers nothing, PowerShell falls back to file names; the
  table leaves those out.

**Edit after an id** (`test_edit_after_id`):

- `rusk e 1<TAB>` → the task text only (no date *values* in completions).
- `rusk e 1 <tab>` / `rusk e 1 foo <tab>` → `-d`, `--date`, `-h`, `--help`
  (the CLI date is optional; the TUI uses the first line).
- `rusk e 1,2 <tab>` → empty (ids are not suggested).

## Adding new tests

1. Create a new test file following the naming pattern `test_*.{ext}`.
2. Use the helper functions of that shell, if available.
3. Follow the structure of the existing tests.
4. Make the file executable (for shell scripts).

## CI

`cargo test` runs the table (`cargo test --test completions`). CI installs
zsh and fish for it, and the image has bash and PowerShell. Nushell is not
installed there, so its table is skipped in CI (the test says so on stderr,
which `cargo test -- --nocapture` shows). The shell suites run with
`./tests/completions/run_all.sh`.

## Notes

- The table runs the scripts in a sandbox: a home, a config and a one-task
  database of their own, the same in a debug and a release build, with this
  build's `rusk` first in PATH. Each shell has two minutes for the whole
  table.
- A script that prints anything while completing, on stdout or stderr,
  fails the table: in a real shell that text lands on the user's terminal.
- `run_all.sh` skips the shells that are not installed on the system.
- Each shell's runner can be executed on its own to debug that shell.
- Zsh: the `completions|c)` branch must not declare `local i` again. A
  duplicate `local i` in `_rusk_main` prints `i=…` during completion, and
  the table catches it.

<br />
<p align="center"><a href="#rusk-shell-completions-tests">Back to top</a></p>
