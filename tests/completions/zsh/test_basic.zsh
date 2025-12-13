#!/bin/zsh
# Basic completion tests for Zsh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
COMPLETION_FILE="$PROJECT_ROOT/completions/rusk.zsh"

echo "Zsh Completion Tests - Basic"
echo "============================================================"

# Test 1: Check if completion file exists
if [ -f "$COMPLETION_FILE" ]; then
    echo "✓ Completion file exists"
else
    echo "✗ Completion file not found: $COMPLETION_FILE"
    exit 1
fi

# Source the completion file
source "$COMPLETION_FILE"

# Test 2: Check if completion function exists
if (( $+functions[_rusk] )); then
    echo "✓ Completion function _rusk exists"
else
    echo "✗ Completion function _rusk not found"
    exit 1
fi

echo ""
echo "All basic tests passed!"
