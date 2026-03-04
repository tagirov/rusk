# Basic completion tests for Nu Shell

let project_root = ($env.PWD | path join ".." "..")
let completion_file = ($project_root | path join "completions" "rusk.nu")

print "Nu Shell Completion Tests - Basic"
print "============================================================"

# Test 1: Check if completion file exists
if ($completion_file | path exists) {
    print "✓ Completion file exists"
} else {
    print $"✗ Completion file not found: ($completion_file)"
    exit 1
}

# Test 2: Check if completion file is valid Nu syntax
try {
    nu -c $"source ($completion_file); print 'Syntax OK'"
    print "✓ Completion file has valid Nu syntax"
} catch {
    print "✗ Completion file has syntax errors"
    exit 1
}

print ""
print "All basic tests passed!"
