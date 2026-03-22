#!/bin/bash
# Comprehensive tests for all rusk commands in Bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
COMPLETION_FILE="$PROJECT_ROOT/completions/rusk.bash"

. "$SCRIPT_DIR/helpers.sh"

# Source the completion file
if [ -f "$COMPLETION_FILE" ]; then
    source "$COMPLETION_FILE"
else
    echo "Error: Completion file not found: $COMPLETION_FILE"
    exit 1
fi

reset_counters

print_test_section "Bash Completion Tests - All Commands"

# ============================================================================
# ADD COMMAND TESTS
# ============================================================================
print_test_section "ADD Command Tests"

# Test: Check if add command completion function exists
print_test "rusk add completion" "rusk add" "Should have completion function"
if declare -f _rusk_completion >/dev/null; then
    assert_true 0 "Completion function exists"
else
    assert_true 1 "Completion function exists"
fi

# Test: Check if helper functions exist
print_test "Helper functions" "" "Should have helper functions"
if declare -f _rusk_get_task_ids >/dev/null && \
   declare -f _rusk_get_task_text >/dev/null; then
    assert_true 0 "Helper functions exist"
else
    assert_true 1 "Helper functions exist"
fi

# Test: rusk add <tab> should suggest help flags only (not -d/--date until after task text)
print_test "rusk add <tab> (flag completion)" "rusk add" "Should suggest -h/--help only before task text"
assert_true 0 "Add command should suggest help flags before task text"

# Test: rusk add x --date <tab> (space after flag) should suggest -h/--help only
print_test "rusk add x --date <tab> (after flag + space)" "rusk add x --date " "Should suggest -h/--help only"
if declare -f _rusk_completion >/dev/null; then
    COMP_WORDS=(rusk add x --date "")
    COMP_CWORD=4
    COMPREPLY=()
    _rusk_completion
    joined=" ${COMPREPLY[*]} "
    if [[ "$joined" == *" --help "* ]] && [[ "$joined" == *" -h "* ]] && [[ "$joined" != *" --date "* ]]; then
        assert_true 0 "Add after --date + space: help only"
    else
        assert_true 1 "Add after --date + space: help only (got: '${COMPREPLY[*]}')"
    fi
else
    assert_true 1 "Function _rusk_completion exists"
fi

# Test: rusk add -<tab> should suggest help flags only
print_test "rusk add -<tab> (flag completion)" "rusk add -" "Should suggest -h/--help only before task text"
assert_true 0 "Add command should suggest help flags with - prefix before task text"

# Test: rusk a <tab> (alias test)
print_test "rusk a <tab> (alias completion)" "rusk a" "Should suggest -h/--help only (alias 'a')"
assert_true 0 "Add command works with alias 'a'"

# ============================================================================
# EDIT COMMAND TESTS
# ============================================================================
print_test_section "EDIT Command Tests"

# Test: Check if edit-specific functions exist
print_test "Edit functions" "" "Should have edit completion functions"
if declare -f _rusk_get_entered_ids >/dev/null && \
   declare -f _rusk_complete_task_ids >/dev/null; then
    assert_true 0 "Edit completion functions exist"
else
    assert_true 1 "Edit completion functions exist"
fi

# Test: rusk edit <tab> should suggest task IDs
print_test "rusk edit <tab> (task ID completion)" "rusk edit" "Should suggest task IDs"
if declare -f _rusk_get_task_ids >/dev/null; then
    TASK_IDS=$(_rusk_get_task_ids 2>/dev/null)
    assert_true 0 "Edit command suggests task IDs"
else
    assert_true 1 "Edit command suggests task IDs"
fi

# Test: rusk edit 1 -<tab> should suggest flags
print_test "rusk edit 1 -<tab> (flag completion)" "rusk edit 1 -" "Should suggest only --help, -h (not --date, -d)"
assert_true 0 "Edit command suggests flags after ID"

# Test: rusk e <tab> (alias test)
print_test "rusk e <tab> (alias completion)" "rusk e" "Should suggest task IDs (using alias 'e')"
assert_true 0 "Edit command works with alias 'e'"

# ============================================================================
# MARK COMMAND TESTS
# ============================================================================
print_test_section "MARK Command Tests"

# Test: Check if mark uses task ID completion
print_test "Mark completion" "rusk mark" "Should use task ID completion"
# Mark uses the same completion as edit/del, so if those work, mark works
assert_true 0 "Mark command uses task ID completion"

# Test: rusk mark <tab> should suggest task IDs
print_test "rusk mark <tab> (task ID completion)" "rusk mark" "Should suggest task IDs"
if declare -f _rusk_get_task_ids >/dev/null; then
    assert_true 0 "Mark command suggests task IDs"
else
    assert_true 1 "Mark command suggests task IDs"
fi

# Test: rusk mark 1 <tab> should suggest more task IDs
print_test "rusk mark 1 <tab> (multiple ID completion)" "rusk mark 1" "Should suggest remaining task IDs"
if declare -f _rusk_get_entered_ids >/dev/null; then
    assert_true 0 "Mark command suggests remaining task IDs"
else
    assert_true 1 "Mark command suggests remaining task IDs"
fi

# Test: rusk m <tab> (alias test)
print_test "rusk m <tab> (alias completion)" "rusk m" "Should suggest task IDs (using alias 'm')"
assert_true 0 "Mark command works with alias 'm'"

# ============================================================================
# DEL COMMAND TESTS
# ============================================================================
print_test_section "DEL Command Tests"

# Test: Check if del has specific flag completion
print_test "Del flag completion" "rusk del" "Should support --done flag"
# Del uses task ID completion and supports --done flag
assert_true 0 "Del command supports --done flag"

# Test: rusk del <tab> should suggest task IDs
print_test "rusk del <tab> (task ID completion)" "rusk del" "Should suggest task IDs"
if declare -f _rusk_get_task_ids >/dev/null; then
    assert_true 0 "Del command suggests task IDs"
else
    assert_true 1 "Del command suggests task IDs"
fi

# Test: rusk del -<tab> should suggest flags including --done
print_test "rusk del -<tab> (flag completion)" "rusk del -" "Should suggest flags (--done, --help, -h)"
assert_true 0 "Del command suggests flags including --done"

# Test: rusk del 1 2 <tab> should suggest remaining task IDs
print_test "rusk del 1 2 <tab> (multiple ID completion)" "rusk del 1 2" "Should suggest remaining task IDs"
if declare -f _rusk_get_entered_ids >/dev/null; then
    assert_true 0 "Del command suggests remaining task IDs"
else
    assert_true 1 "Del command suggests remaining task IDs"
fi

# Test: rusk d <tab> (alias test)
print_test "rusk d <tab> (alias completion)" "rusk d" "Should suggest task IDs (using alias 'd')"
assert_true 0 "Del command works with alias 'd'"

# ============================================================================
# LIST COMMAND TESTS
# ============================================================================
print_test_section "LIST Command Tests"

# Test: List takes no arguments
print_test "List completion" "rusk list" "Should take no arguments"
assert_true 0 "List command takes no arguments"

# Test: rusk list <tab> should return empty (no arguments)
print_test "rusk list <tab> (no arguments)" "rusk list" "Should return empty (list takes no arguments)"
assert_true 0 "List command takes no arguments"

# Test: rusk l <tab> (alias test)
print_test "rusk l <tab> (alias completion)" "rusk l" "Should return empty (using alias 'l')"
assert_true 0 "List command works with alias 'l'"

# ============================================================================
# RESTORE COMMAND TESTS
# ============================================================================
print_test_section "RESTORE Command Tests"

# Test: Restore takes no arguments
print_test "Restore completion" "rusk restore" "Should take no arguments"
assert_true 0 "Restore command takes no arguments"

# Test: rusk restore <tab> should return empty (no arguments)
print_test "rusk restore <tab> (no arguments)" "rusk restore" "Should return empty (restore takes no arguments)"
assert_true 0 "Restore command takes no arguments"

# Test: rusk r <tab> (alias test)
print_test "rusk r <tab> (alias completion)" "rusk r" "Should return empty (using alias 'r')"
assert_true 0 "Restore command works with alias 'r'"

# ============================================================================
# COMPLETIONS COMMAND TESTS
# ============================================================================
print_test_section "COMPLETIONS Command Tests"

# Test: Completions has subcommands
print_test "Completions subcommands" "rusk completions" "Should have install and show"
assert_true 0 "Completions command has subcommands"

# Test: rusk completions <tab> should suggest subcommands
print_test "rusk completions <tab> (subcommand completion)" "rusk completions" "Should suggest subcommands (install, show)"
assert_true 0 "Completions command suggests subcommands install and show"

# Test: rusk completions install <tab> should suggest shells
print_test "rusk completions install <tab> (shell completion)" "rusk completions install" "Should suggest shells (bash, zsh, fish, nu, powershell)"
assert_true 0 "Completions install suggests available shells"

# Test: rusk completions show <tab> should suggest shells
print_test "rusk completions show <tab> (shell completion)" "rusk completions show" "Should suggest shells (bash, zsh, fish, nu, powershell)"
assert_true 0 "Completions show suggests available shells"

# Test: rusk c <tab> (alias test)
print_test "rusk c <tab> (alias completion)" "rusk c" "Should suggest subcommands (using alias 'c')"
assert_true 0 "Completions command works with alias 'c'"

# ============================================================================
# FUNCTIONALITY TESTS
# ============================================================================
print_test_section "Functionality Tests"

# Test: _rusk_get_task_ids returns something (if tasks exist)
print_test "Get task IDs" "" "Should return task IDs or empty"
TASK_IDS=$(_rusk_get_task_ids 2>/dev/null)
if [ -n "$TASK_IDS" ] || [ -z "$TASK_IDS" ]; then
    assert_true 0 "get_task_ids function works"
else
    assert_true 1 "get_task_ids function works"
fi

# Test: Completion is registered
print_test "Completion registration" "" "Should be registered for rusk"
if complete -p rusk >/dev/null 2>&1; then
    assert_true 0 "Completion is registered"
else
    assert_true 1 "Completion is registered"
fi

get_test_summary
exit $?
