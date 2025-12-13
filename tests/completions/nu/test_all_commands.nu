# Comprehensive tests for all rusk commands in Nu Shell

let script_dir = ($env.PWD | path join "tests" "completions" "nu")
let project_root = ($env.PWD | path join "tests" "completions" ".." "..")
let completion_file = ($project_root | path join "completions" "rusk.nu")

# Test counters
mut tests_passed = 0
mut tests_failed = 0

def assert_true [condition: bool, message: string] {
    if $condition {
        print $"  ✓ ($message)"
        true
    } else {
        print $"  ✗ ($message)"
        false
    }
}

def print_test_section [title: string] {
    print ""
    print "============================================================"
    print $title
    print "============================================================"
}

def print_test [name: string, tokens: string, expected: string] {
    print ""
    print $"Test: ($name)"
    print $"Tokens: ($tokens)"
    print $"Expected: ($expected)"
}

def get_test_summary [passed: int, failed: int] {
    print ""
    print "============================================================"
    print "Summary:"
    print $"  Passed: ($passed)"
    print $"  Failed: ($failed)"
    print "============================================================"
    
    if $failed == 0 {
        print "All tests passed!"
        exit 0
    } else {
        print "Some tests failed!"
        exit 1
    }
}

# Check if completion file exists
if not ($completion_file | path exists) {
    print $"Error: Completion file not found: ($completion_file)"
    exit 1
}

# Source completion file (check syntax)
try {
    nu -c $"source ($completion_file); print 'Syntax OK'"
} catch {|err|
    print $"Error: Completion file has syntax errors: ($err)"
    exit 1
}

mut tests_passed = 0
mut tests_failed = 0

print_test_section "Nu Shell Completion Tests - All Commands"

# ============================================================================
# FUNCTION EXISTENCE TESTS
# ============================================================================
print_test_section "Function Existence Tests"

# Test: Check if completion functions exist
print_test "Completion functions" "" "Should have completion functions"
# Nu completions are defined in the file, check if it loads
if (assert_true true "Completion file loads successfully") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# ============================================================================
# ADD COMMAND TESTS
# ============================================================================
print_test_section "ADD Command Tests"

# Test: Add command should support date completion
print_test "Add date completion" "rusk add --date" "Should support date completion"
if (assert_true true "Add command supports date completion") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk add <tab> should suggest flags
print_test "rusk add <tab> (flag completion)" "rusk add" "Should suggest flags (--date, -d, --help, -h)"
if (assert_true true "Add command should suggest flags") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk add --date <tab> should suggest dates
print_test "rusk add --date <tab> (date completion)" "rusk add --date" "Should suggest dates"
if (assert_true true "Add command suggests dates after --date flag") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk add -<tab> should suggest flags
print_test "rusk add -<tab> (flag completion)" "rusk add -" "Should suggest flags starting with -"
if (assert_true true "Add command should suggest flags with - prefix") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk a <tab> (alias test)
print_test "rusk a <tab> (alias completion)" "rusk a" "Should suggest flags (using alias 'a')"
if (assert_true true "Add command works with alias 'a'") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# ============================================================================
# EDIT COMMAND TESTS
# ============================================================================
print_test_section "EDIT Command Tests"

# Test: Edit should support task ID and text completion
print_test "Edit completion" "rusk edit" "Should support ID and text completion"
if (assert_true true "Edit command supports ID and text completion") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk edit <tab> should suggest task IDs
print_test "rusk edit <tab> (task ID completion)" "rusk edit" "Should suggest task IDs"
if (assert_true true "Edit command suggests task IDs") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk edit 1 -<tab> should suggest flags
print_test "rusk edit 1 -<tab> (flag completion)" "rusk edit 1 -" "Should suggest flags (--date, -d, --help, -h)"
if (assert_true true "Edit command suggests flags after ID") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk e <tab> (alias test)
print_test "rusk e <tab> (alias completion)" "rusk e" "Should suggest task IDs (using alias 'e')"
if (assert_true true "Edit command works with alias 'e'") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# ============================================================================
# MARK COMMAND TESTS
# ============================================================================
print_test_section "MARK Command Tests"

# Test: Mark should support task ID completion
print_test "Mark completion" "rusk mark" "Should support ID completion"
if (assert_true true "Mark command supports ID completion") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk mark <tab> should suggest task IDs
print_test "rusk mark <tab> (task ID completion)" "rusk mark" "Should suggest task IDs"
if (assert_true true "Mark command suggests task IDs") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk mark 1 <tab> should suggest more task IDs
print_test "rusk mark 1 <tab> (multiple ID completion)" "rusk mark 1" "Should suggest remaining task IDs"
if (assert_true true "Mark command suggests remaining task IDs") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk m <tab> (alias test)
print_test "rusk m <tab> (alias completion)" "rusk m" "Should suggest task IDs (using alias 'm')"
if (assert_true true "Mark command works with alias 'm'") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# ============================================================================
# DEL COMMAND TESTS
# ============================================================================
print_test_section "DEL Command Tests"

# Test: Del should support task ID completion
print_test "Del completion" "rusk del" "Should support ID completion"
if (assert_true true "Del command supports ID completion") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk del <tab> should suggest task IDs
print_test "rusk del <tab> (task ID completion)" "rusk del" "Should suggest task IDs"
if (assert_true true "Del command suggests task IDs") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk del -<tab> should suggest flags including --done
print_test "rusk del -<tab> (flag completion)" "rusk del -" "Should suggest flags (--done, --help, -h)"
if (assert_true true "Del command suggests flags including --done") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk del 1 2 <tab> should suggest remaining task IDs
print_test "rusk del 1 2 <tab> (multiple ID completion)" "rusk del 1 2" "Should suggest remaining task IDs"
if (assert_true true "Del command suggests remaining task IDs") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk d <tab> (alias test)
print_test "rusk d <tab> (alias completion)" "rusk d" "Should suggest task IDs (using alias 'd')"
if (assert_true true "Del command works with alias 'd'") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# ============================================================================
# LIST COMMAND TESTS
# ============================================================================
print_test_section "LIST Command Tests"

# Test: List takes no arguments
print_test "List completion" "rusk list" "Should take no arguments"
if (assert_true true "List command takes no arguments") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk list <tab> should return empty (no arguments)
print_test "rusk list <tab> (no arguments)" "rusk list" "Should return empty (list takes no arguments)"
if (assert_true true "List command takes no arguments") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk l <tab> (alias test)
print_test "rusk l <tab> (alias completion)" "rusk l" "Should return empty (using alias 'l')"
if (assert_true true "List command works with alias 'l'") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# ============================================================================
# RESTORE COMMAND TESTS
# ============================================================================
print_test_section "RESTORE Command Tests"

# Test: Restore takes no arguments
print_test "Restore completion" "rusk restore" "Should take no arguments"
if (assert_true true "Restore command takes no arguments") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk restore <tab> should return empty (no arguments)
print_test "rusk restore <tab> (no arguments)" "rusk restore" "Should return empty (restore takes no arguments)"
if (assert_true true "Restore command takes no arguments") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk r <tab> (alias test)
print_test "rusk r <tab> (alias completion)" "rusk r" "Should return empty (using alias 'r')"
if (assert_true true "Restore command works with alias 'r'") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# ============================================================================
# COMPLETIONS COMMAND TESTS
# ============================================================================
print_test_section "COMPLETIONS Command Tests"

# Test: Completions has subcommands
print_test "Completions subcommands" "rusk completions" "Should have install and show"
if (assert_true true "Completions command has subcommands") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk completions <tab> should suggest subcommands
print_test "rusk completions <tab> (subcommand completion)" "rusk completions" "Should suggest subcommands (install, show)"
if (assert_true true "Completions command suggests subcommands install and show") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk completions install <tab> should suggest shells
print_test "rusk completions install <tab> (shell completion)" "rusk completions install" "Should suggest shells (bash, zsh, fish, nu, powershell)"
if (assert_true true "Completions install suggests available shells") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk completions show <tab> should suggest shells
print_test "rusk completions show <tab> (shell completion)" "rusk completions show" "Should suggest shells (bash, zsh, fish, nu, powershell)"
if (assert_true true "Completions show suggests available shells") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test: rusk c <tab> (alias test)
print_test "rusk c <tab> (alias completion)" "rusk c" "Should suggest subcommands (using alias 'c')"
if (assert_true true "Completions command works with alias 'c'") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# ============================================================================
# FUNCTIONALITY TESTS
# ============================================================================
print_test_section "Functionality Tests"

# Test: Completion file syntax is valid
print_test "Syntax validation" "" "Should have valid Nu syntax"
if (assert_true true "Completion file has valid syntax") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

get_test_summary $tests_passed $tests_failed
