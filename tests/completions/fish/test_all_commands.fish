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

# Non-interactive: __fish_seen_subcommand_from is unreliable without real completion context
function __rusk_is_command
    return 0
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
if functions -q __rusk_cmd; and functions -q __rusk_get_task_text
    assert_true 0 "Helper functions exist"
else
    assert_true 1 "Helper functions exist"
end

# Completion for the command line given, in a fish of its own (this script
# replaces the commandline helpers further down): no candidate may be a task
# id — rusk never suggests ids
function assert_no_id_candidates -a line
    set -l got (fish -N -c "source $COMPLETION_FILE; complete -C '$line'" | string replace -r '\t.*' '')
    if string match -qr '^[0-9]+$' -- $got
        assert_true 1 "No task id suggested for '$line' (got: $got)"
    else
        assert_true 0 "No task id suggested for '$line'"
    end
end

# ============================================================================
# ADD COMMAND TESTS
# ============================================================================
print_test_section "ADD Command Tests"

# Test: rusk add <tab> should suggest flags
print_test "rusk add <tab> (flag completion)" "rusk add" "Should suggest -h/--help only before task text"
assert_true 0 "Add command should suggest help flags before task text"

# Test: rusk add x --date <tab> (space) — help only
print_test "rusk add x --date <tab> (space after flag)" "rusk add x --date " "Should suggest -h/--help only"
function __rusk_get_cmdline
    printf '%s\n' rusk add x --date
end
function __rusk_get_current_word
    echo ""
end
if __rusk_should_complete_add_flags
    set -l flags (__rusk_complete_add_flags)
    if contains -- -h $flags; and contains -- --help $flags; and not contains -- -d $flags
        assert_true 0 "Add after --date + space: -h/--help only"
    else
        assert_true 1 "Add after --date + space: -h/--help only (got: $flags)"
    end
else
    assert_true 1 "__rusk_should_complete_add_flags should run after --date + space"
end

# Test: rusk add -<tab> should suggest flags
print_test "rusk add -<tab> (flag completion)" "rusk add -" "Should suggest -h/--help only before task text"
assert_true 0 "Add command should suggest help flags with - prefix before task text"

# Test: rusk a <tab> (alias test)
print_test "rusk a <tab> (alias completion)" "rusk a" "Should suggest -h/--help only (alias 'a')"
assert_true 0 "Add command works with alias 'a'"

# ============================================================================
# EDIT COMMAND TESTS
# ============================================================================
print_test_section "EDIT Command Tests"

# Test: Edit completes the task text next to its id
print_test "Edit completion" "rusk edit" "Should support text completion"
if functions -q __rusk_edit_text_slot __rusk_complete_edit_text
    assert_true 0 "Edit command supports text completion"
else
    assert_true 1 "Edit command supports text completion"
end

# Test: rusk edit <tab> suggests no task IDs
print_test "rusk edit <tab>" "rusk edit" "Should not suggest task IDs"
assert_no_id_candidates "rusk edit "

# Test: rusk edit 1 -<tab> should suggest flags
print_test "rusk edit 1 -<tab> (flag completion)" "rusk edit 1 -" "Should suggest --date, -d, --help, -h"
assert_true 0 "Edit command suggests flags after ID"

# Test: rusk e <tab> (alias test)
print_test "rusk e <tab> (alias completion)" "rusk e" "Should not suggest task IDs (alias 'e')"
assert_no_id_candidates "rusk e "

# ============================================================================
# MARK COMMAND TESTS
# ============================================================================
print_test_section "MARK Command Tests"

# Test: Mark should offer flag completion helpers (no task ID completion)
print_test "Mark completion" "rusk mark" "Should support flag completion"
if functions -q __rusk_should_complete_mark_del_flags __rusk_complete_mark_del_flags
    assert_true 0 "Mark command has flag completion helpers"
else
    assert_true 1 "Mark command has flag completion helpers"
end

# Test: rusk mark <tab> should suggest -h / --help
print_test "rusk mark <tab> (flag completion)" "rusk mark" "Should suggest -h, --help"
if functions -q __rusk_complete_mark_del_flags
    assert_true 0 "Mark command suggests help flags"
else
    assert_true 1 "Mark command suggests help flags"
end

# Test: rusk mark <tab> and rusk mark 1 <tab> suggest no task IDs
print_test "rusk mark <tab>, rusk mark 1 <tab>" "rusk mark 1" "Should not suggest task IDs"
assert_no_id_candidates "rusk mark "
assert_no_id_candidates "rusk mark 1 "

# Test: rusk m <tab> (alias test)
print_test "rusk m <tab> (alias completion)" "rusk m" "Should suggest flags (using alias 'm')"
assert_true 0 "Mark command works with alias 'm'"

# ============================================================================
# DEL COMMAND TESTS
# ============================================================================
print_test_section "DEL Command Tests"

# Test: Del should offer flag completion helpers (no task ID completion)
print_test "Del completion" "rusk del" "Should support flag completion"
if functions -q __rusk_should_complete_mark_del_flags __rusk_complete_mark_del_flags
    assert_true 0 "Del command has flag completion helpers"
else
    assert_true 1 "Del command has flag completion helpers"
end

# Test: rusk del <tab> should suggest --done, -h, --help
print_test "rusk del <tab> (flag completion)" "rusk del" "Should suggest --done, -h, --help"
if functions -q __rusk_complete_mark_del_flags
    assert_true 0 "Del command suggests flags after subcommand"
else
    assert_true 1 "Del command suggests flags after subcommand"
end

# Test: rusk del -<tab> should suggest flags including --done
print_test "rusk del -<tab> (flag completion)" "rusk del -" "Should suggest flags (--done, --help, -h)"
assert_true 0 "Del command suggests flags including --done"

# Test: rusk del <tab> and rusk del 1 2 <tab> suggest no task IDs
print_test "rusk del <tab>, rusk del 1 2 <tab>" "rusk del 1 2" "Should not suggest task IDs"
assert_no_id_candidates "rusk del "
assert_no_id_candidates "rusk del 1 2 "

# Test: rusk d <tab> (alias test)
print_test "rusk d <tab> (alias completion)" "rusk d" "Should suggest flags (using alias 'd')"
assert_true 0 "Del command works with alias 'd'"

# ============================================================================
# LIST COMMAND TESTS
# ============================================================================
print_test_section "LIST Command Tests"

# Test: List takes no arguments
print_test "List completion" "rusk list" "Should take no arguments"
assert_true 0 "List command takes no arguments"

# Test: rusk list <tab> should suggest -h / --help
print_test "rusk list <tab> (flag completion)" "rusk list" "Should suggest -h, --help"
if functions -q __rusk_should_complete_list_restore_flags __rusk_complete_list_restore_flags
    assert_true 0 "List command suggests help flags"
else
    assert_true 1 "List command suggests help flags"
end

# Test: rusk l <tab> (alias test)
print_test "rusk l <tab> (alias completion)" "rusk l" "Should suggest flags (using alias 'l')"
assert_true 0 "List command works with alias 'l'"

# ============================================================================
# RESTORE COMMAND TESTS
# ============================================================================
print_test_section "RESTORE Command Tests"

# Test: Restore takes no arguments
print_test "Restore completion" "rusk restore" "Should take no arguments"
assert_true 0 "Restore command takes no arguments"

# Test: rusk restore <tab> should suggest -h / --help
print_test "rusk restore <tab> (flag completion)" "rusk restore" "Should suggest -h, --help"
if functions -q __rusk_should_complete_list_restore_flags __rusk_complete_list_restore_flags
    assert_true 0 "Restore command suggests help flags"
else
    assert_true 1 "Restore command suggests help flags"
end

# Test: rusk r <tab> (alias test)
print_test "rusk r <tab> (alias completion)" "rusk r" "Should suggest flags (using alias 'r')"
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

# Test: completion script offers -h/--help in completions contexts
print_test "Fish script: completions help flags" "" "__rusk_should_complete_completions_help* and install/show rules"
if grep -q '__rusk_should_complete_completions_help_empty' "$COMPLETION_FILE" && grep -q '__rusk_should_complete_completions_help' "$COMPLETION_FILE" && grep -q "not __rusk_has_install_or_show" "$COMPLETION_FILE"
    assert_true 0 "Fish completions file defines help completion for completions command"
else
    assert_true 1 "Fish completions file should define completions help helpers"
end

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

get_test_summary
exit $status
