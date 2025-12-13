#!/usr/bin/env fish
# Run all Fish completion tests

# Note: Fish doesn't support 'set -e' like bash, we handle errors manually

set SCRIPT_DIR (dirname (status -f))
set TEST_FILES (find $SCRIPT_DIR -name "test_*.fish" | sort)

echo "Fish Completion Tests"
echo "============================================================"
echo "Running "(count $TEST_FILES)" test file(s)..."
echo ""

set PASSED 0
set FAILED 0

for test_file in $TEST_FILES
    echo "Running: "(basename $test_file)
    if fish $test_file 2>&1
        set PASSED (math $PASSED + 1)
        echo "✓ "(basename $test_file)" passed"
    else
        set FAILED (math $FAILED + 1)
        echo "✗ "(basename $test_file)" failed"
    end
    echo ""
end

echo "============================================================"
echo "Summary:"
echo "  Passed: $PASSED"
echo "  Failed: $FAILED"
echo "============================================================"

if test $FAILED -eq 0
    echo "All tests passed!"
    exit 0
else
    echo "Some tests failed!"
    exit 1
end
