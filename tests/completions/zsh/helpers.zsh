# Helper functions for Zsh completion tests

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# Test counters
typeset -i TESTS_PASSED=0
typeset -i TESTS_FAILED=0

# Assert functions
assert_true() {
    local condition=$1
    local message=$2
    if [[ "$condition" == "true" ]] || (( condition == 0 )) 2>/dev/null; then
        echo -e "  ${GREEN}✓${NC} $message"
        (( TESTS_PASSED++ ))
        return 0
    else
        echo -e "  ${RED}✗${NC} $message"
        (( TESTS_FAILED++ ))
        return 1
    fi
}

assert_false() {
    local condition=$1
    local message=$2
    if [[ "$condition" != "true" ]] && (( condition != 0 )) 2>/dev/null; then
        echo -e "  ${GREEN}✓${NC} $message"
        (( TESTS_PASSED++ ))
        return 0
    else
        echo -e "  ${RED}✗${NC} $message"
        (( TESTS_FAILED++ ))
        return 1
    fi
}

# Print test section
print_test_section() {
    echo ""
    echo "============================================================"
    echo "$1"
    echo "============================================================"
}

# Print test
print_test() {
    echo ""
    echo "Test: $1"
    echo "Tokens: $2"
    echo "Expected: $3"
}

# Get summary
get_test_summary() {
    echo ""
    echo "============================================================"
    echo "Summary:"
    echo "  Passed: $TESTS_PASSED"
    echo "  Failed: $TESTS_FAILED"
    echo "============================================================"
    
    if (( TESTS_FAILED == 0 )); then
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
