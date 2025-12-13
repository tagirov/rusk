# PowerShell Completion Tests

This directory contains comprehensive tests for PowerShell completion functionality.

## Test Files

- `test_basic_completion.ps1` - Basic completion tests (commands, subcommands, flags)
- `test_edit_after_id.ps1` - Critical tests for edit command after ID (ensures no dates are suggested)
- `test_all_commands.ps1` - Comprehensive tests for all rusk commands and their completion behavior

## Test Coverage

### Commands Tested

1. **add (a)** - Flag completion, date completion after `--date` flag
2. **edit (e)** - Task ID completion, task text completion after ID, flag completion, date completion after flag
3. **mark (m)** - Task ID completion, multiple ID completion
4. **del (d)** - Task ID completion, flag completion (including `--done`), multiple ID completion
5. **list (l)** - No arguments (empty completion)
6. **restore (r)** - No arguments (empty completion)
7. **completions (c)** - Subcommand completion (install, show), shell completion

### Scenarios Tested

- Command completion (`rusk <tab>`)
- Subcommand completion (`rusk edit <tab>`)
- Task ID completion (`rusk edit <tab>`, `rusk mark <tab>`)
- Task text completion after ID (`rusk edit 1 <tab>`)
- Date completion after date flag (`rusk add --date <tab>`)
- Flag completion (`rusk edit 1 -<tab>`)
- Multiple ID completion (`rusk edit 1 2 <tab>`)
- Alias completion (all command aliases: a, e, m, d, l, r, c)

## Running Tests

```powershell
# Run all tests
pwsh -File tests/completions/powershell/run_all.ps1

# Run specific test
pwsh -File tests/completions/powershell/test_all_commands.ps1
```

## Helper Functions

The `helpers.ps1` file provides:
- `Test-CompletionScenario` - Main test function
- `Assert-Equals`, `Assert-NotEquals`, `Assert-True`, `Assert-False` - Assertion functions

## Critical Tests

The `test_edit_after_id.ps1` file contains critical tests that ensure:
- `rusk e 1 <tab>` returns ONLY task text, NOT dates
- This was a reported bug that has been fixed
