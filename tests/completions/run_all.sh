#!/bin/bash
# Run all completion tests for all shells

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Running All Completion Tests"
echo "============================================================"
echo ""

TOTAL_PASSED=0
TOTAL_FAILED=0

# PowerShell tests
if command -v pwsh >/dev/null 2>&1; then
    echo "=== PowerShell Tests ==="
    if [ -f "powershell/run_all.ps1" ]; then
        if pwsh -File powershell/run_all.ps1; then
            TOTAL_PASSED=$((TOTAL_PASSED + 1))
        else
            TOTAL_FAILED=$((TOTAL_FAILED + 1))
        fi
    fi
    echo ""
else
    echo "=== PowerShell Tests ==="
    echo "⚠ pwsh not found, skipping PowerShell tests"
    echo ""
fi

# Bash tests
if command -v bash >/dev/null 2>&1; then
    echo "=== Bash Tests ==="
    if [ -f "bash/run_all.sh" ]; then
        if bash bash/run_all.sh; then
            TOTAL_PASSED=$((TOTAL_PASSED + 1))
        else
            TOTAL_FAILED=$((TOTAL_FAILED + 1))
        fi
    fi
    echo ""
else
    echo "=== Bash Tests ==="
    echo "⚠ bash not found, skipping Bash tests"
    echo ""
fi

# Zsh tests
if command -v zsh >/dev/null 2>&1; then
    echo "=== Zsh Tests ==="
    if [ -f "zsh/run_all.sh" ]; then
        if zsh zsh/run_all.sh; then
            TOTAL_PASSED=$((TOTAL_PASSED + 1))
        else
            TOTAL_FAILED=$((TOTAL_FAILED + 1))
        fi
    fi
    echo ""
else
    echo "=== Zsh Tests ==="
    echo "⚠ zsh not found, skipping Zsh tests"
    echo ""
fi

# Fish tests
if command -v fish >/dev/null 2>&1; then
    echo "=== Fish Tests ==="
    if [ -f "fish/run_all.fish" ]; then
        if fish fish/run_all.fish; then
            TOTAL_PASSED=$((TOTAL_PASSED + 1))
        else
            TOTAL_FAILED=$((TOTAL_FAILED + 1))
        fi
    fi
    echo ""
else
    echo "=== Fish Tests ==="
    echo "⚠ fish not found, skipping Fish tests"
    echo ""
fi

# Nu Shell tests
if command -v nu >/dev/null 2>&1; then
    echo "=== Nu Shell Tests ==="
    if [ -f "nu/run_all.nu" ]; then
        if nu nu/run_all.nu; then
            TOTAL_PASSED=$((TOTAL_PASSED + 1))
        else
            TOTAL_FAILED=$((TOTAL_FAILED + 1))
        fi
    fi
    echo ""
else
    echo "=== Nu Shell Tests ==="
    echo "⚠ nu not found, skipping Nu Shell tests"
    echo ""
fi

echo "============================================================"
echo "Overall Summary:"
echo "  Shells Passed: $TOTAL_PASSED"
echo "  Shells Failed: $TOTAL_FAILED"
echo "============================================================"

if [ $TOTAL_FAILED -eq 0 ]; then
    echo "All completion tests passed!"
    exit 0
else
    echo "Some completion tests failed!"
    exit 1
fi
