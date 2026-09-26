#!/usr/bin/env fish
# Test: rusk e <id><tab> completes the id, the next <tab> the task text (fish
# inserts one token per Tab and escapes it itself)

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

set -g __rusk_test_text "dummy task text"
function __rusk_get_task_text
    test -n "$__rusk_test_text"; or return 1
    printf '%s' $__rusk_test_text
end

# `commandline -opc` (the completed tokens) and `commandline -ct` (the token under the cursor)
set -g __rusk_test_words rusk e 1
set -g __rusk_test_current_word ""
function __rusk_get_cmdline
    printf '%s\n' $__rusk_test_words
end
function __rusk_get_current_word
    echo $__rusk_test_current_word
end

function candidates
    __rusk_complete_edit_text | string split0
end

# Test 1: rusk e 1<tab> (no space) - the id completes, with its text as the description
print_test "rusk e 1<tab> (without space)" "rusk e" "Should complete the id, no flags"
set -g __rusk_test_words rusk e
set -g __rusk_test_current_word 1
if test (__rusk_edit_text_slot | string join ' ') = "id 1"
    assert_true 0 "The cursor is on the id"
else
    assert_true 1 "The cursor is on the id"
end
if __rusk_should_complete_edit_flags
    assert_true 1 "Does NOT suggest flags while completing the ID"
else
    assert_true 0 "Does NOT suggest flags while completing the ID"
end
set -l got (candidates)
if test "$got" = (printf '1\tdummy task text')
    assert_true 0 "Offers the id with the start of its text"
else
    assert_true 1 "Offers the id with the start of its text (got: $got)"
end

# Test 2: rusk e 1 <tab> (with space after ID) - the text itself, fish escapes it
print_test "rusk e 1 <tab> (with space after ID)" "rusk e 1" "Should offer the task text, no flags"
set -g __rusk_test_words rusk e 1
set -g __rusk_test_current_word ""
if __rusk_should_complete_edit_flags
    assert_true 1 "Does NOT suggest flags where the text comes"
else
    assert_true 0 "Does NOT suggest flags where the text comes"
end
set -l got (candidates)
if test "$got" = (printf 'dummy task text\tTask text')
    assert_true 0 "Offers the task text"
else
    assert_true 1 "Offers the task text (got: $got)"
end

# Test 3: a text that starts with `-` comes after `--`
print_test "rusk e 1 <tab>, then rusk e 1 -- <tab> (text starts with -)" "rusk e 1" "Should offer --, then the text"
set -g __rusk_test_text "-d looks like a flag"
set -l got (candidates | string replace -r '\t.*' '')
if test "$got" = --
    assert_true 0 "Offers -- first"
else
    assert_true 1 "Offers -- first (got: $got)"
end
set -g __rusk_test_words rusk e 1 --
set -l got (candidates | string replace -r '\t.*' '')
if test "$got" = "-d looks like a flag"
    assert_true 0 "Offers the text after --"
else
    assert_true 1 "Offers the text after -- (got: $got)"
end
if __rusk_should_complete_edit_flags
    assert_true 1 "Does NOT suggest flags after --"
else
    assert_true 0 "Does NOT suggest flags after --"
end

# Test 4: a text fish cannot insert as it is (a tab), or no task: the flags
print_test "rusk e 1 <tab> (text with a tab / no such task)" "rusk e 1" "Should offer the flags"
set -g __rusk_test_words rusk e 1
for text in (printf 'a\tb') ""
    set -g __rusk_test_text $text
    set -l got (candidates)
    if contains -- -d $got; and contains -- --date $got; and contains -- -h $got; and contains -- --help $got
        assert_true 0 "Offers -d/--date and help instead of the text"
    else
        assert_true 1 "Offers -d/--date and help instead of the text (got: $got)"
    end
end

# Test 5: rusk e 1 2 <tab> (multiple IDs) - no text
print_test "rusk e 1 2 <tab> (multiple IDs)" "rusk e 1 2" "Should offer no text"
set -g __rusk_test_text "dummy task text"
set -g __rusk_test_words rusk e 1 2
if __rusk_edit_text_slot >/dev/null
    assert_true 1 "No text for more than one id"
else
    assert_true 0 "No text for more than one id"
end

# Test 6: the flags are still there once a `-` is typed
print_test "rusk e 1 -<tab>" "rusk e 1" "Should return -d/--date and -h/--help"
set -g __rusk_test_words rusk e 1
set -g __rusk_test_current_word -
if __rusk_should_complete_edit_flags
    set -l flags (__rusk_complete_edit_flags)
    if contains -- -d $flags; and contains -- --date $flags; and contains -- -h $flags; and contains -- --help $flags
        assert_true 0 "Edit flag completion lists -d/--date and -h/--help"
    else
        assert_true 1 "Edit flag completion lists -d/--date and -h/--help (got: $flags)"
    end
else
    assert_true 1 "__rusk_should_complete_edit_flags should be true for a word that starts with -"
end

get_test_summary
exit $status

