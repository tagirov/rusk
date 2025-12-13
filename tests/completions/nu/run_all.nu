# Run all Nu Shell completion tests

# Get script directory
let script_dir = ($env.PWD | path join "tests" "completions" "nu")
let test_files = (try {
    ls $script_dir | where name =~ "test_" | get name
} catch {
    # Fallback: try to find test files
    []
})

print "Nu Shell Completion Tests"
print "============================================================"
let test_count = ($test_files | length)
print "Running " + ($test_count | into string) + " test file(s)..."
print ""

mut passed = 0
mut failed = 0

for test_file in $test_files {
    let test_name = ($test_file | path basename)
    print $"Running: ($test_name)"
    let result = (try {
        nu $test_file
        {status: "passed", name: $test_name}
    } catch {|err|
        {status: "failed", name: $test_name, error: $err}
    })
    
    if $result.status == "passed" {
        $passed = ($passed + 1)
        print $"✓ ($test_name) passed"
    } else {
        $failed = ($failed + 1)
        print $"✗ ($test_name) failed: ($result.error | default "unknown error")"
    }
    print ""
}

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
