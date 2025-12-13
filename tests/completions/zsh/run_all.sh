#!/bin/zsh
# Run all Zsh completion tests

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TEST_FILES=($(find "$SCRIPT_DIR" -name "test_*.zsh" | sort))

echo "Zsh Completion Tests"
echo "============================================================"
echo "Running ${#TEST_FILES[@]} test file(s)..."
echo ""

PASSED=0
FAILED=0

for test_file in $TEST_FILES; do
    echo "Running: $(basename "$test_file")"
    if zsh "$test_file" 2>&1; then
        PASSED=$((PASSED + 1))
        echo "✓ $(basename "$test_file") passed"
    else
        FAILED=$((FAILED + 1))
        echo "✗ $(basename "$test_file") failed"
    fi
    echo ""
done

echo "============================================================"
echo "Summary:"
echo "  Passed: $PASSED"
echo "  Failed: $FAILED"
echo "============================================================"

if [ $FAILED -eq 0 ]; then
    echo "All tests passed!"
    exit 0
else
    echo "Some tests failed!"
    exit 1
fi
