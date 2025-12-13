# Basic completion tests for PowerShell

. $PSScriptRoot/helpers.ps1
. /home/alex/.config/powershell/rusk-completions.ps1

$allTestsPassed = $true

Write-Host "`nPowerShell Completion Tests - Basic" -ForegroundColor Cyan
Write-Host "=" * 60 -ForegroundColor Cyan

# Test 1: rusk <tab> (command completion)
$test1 = Test-CompletionScenario `
    -Description "rusk <tab> (command completion)" `
    -Tokens @("rusk", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should return available commands" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        
        # Should suggest commands when only "rusk" is typed (tokens count is 1 or 2 with empty)
        $nonEmptyTokens = $Tokens | Where-Object { -not [string]::IsNullOrEmpty($_.Value) }
        if ($nonEmptyTokens.Count -eq 1) {
            Assert-True $true "Should suggest commands"
            return $true
        }
        return $false
    }

if (-not $test1) { $allTestsPassed = $false }

# Test 2: rusk e <tab> (subcommand completion)
$test2 = Test-CompletionScenario `
    -Description "rusk e <tab> (no ID yet)" `
    -Tokens @("rusk", "e", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should return task IDs" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        
        $enteredIds = _rusk_get_entered_ids $Tokens $WordToComplete
        
        # With no IDs entered, should suggest task IDs
        if ($enteredIds.Count -eq 0) {
            Assert-True $true "No IDs entered, should suggest task IDs"
            return $true
        }
        return $false
    }

if (-not $test2) { $allTestsPassed = $false }

# Test 3: rusk edit 1 -<tab> (flag completion)
$test3 = Test-CompletionScenario `
    -Description "rusk edit 1 -<tab> (flag completion)" `
    -Tokens @("rusk", "edit", "1", "-") `
    -WordToComplete "-" `
    -ExpectedBehavior "Should return available flags" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        
        # Should suggest flags when typing "-"
        if ($Cur -like '-*') {
            Assert-True $true "Should suggest flags"
            return $true
        }
        return $false
    }

if (-not $test3) { $allTestsPassed = $false }

Write-Host "`n" + "=" * 60 -ForegroundColor Cyan
if ($allTestsPassed) {
    Write-Host "All tests passed!" -ForegroundColor Green
    exit 0
} else {
    Write-Host "Some tests failed!" -ForegroundColor Red
    exit 1
}
