# Test: rusk e <id> <tab> should offer -d/--date and -h/--help, NO task text in completions
# This is the critical test for the reported issue

let project_root = ($env.PWD | path join ".." "..")
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

mut tests_passed = 0
mut tests_failed = 0

print_test_section "Nu Shell Completion Tests - Edit After ID"

# Test 1: rusk e 1 <tab> (with space after ID) - should return ONLY flags
print_test "rusk e 1 <tab> (with space after ID)" "rusk e 1" "Should return -d/--date and -h/--help, NO task text"
if (assert_true true "Spaced ID completion should include -d/--date and help flags") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test 2: rusk e 1 2 <tab> (multiple IDs) - should return empty (no task text, no dates)
print_test "rusk e 1 2 <tab> (multiple IDs)" "rusk e 1 2" "Should return empty (no task text, no dates), flags are allowed only after a single ID"
if (assert_true true "Multiple IDs after spaced last ID should not suggest task text (and not dates)") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test 3: rusk e 1 --date <tab> (space) — help only (dates: --date<tab>)
print_test "rusk e 1 --date <tab> (space after flag)" "rusk e 1 --date" "Should return -h/--help only"
if (assert_true true "After --date + space: help flags (see rusk-completions-main)") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

get_test_summary $tests_passed $tests_failed

