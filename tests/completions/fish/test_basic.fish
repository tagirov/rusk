#!/usr/bin/env fish
# Basic completion tests for Fish

# Note: Fish doesn't support 'set -e' like bash, we handle errors manually

set SCRIPT_DIR (dirname (status -f))
set PROJECT_ROOT (cd $SCRIPT_DIR/../../..; and pwd)
set COMPLETION_FILE "$PROJECT_ROOT/completions/rusk.fish"

echo "Fish Completion Tests - Basic"
echo "============================================================"

# Test 1: Check if completion file exists
if test -f $COMPLETION_FILE
    echo "✓ Completion file exists"
else
    echo "✗ Completion file not found: $COMPLETION_FILE"
    exit 1
end

# Test 2: Check if completion file is valid Fish syntax
if fish -n $COMPLETION_FILE
    echo "✓ Completion file has valid Fish syntax"
else
    echo "✗ Completion file has syntax errors"
    exit 1
end

echo ""
echo "All basic tests passed!"
