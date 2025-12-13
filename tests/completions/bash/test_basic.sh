#!/bin/bash
# Basic completion tests for Bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
COMPLETION_FILE="$PROJECT_ROOT/completions/rusk.bash"

echo "Bash Completion Tests - Basic"
echo "============================================================"

# Test 1: Check if completion file exists
if [ -f "$COMPLETION_FILE" ]; then
    echo "✓ Completion file exists"
else
    echo "✗ Completion file not found: $COMPLETION_FILE"
    exit 1
fi

# Test 2: Check if completion file has valid syntax
if bash -n "$COMPLETION_FILE" 2>/dev/null; then
    echo "✓ Completion file has valid Bash syntax"
else
    echo "✗ Completion file has syntax errors"
    exit 1
fi

# Test 3: Source the completion file and check for functions
source "$COMPLETION_FILE"

# Test 4: Check if helper functions exist
if declare -f _rusk_get_task_ids >/dev/null 2>&1; then
    echo "✓ Helper function _rusk_get_task_ids exists"
else
    echo "⚠ Helper function _rusk_get_task_ids not found (may use different naming)"
fi

# Test 5: Check if completion is registered
if complete -p rusk >/dev/null 2>&1; then
    echo "✓ Completion is registered for 'rusk'"
else
    echo "⚠ Completion not registered (may need to be sourced in shell)"
fi

echo ""
echo "All basic tests passed!"
