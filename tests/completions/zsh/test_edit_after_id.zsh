#!/bin/zsh
# Test: rusk e <id><tab> should append task text; rusk e <id> <tab> should offer -d/--date and help
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
_rusk_get_task_text_raw() {
    REPLY="dummy task text"
}

reset_counters

print_test_section "Zsh Completion Tests - Edit After ID"

# Test 1: rusk e 1 <tab> (with space after ID) — -d/--date and help, no text
print_test "rusk e 1 <tab> (with space after ID)" "rusk e 1" "Should return -d/--date and -h/--help for edit"
completion_candidates rusk e 1 ""
if (( ${reply[(Ie)-d]} && ${reply[(Ie)--date]} && ${reply[(Ie)-h]} && ${reply[(Ie)--help]} )) \
    && [[ " ${reply[*]} " != *"dummy task text"* ]]; then
    assert_true 0 "Completion after spaced ID offers -d/--date and -h/--help, not the text"
else
    assert_true 1 "Completion after spaced ID offers -d/--date and -h/--help, not the text (got: ${reply[*]})"
fi

# Test 2: rusk e 1<tab> (without space) - should append task text, not dates
print_test "rusk e 1<tab> (without space)" "rusk e 1" "Should append task text (NOT dates) to the typed ID"
if (( $+functions[_rusk] )) && (( $+functions[_rusk_get_task_text_raw] )); then
    _rusk_get_task_text_raw "1" 2>/dev/null
    RAW_TEXT="$REPLY"
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

# Test 3: rusk e 1 2 <tab> (multiple IDs) - no task text, no task IDs
print_test "rusk e 1 2 <tab> (multiple IDs)" "rusk e 1 2" "Should return neither task text nor task IDs"
assert_no_id_candidates rusk e 1 2 ""

# Test 4: rusk e <tab> (no id yet) — the help flags only
print_test "rusk e <tab> (no id yet)" "rusk e" "Should return -h/--help only"
completion_candidates rusk e ""
if (( ${#reply} == 2 && ${reply[(Ie)-h]} && ${reply[(Ie)--help]} )); then
    assert_true 0 "Completion before an id offers the help flags only"
else
    assert_true 1 "Completion before an id offers the help flags only (got: ${reply[*]})"
fi

get_test_summary
exit $?

