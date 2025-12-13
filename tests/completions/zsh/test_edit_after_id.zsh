#!/bin/zsh
# Test: rusk e <id> <tab> should return ONLY task text, NO dates
# This is the critical test for the reported issue

set +e  # Don't exit on error

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
COMPLETION_FILE="$PROJECT_ROOT/completions/rusk.zsh"

. "$SCRIPT_DIR/helpers.zsh"

# Source the completion file
if [[ -f "$COMPLETION_FILE" ]]; then
    source "$COMPLETION_FILE"
else
    echo "Error: Completion file not found: $COMPLETION_FILE"
    exit 1
fi

reset_counters

print_test_section "Zsh Completion Tests - Edit After ID"

# Test 1: rusk e 1 <tab> (with space after ID) - should return task text only
print_test "rusk e 1 <tab> (with space after ID)" "rusk e 1" "Should return ONLY task text, NO dates"
if (( $+functions[_rusk_get_task_text] )); then
    TASK_TEXT=$(_rusk_get_task_text "1" 2>/dev/null)
    if [[ -n "$TASK_TEXT" ]]; then
        # Should return task text, not dates
        if [[ "$TASK_TEXT" =~ ^[0-9]{2}-[0-9]{2}-[0-9]{4} ]]; then
            assert_true 1 "Returns task text (NOT dates): '$TASK_TEXT'"
        else
            assert_true 0 "Returns task text (NOT dates): '$TASK_TEXT'"
        fi
    else
        assert_true 0 "Returns empty (no task text found)"
    fi
else
    assert_true 1 "Function _rusk_get_task_text exists"
fi

# Test 2: rusk e 1 2 <tab> (multiple IDs) - should return task IDs, not text
print_test "rusk e 1 2 <tab> (multiple IDs)" "rusk e 1 2" "Should return task IDs (not text, not dates)"
if (( $+functions[_rusk_get_entered_ids] )); then
    assert_true 0 "Multiple IDs detected, should return task IDs"
else
    assert_true 1 "Function _rusk_get_entered_ids exists"
fi

# Test 3: rusk e 1 --date <tab> (date flag after ID) - should return dates
print_test "rusk e 1 --date <tab> (date flag after ID)" "rusk e 1 --date" "Should return dates (after date flag)"
if (( $+functions[_rusk_get_date_options] )); then
    DATE_OPTIONS=($(_rusk_get_date_options 2>/dev/null))
    if (( ${#DATE_OPTIONS[@]} > 0 )); then
        assert_true 0 "Date flag detected, should return dates"
    else
        assert_true 1 "Date flag detected, should return dates"
    fi
else
    assert_true 1 "Function _rusk_get_date_options exists"
fi

get_test_summary
exit $?

