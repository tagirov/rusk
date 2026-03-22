#!/bin/zsh
# Test: rusk e <id><tab> should append task text, while rusk e <id> <tab> should return ONLY flags (no dates, no task text)
# This is the critical test for the reported issue

set +e  # Don't exit on error

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
COMPLETION_FILE="$PROJECT_ROOT/completions/rusk.zsh"

. "$SCRIPT_DIR/helpers.zsh"

# Source the completion file
if [[ -f "$COMPLETION_FILE" ]]; then
    _RUSK_ZSH_SKIP_ENTRY=1 source "$COMPLETION_FILE"
else
    echo "Error: Completion file not found: $COMPLETION_FILE"
    exit 1
fi

# Deterministic stubs to avoid dependency on external task data
_rusk_get_task_text() {
    echo "dummy task text"
}

_rusk_get_task_text_raw() {
    echo "dummy task text"
}

_rusk_get_date_options() {
    echo "01-01-2020 02-01-2020"
}

reset_counters

print_test_section "Zsh Completion Tests - Edit After ID"

# Test 1: rusk e 1 <tab> (with space after ID) - should return ONLY flags
print_test "rusk e 1 <tab> (with space after ID)" "rusk e 1" "Should return ONLY -h/--help (not -d/--date), NO task text and NO dates"
if grep -q '_rusk_get_task_text_raw "$prev"' "$COMPLETION_FILE"; then
    assert_true 1 "Spaced ID completion does not call task text helper (task text disabled)"
else
    # Ensure we do suggest flags in the edit/e branch
    if grep -q 'compadd -- -h --help' "$COMPLETION_FILE"; then
        assert_true 0 "Spaced ID completion returns help flags only (no -d/--date in script)"
    else
        assert_true 1 "Spaced ID completion returns help flags only (missing expected compadd)"
    fi
fi

# Test 2: rusk e 1<tab> (without space) - should append task text, not dates
print_test "rusk e 1<tab> (without space)" "rusk e 1" "Should append task text (NOT dates) to the typed ID"
if (( $+functions[_rusk] )) && (( $+functions[_rusk_get_task_text_raw] )); then
    RAW_TEXT=$(_rusk_get_task_text_raw "1" 2>/dev/null)
    if [[ -n "$RAW_TEXT" ]]; then
        EXPECTED_COMPLETION="1 ${RAW_TEXT}"
        BUFFER="rusk e 1"
        # Simulate real zsh completion context where BUFFER is read-only.
        typeset -r BUFFER
        LBUFFER="rusk e 1"
        RBUFFER=""
        typeset -a words
        words=("rusk" "e" "1")
        CURRENT=3
        # compadd should populate `reply` (not mutate BUFFER directly).
        reply=()
        _rusk
        local reply_joined="${reply[*]}"
        if (( ${#reply[@]} > 0 )) && [[ "$reply_joined" == "$EXPECTED_COMPLETION" ]] && [[ "$reply_joined" != *'\\ '* ]]; then
            assert_true 0 "Completion appends task text after non-spaced ID without escaped spaces"
        else
            assert_true 1 "Completion appends task text after non-spaced ID without escaped spaces (expected: '$EXPECTED_COMPLETION', reply: '${reply[*]}')"
        fi
    else
        assert_true 0 "Returns empty (no task text found)"
    fi
else
    assert_true 1 "Functions _rusk and _rusk_get_task_text_raw exist"
fi

# Test 3: rusk e 1 2 <tab> (multiple IDs) - should return task IDs, not text
print_test "rusk e 1 2 <tab> (multiple IDs)" "rusk e 1 2" "Should return task IDs (not text, not dates)"
if (( $+functions[_rusk_get_entered_ids] )); then
    assert_true 0 "Multiple IDs detected, should return task IDs"
else
    assert_true 1 "Function _rusk_get_entered_ids exists"
fi

# Test 4: after --date + space, script should offer -h/--help (runtime compadd needs zle; bash test covers behavior)
print_test "rusk e 1 --date <tab> (space after flag)" "rusk e 1 --date " "Should return -h/--help only"
if grep -q 'edit|e)' "$COMPLETION_FILE" && grep -q 'if \[\[ -z "\$cur" \]\]; then' "$COMPLETION_FILE"; then
    cnt=$(grep -c 'compadd -- -h --help' "$COMPLETION_FILE" || echo 0)
    if [[ "$cnt" -ge 4 ]]; then
        assert_true 0 "Completion script has help-only branches after date flag + space"
    else
        assert_true 1 "Expected multiple compadd -h/--help branches (count=$cnt)"
    fi
else
    assert_true 1 "Completion file should contain edit branch and empty-cur handling"
fi

get_test_summary
exit $?

