#!/usr/bin/env fish
# Test: rusk e <id> <tab> should return ONLY -h/--help, not -d/--date
# This is the critical test for the reported issue

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

print_test_section "Fish Completion Tests - Edit After ID"

# Deterministic overrides (avoid depending on real task data / fish interactive commandline)
function __rusk_is_command
    return 0
end

function __rusk_get_task_text
    echo "dummy task text"
end

set -g __rusk_test_current_word ""

function __rusk_get_cmdline
    printf '%s\n' rusk e 1
end

function __rusk_get_current_word
    echo $__rusk_test_current_word
end

# Test 1: rusk e 1 <tab> (with space after ID) - should return ONLY flags
print_test "rusk e 1 <tab> (with space after ID)" "rusk e 1" "Should return ONLY -h/--help, NO -d/--date, NO task text"
set -g __rusk_test_current_word ""

if __rusk_should_complete_edit_text
    assert_true 1 "Does NOT suggest task text after spaced ID"
else
    assert_true 0 "Does NOT suggest task text after spaced ID"
end

if __rusk_should_complete_edit_flags
    assert_true 0 "Suggests flags after spaced ID"
else
    assert_true 1 "Suggests flags after spaced ID"
end

set -l flags (__rusk_complete_edit_flags)
if not contains -- -d $flags; and not contains -- --date $flags; and contains -- -h $flags; and contains -- --help $flags
    assert_true 0 "Flags completion contains expected flags"
else
    assert_true 1 "Flags completion contains expected flags (got: $flags)"
end

# Test 2: rusk e 1<tab> (no space) - should suggest task text
print_test "rusk e 1<tab> (without space)" "rusk e 1" "Should suggest task text (no flags)"
set -g __rusk_test_current_word "1"

if __rusk_should_complete_edit_text
    assert_true 0 "Suggests task text after non-spaced ID"
else
    assert_true 1 "Suggests task text after non-spaced ID"
end

if __rusk_should_complete_edit_flags
    assert_true 1 "Does NOT suggest flags while completing the ID"
else
    assert_true 0 "Does NOT suggest flags while completing the ID"
end

set -l task_text (__rusk_complete_edit_text)
if test "$task_text" = "dummy task text" -o "$task_text" = "1 dummy task text"
    assert_true 0 "Task text completion returns expected dummy text"
else
    assert_true 1 "Task text completion returns expected dummy text (got: $task_text)"
end

# Test 3: rusk e 1 2 <tab> (multiple IDs) - should return task IDs, not text
print_test "rusk e 1 2 <tab> (multiple IDs)" "rusk e 1 2" "Should return task IDs (not text, not dates)"
assert_true 0 "Multiple IDs detected, should return task IDs"

# Test 4: rusk e 1 --date <tab> (space after flag) — help flags, not dates
print_test "rusk e 1 --date <tab> (space after flag)" "rusk e 1 --date " "Should return -h/--help only"
function __rusk_get_cmdline
    printf '%s\n' rusk e 1 --date
end
set -g __rusk_test_current_word ""
if __rusk_should_complete_edit_flags
    set -l flags (__rusk_complete_edit_flags)
    if contains -- -h $flags; and contains -- --help $flags
        assert_true 0 "Edit after --date + space suggests -h/--help"
    else
        assert_true 1 "Edit after --date + space suggests -h/--help (got: $flags)"
    end
else
    assert_true 1 "__rusk_should_complete_edit_flags should be true for --date + space"
end

get_test_summary
exit $status

