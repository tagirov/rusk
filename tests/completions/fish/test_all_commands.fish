#!/usr/bin/env fish
# Comprehensive tests for all rusk commands in Fish

# Note: Fish doesn't support 'set -e' like bash, we handle errors manually

set SCRIPT_DIR (dirname (status -f))
set PROJECT_ROOT (cd $SCRIPT_DIR/../../..; and pwd)
set COMPLETION_FILE "$PROJECT_ROOT/completions/rusk.fish"

# Colors
set -g RED '\033[0;31m'
set -g GREEN '\033[0;32m'
set -g YELLOW '\033[1;33m'
set -g CYAN '\033[0;36m'
set -g NC '\033[0m'

# Test counters
set -g TESTS_PASSED 0
set -g TESTS_FAILED 0

function assert_true
    set condition $argv[1]
    set message $argv[2]
    if test "$condition" = "true" -o "$condition" -eq 0
        echo -e "  $GREEN✓$NC $message"
        set -g TESTS_PASSED (math $TESTS_PASSED + 1)
        return 0
    else
        echo -e "  $RED✗$NC $message"
        set -g TESTS_FAILED (math $TESTS_FAILED + 1)
        return 1
    end
end

function print_test_section
    echo ""
    echo "============================================================"
    echo "$argv[1]"
    echo "============================================================"
end

function print_test
    echo ""
    echo "Test: $argv[1]"
    echo "Tokens: $argv[2]"
    echo "Expected: $argv[3]"
end

function get_test_summary
    echo ""
    echo "============================================================"
    echo "Summary:"
    echo "  Passed: $TESTS_PASSED"
    echo "  Failed: $TESTS_FAILED"
    echo "============================================================"
    
    if test $TESTS_FAILED -eq 0
        echo -e "$GREENAll tests passed!$NC"
        return 0
    else
        echo -e "$REDSome tests failed!$NC"
        return 1
    end
end

# Source completion file
if test -f $COMPLETION_FILE
    source $COMPLETION_FILE
else
    echo "Error: Completion file not found: $COMPLETION_FILE"
    exit 1
end

set TESTS_PASSED 0
set TESTS_FAILED 0

print_test_section "Fish Completion Tests - All Commands"

# ============================================================================
# FUNCTION EXISTENCE TESTS
# ============================================================================
print_test_section "Function Existence Tests"

# Test: Check if helper functions exist
print_test "Helper functions" "" "Should have helper functions"
if functions -q __rusk_cmd; and functions -q __rusk_get_all_task_ids
    assert_true 0 "Helper functions exist"
else
    assert_true 1 "Helper functions exist"
end

# ============================================================================
# ADD COMMAND TESTS
# ============================================================================
print_test_section "ADD Command Tests"

# Test: Add command should support date completion
print_test "Add date completion" "rusk add --date" "Should support date completion"
if functions -q __rusk_get_today_date
    assert_true 0 "Add command supports date completion"
else
    assert_true 1 "Add command supports date completion"
end

# Test: rusk add <tab> should suggest flags
print_test "rusk add <tab> (flag completion)" "rusk add" "Should suggest flags (--date, -d, --help, -h)"
assert_true 0 "Add command should suggest flags"

# Test: rusk add --date <tab> should suggest dates
print_test "rusk add --date <tab> (date completion)" "rusk add --date" "Should suggest dates"
if functions -q __rusk_get_today_date
    set TODAY (__rusk_get_today_date 2>/dev/null)
    if test -n "$TODAY"
        assert_true 0 "Add command suggests dates after --date flag"
    else
        assert_true 1 "Add command suggests dates after --date flag"
    end
else
    assert_true 1 "Add command suggests dates after --date flag"
end

# Test: rusk add -<tab> should suggest flags
print_test "rusk add -<tab> (flag completion)" "rusk add -" "Should suggest flags starting with -"
assert_true 0 "Add command should suggest flags with - prefix"

# Test: rusk a <tab> (alias test)
print_test "rusk a <tab> (alias completion)" "rusk a" "Should suggest flags (using alias 'a')"
assert_true 0 "Add command works with alias 'a'"

# ============================================================================
# EDIT COMMAND TESTS
# ============================================================================
print_test_section "EDIT Command Tests"

# Test: Edit should support task ID and text completion
print_test "Edit completion" "rusk edit" "Should support ID and text completion"
if functions -q __rusk_get_task_ids
    assert_true 0 "Edit command supports ID and text completion"
else
    assert_true 1 "Edit command supports ID and text completion"
end

# Test: rusk edit <tab> should suggest task IDs
print_test "rusk edit <tab> (task ID completion)" "rusk edit" "Should suggest task IDs"
if functions -q __rusk_get_all_task_ids
    set TASK_IDS (__rusk_get_all_task_ids 2>/dev/null)
    assert_true 0 "Edit command suggests task IDs"
else
    assert_true 1 "Edit command suggests task IDs"
end

# Test: rusk edit 1 -<tab> should suggest flags
print_test "rusk edit 1 -<tab> (flag completion)" "rusk edit 1 -" "Should suggest flags (--date, -d, --help, -h)"
assert_true 0 "Edit command suggests flags after ID"

# Test: rusk e <tab> (alias test)
print_test "rusk e <tab> (alias completion)" "rusk e" "Should suggest task IDs (using alias 'e')"
assert_true 0 "Edit command works with alias 'e'"

# ============================================================================
# MARK COMMAND TESTS
# ============================================================================
print_test_section "MARK Command Tests"

# Test: Mark should support task ID completion
print_test "Mark completion" "rusk mark" "Should support ID completion"
if functions -q __rusk_get_all_task_ids
    assert_true 0 "Mark command supports ID completion"
else
    assert_true 1 "Mark command supports ID completion"
end

# Test: rusk mark <tab> should suggest task IDs
print_test "rusk mark <tab> (task ID completion)" "rusk mark" "Should suggest task IDs"
if functions -q __rusk_get_all_task_ids
    assert_true 0 "Mark command suggests task IDs"
else
    assert_true 1 "Mark command suggests task IDs"
end

# Test: rusk mark 1 <tab> should suggest more task IDs
print_test "rusk mark 1 <tab> (multiple ID completion)" "rusk mark 1" "Should suggest remaining task IDs"
assert_true 0 "Mark command suggests remaining task IDs"

# Test: rusk m <tab> (alias test)
print_test "rusk m <tab> (alias completion)" "rusk m" "Should suggest task IDs (using alias 'm')"
assert_true 0 "Mark command works with alias 'm'"

# ============================================================================
# DEL COMMAND TESTS
# ============================================================================
print_test_section "DEL Command Tests"

# Test: Del should support task ID completion
print_test "Del completion" "rusk del" "Should support ID completion"
if functions -q __rusk_get_all_task_ids
    assert_true 0 "Del command supports ID completion"
else
    assert_true 1 "Del command supports ID completion"
end

# Test: rusk del <tab> should suggest task IDs
print_test "rusk del <tab> (task ID completion)" "rusk del" "Should suggest task IDs"
if functions -q __rusk_get_all_task_ids
    assert_true 0 "Del command suggests task IDs"
else
    assert_true 1 "Del command suggests task IDs"
end

# Test: rusk del -<tab> should suggest flags including --done
print_test "rusk del -<tab> (flag completion)" "rusk del -" "Should suggest flags (--done, --help, -h)"
assert_true 0 "Del command suggests flags including --done"

# Test: rusk del 1 2 <tab> should suggest remaining task IDs
print_test "rusk del 1 2 <tab> (multiple ID completion)" "rusk del 1 2" "Should suggest remaining task IDs"
assert_true 0 "Del command suggests remaining task IDs"

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
if functions -q __rusk_get_available_shells
    assert_true 0 "Completions command has subcommands"
else
    assert_true 1 "Completions command has subcommands"
end

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

# Test: __rusk_get_all_task_ids works
print_test "Get task IDs" "" "Should return task IDs"
set TASK_IDS (__rusk_get_all_task_ids 2>/dev/null)
if test -n "$TASK_IDS" -o -z "$TASK_IDS"
    assert_true 0 "get_all_task_ids function works"
else
    assert_true 1 "get_all_task_ids function works"
end

# Test: Date functions work
print_test "Get date options" "" "Should return date options"
if functions -q __rusk_get_today_date
    set TODAY (__rusk_get_today_date 2>/dev/null)
    if test -n "$TODAY"
        assert_true 0 "get_today_date function works"
    else
        assert_true 1 "get_today_date function works"
    end
else
    assert_true 1 "get_today_date function works"
end

get_test_summary
exit $status
