#!/bin/bash
# Helper functions for Bash completion tests

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Test counter
TESTS_PASSED=0
TESTS_FAILED=0

# Assert functions
assert_true() {
    local condition=$1
    local message=$2
    if [ "$condition" = "true" ] || [ "$condition" -eq 0 ] 2>/dev/null; then
        echo -e "  ${GREEN}✓${NC} $message"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        echo -e "  ${RED}✗${NC} $message"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi
}

assert_false() {
    local condition=$1
    local message=$2
    if [ "$condition" != "true" ] && [ "$condition" -ne 0 ] 2>/dev/null; then
        echo -e "  ${GREEN}✓${NC} $message"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        echo -e "  ${RED}✗${NC} $message"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi
}

assert_equals() {
    local actual=$1
    local expected=$2
    local message=$3
    if [ "$actual" = "$expected" ]; then
        echo -e "  ${GREEN}✓${NC} $message"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        echo -e "  ${RED}✗${NC} $message (expected: $expected, actual: $actual)"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi
}

# Print test section header
print_test_section() {
    echo ""
    echo "============================================================"
    echo "$1"
    echo "============================================================"
}

# Print test header
print_test() {
    echo ""
    echo "Test: $1"
    echo "Tokens: $2"
    echo "Expected: $3"
}

# Get test summary
get_test_summary() {
    echo ""
    echo "============================================================"
    echo "Summary:"
    echo "  Passed: $TESTS_PASSED"
    echo "  Failed: $TESTS_FAILED"
    echo "============================================================"
    
    if [ $TESTS_FAILED -eq 0 ]; then
        echo -e "${GREEN}All tests passed!${NC}"
        return 0
    else
        echo -e "${RED}Some tests failed!${NC}"
        return 1
    fi
}

# Reset counters
reset_counters() {
    TESTS_PASSED=0
    TESTS_FAILED=0
}
