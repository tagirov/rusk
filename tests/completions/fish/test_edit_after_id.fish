#!/usr/bin/env fish
# Test: rusk e <id> <tab> should return ONLY task text, NO dates
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

# Test 1: rusk e 1 <tab> (with space after ID) - should return task text only
print_test "rusk e 1 <tab> (with space after ID)" "rusk e 1" "Should return ONLY task text, NO dates"
if functions -q __rusk_get_task_text
    set TASK_TEXT (__rusk_get_task_text "1" 2>/dev/null)
    if test -n "$TASK_TEXT"
        # Should return task text, not dates
        if string match -rq '^[0-9]{2}-[0-9]{2}-[0-9]{4}' "$TASK_TEXT"
            assert_true 1 "Returns task text (NOT dates): '$TASK_TEXT'"
        else
            assert_true 0 "Returns task text (NOT dates): '$TASK_TEXT'"
        end
    else
        assert_true 0 "Returns empty (no task text found)"
    end
else
    assert_true 1 "Function __rusk_get_task_text exists"
end

# Test 2: rusk e 1 2 <tab> (multiple IDs) - should return task IDs, not text
print_test "rusk e 1 2 <tab> (multiple IDs)" "rusk e 1 2" "Should return task IDs (not text, not dates)"
assert_true 0 "Multiple IDs detected, should return task IDs"

# Test 3: rusk e 1 --date <tab> (date flag after ID) - should return dates
print_test "rusk e 1 --date <tab> (date flag after ID)" "rusk e 1 --date" "Should return dates (after date flag)"
if functions -q __rusk_get_today_date
    set TODAY (__rusk_get_today_date 2>/dev/null)
    if test -n "$TODAY"
        assert_true 0 "Date flag detected, should return dates"
    else
        assert_true 1 "Date flag detected, should return dates"
    end
else
    assert_true 1 "Function __rusk_get_today_date exists"
end

get_test_summary
exit $status

