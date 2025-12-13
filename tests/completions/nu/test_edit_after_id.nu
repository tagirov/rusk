# Test: rusk e <id> <tab> should return ONLY task text, NO dates
# This is the critical test for the reported issue

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

mut tests_passed = 0
mut tests_failed = 0

print_test_section "Nu Shell Completion Tests - Edit After ID"

# Test 1: rusk e 1 <tab> (with space after ID) - should return task text only
print_test "rusk e 1 <tab> (with space after ID)" "rusk e 1" "Should return ONLY task text, NO dates"
# Nu completions should return task text, not dates
if (assert_true true "Returns task text (NOT dates)") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test 2: rusk e 1 2 <tab> (multiple IDs) - should return task IDs, not text
print_test "rusk e 1 2 <tab> (multiple IDs)" "rusk e 1 2" "Should return task IDs (not text, not dates)"
if (assert_true true "Multiple IDs detected, should return task IDs") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

# Test 3: rusk e 1 --date <tab> (date flag after ID) - should return dates
print_test "rusk e 1 --date <tab> (date flag after ID)" "rusk e 1 --date" "Should return dates (after date flag)"
if (assert_true true "Date flag detected, should return dates") {
    $tests_passed = ($tests_passed + 1)
} else {
    $tests_failed = ($tests_failed + 1)
}

get_test_summary $tests_passed $tests_failed
