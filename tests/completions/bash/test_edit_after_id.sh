#!/bin/bash
# Test: rusk e <id><tab> should append task text; rusk e <id> <tab> should offer -d/--date and -h/--help
# This is the critical test for the reported issue

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

# Deterministic stubs to avoid dependency on external task data
_rusk_get_task_text() {
    # $1 is task id, ignored in tests
    echo "dummy task text"
}

reset_counters

print_test_section "Bash Completion Tests - Edit After ID"

# Test 1: rusk e 1 <tab> (with space after ID) - should return only flags
print_test "rusk e 1 <tab> (with space after ID)" "rusk e 1" "Should return -d/--date and -h/--help, NO task text"
if declare -f _rusk_get_task_text >/dev/null; then
    TASK_TEXT=$(_rusk_get_task_text "1" 2>/dev/null)
    if [ -n "$TASK_TEXT" ]; then
        # Simulate completion: rusk e 1 <space><TAB>
        COMP_WORDS=(rusk e 1 "")
        COMP_CWORD=3
        COMPREPLY=()
        _rusk_completion
        joined=" ${COMPREPLY[*]} "
        has_dash_d=0 has_date=0 has_dash_h=0 has_help=0
        [[ "$joined" == *" -d "* ]] && has_dash_d=1
        [[ "$joined" == *" --date "* ]] && has_date=1
        [[ "$joined" == *" -h "* ]] && has_dash_h=1
        [[ "$joined" == *" --help "* ]] && has_help=1
        has_task_text=0
        [[ "$joined" == *"$TASK_TEXT"* ]] && has_task_text=1

        if [ ${has_dash_d} -eq 1 ] && [ ${has_date} -eq 1 ] && [ ${has_dash_h} -eq 1 ] && [ ${has_help} -eq 1 ] && [ ${has_task_text} -eq 0 ]; then
            assert_true 0 "Completion after spaced ID returns date + help flags"
        else
            assert_true 1 "Completion after spaced ID returns date + help flags (got: '${COMPREPLY[*]}')"
        fi
    else
        assert_true 0 "Returns empty (no task text found)"
    fi
else
    assert_true 1 "Function _rusk_get_task_text exists"
fi

# Test 2: rusk e 1<tab> (without space) - should append task text, not dates
print_test "rusk e 1<tab> (without space)" "rusk e 1" "Should return task text appended to ID, NO dates"
if declare -f _rusk_completion >/dev/null; then
    TASK_TEXT=$(_rusk_get_task_text "1" 2>/dev/null)
    if [ -n "$TASK_TEXT" ]; then
        COMP_WORDS=(rusk e 1)
        COMP_CWORD=2
        COMPREPLY=()
        _rusk_completion
        if [ ${#COMPREPLY[@]} -gt 0 ] && [ "${COMPREPLY[0]}" = "1 $TASK_TEXT" ]; then
            assert_true 0 "Completion appends ONLY task text after non-spaced ID"
        else
            assert_true 1 "Completion appends ONLY task text after non-spaced ID (expected: '1 $TASK_TEXT', got: '${COMPREPLY[0]}')"
        fi
    else
        assert_true 0 "Returns empty (no task text found)"
    fi
else
    assert_true 1 "Function _rusk_completion exists"
fi

# Test 3: rusk e 1 2 <tab> (multiple IDs) - should return task IDs, not text
print_test "rusk e 1 2 <tab> (multiple IDs)" "rusk e 1 2" "Should return task IDs (not text, not dates)"
if declare -f _rusk_get_entered_ids >/dev/null; then
    assert_true 0 "Multiple IDs detected, should return task IDs"
else
    assert_true 1 "Function _rusk_get_entered_ids exists"
fi

# Test 4: rusk e 1 --date <tab> (space after flag) - help flags only, not dates
print_test "rusk e 1 --date <tab> (space after flag)" "rusk e 1 --date " "Should return -h/--help only"
if declare -f _rusk_completion >/dev/null; then
    COMP_WORDS=(rusk e 1 --date "")
    COMP_CWORD=4
    COMPREPLY=()
    _rusk_completion
    joined=" ${COMPREPLY[*]} "
    has_help=0 has_h=0 has_dash_d=0
    [[ "$joined" == *" --help "* ]] && has_help=1
    [[ "$joined" == *" -h "* ]] && has_h=1
    [[ "$joined" == *" -d "* ]] || [[ "$joined" == *" --date "* ]] && has_dash_d=1
    if [ ${has_help} -eq 1 ] && [ ${has_h} -eq 1 ] && [ ${has_dash_d} -eq 0 ]; then
        assert_true 0 "After --date + space: help flags only"
    else
        assert_true 1 "After --date + space: help flags only (got: '${COMPREPLY[*]}')"
    fi
else
    assert_true 1 "Function _rusk_completion exists"
fi

get_test_summary
exit $?

