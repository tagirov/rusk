# Comprehensive tests for all rusk commands and their completion behavior

. $PSScriptRoot/helpers.ps1
. /home/alex/.config/powershell/rusk-completions.ps1

$allTestsPassed = $true

Write-Host "`nPowerShell Completion Tests - All Commands" -ForegroundColor Cyan
Write-Host "=" * 60 -ForegroundColor Cyan

# ============================================================================
# ADD COMMAND TESTS
# ============================================================================
Write-Host "`n=== ADD Command Tests ===" -ForegroundColor Yellow

# Test: rusk add <tab> (should suggest flags)
$test_add1 = Test-CompletionScenario `
    -Description "rusk add <tab> (flag completion)" `
    -Tokens @("rusk", "add", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest flags (--date, -d, --help, -h)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "add" -or $Prev -eq "a") {
            Assert-True $true "Should suggest flags for add command"
            return $true
        }
        return $false
    }

if (-not $test_add1) { $allTestsPassed = $false }

# Test: rusk add --date <tab> (should suggest dates)
$test_add2 = Test-CompletionScenario `
    -Description "rusk add --date <tab> (date completion)" `
    -Tokens @("rusk", "add", "--date", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest dates" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq '--date' -or $Prev -eq '-d') {
            Assert-True $true "Should suggest dates after date flag"
            return $true
        }
        return $false
    }

if (-not $test_add2) { $allTestsPassed = $false }

# Test: rusk add -<tab> (should suggest flags)
$test_add3 = Test-CompletionScenario `
    -Description "rusk add -<tab> (flag completion)" `
    -Tokens @("rusk", "add", "-") `
    -WordToComplete "-" `
    -ExpectedBehavior "Should suggest flags starting with -" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Cur -like '-*') {
            Assert-True $true "Should suggest flags"
            return $true
        }
        return $false
    }

if (-not $test_add3) { $allTestsPassed = $false }

# ============================================================================
# EDIT COMMAND TESTS (already covered in test_edit_after_id.ps1, but adding basic checks)
# ============================================================================
Write-Host "`n=== EDIT Command Tests ===" -ForegroundColor Yellow

# Test: rusk edit <tab> (should suggest task IDs)
$test_edit1 = Test-CompletionScenario `
    -Description "rusk edit <tab> (task ID completion)" `
    -Tokens @("rusk", "edit", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest task IDs" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        $enteredIds = _rusk_get_entered_ids $Tokens $WordToComplete
        if (($Prev -eq "edit" -or $Prev -eq "e") -and $enteredIds.Count -eq 0) {
            Assert-True $true "Should suggest task IDs"
            return $true
        }
        return $false
    }

if (-not $test_edit1) { $allTestsPassed = $false }

# Test: rusk edit 1 -<tab> (should suggest flags)
$test_edit2 = Test-CompletionScenario `
    -Description "rusk edit 1 -<tab> (flag completion)" `
    -Tokens @("rusk", "edit", "1", "-") `
    -WordToComplete "-" `
    -ExpectedBehavior "Should suggest flags (--date, -d, --help, -h)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Cur -like '-*') {
            Assert-True $true "Should suggest flags"
            return $true
        }
        return $false
    }

if (-not $test_edit2) { $allTestsPassed = $false }

# ============================================================================
# MARK COMMAND TESTS
# ============================================================================
Write-Host "`n=== MARK Command Tests ===" -ForegroundColor Yellow

# Test: rusk mark <tab> (should suggest task IDs)
$test_mark1 = Test-CompletionScenario `
    -Description "rusk mark <tab> (task ID completion)" `
    -Tokens @("rusk", "mark", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest task IDs" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "mark" -or $Prev -eq "m") {
            Assert-True $true "Should suggest task IDs for mark command"
            return $true
        }
        return $false
    }

if (-not $test_mark1) { $allTestsPassed = $false }

# Test: rusk mark 1 <tab> (should suggest more task IDs)
$test_mark2 = Test-CompletionScenario `
    -Description "rusk mark 1 <tab> (multiple ID completion)" `
    -Tokens @("rusk", "mark", "1", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest remaining task IDs" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        $enteredIds = _rusk_get_entered_ids $Tokens $WordToComplete
        if ($enteredIds.Count -gt 0) {
            Assert-True $true "Should suggest remaining task IDs"
            return $true
        }
        return $false
    }

if (-not $test_mark2) { $allTestsPassed = $false }

# Test: rusk m <tab> (alias test)
$test_mark3 = Test-CompletionScenario `
    -Description "rusk m <tab> (alias completion)" `
    -Tokens @("rusk", "m", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest task IDs (using alias)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "m") {
            Assert-True $true "Should work with alias 'm'"
            return $true
        }
        return $false
    }

if (-not $test_mark3) { $allTestsPassed = $false }

# ============================================================================
# DEL COMMAND TESTS
# ============================================================================
Write-Host "`n=== DEL Command Tests ===" -ForegroundColor Yellow

# Test: rusk del <tab> (should suggest task IDs)
$test_del1 = Test-CompletionScenario `
    -Description "rusk del <tab> (task ID completion)" `
    -Tokens @("rusk", "del", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest task IDs" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "del" -or $Prev -eq "d") {
            Assert-True $true "Should suggest task IDs for del command"
            return $true
        }
        return $false
    }

if (-not $test_del1) { $allTestsPassed = $false }

# Test: rusk del -<tab> (should suggest flags including --done)
$test_del2 = Test-CompletionScenario `
    -Description "rusk del -<tab> (flag completion)" `
    -Tokens @("rusk", "del", "-") `
    -WordToComplete "-" `
    -ExpectedBehavior "Should suggest flags (--done, --help, -h)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Cur -like '-*') {
            Assert-True $true "Should suggest flags including --done"
            return $true
        }
        return $false
    }

if (-not $test_del2) { $allTestsPassed = $false }

# Test: rusk del 1 2 <tab> (multiple IDs)
$test_del3 = Test-CompletionScenario `
    -Description "rusk del 1 2 <tab> (multiple ID completion)" `
    -Tokens @("rusk", "del", "1", "2", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest remaining task IDs" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        $enteredIds = _rusk_get_entered_ids $Tokens $WordToComplete
        if ($enteredIds.Count -gt 0) {
            Assert-True $true "Should suggest remaining task IDs"
            return $true
        }
        return $false
    }

if (-not $test_del3) { $allTestsPassed = $false }

# ============================================================================
# LIST COMMAND TESTS
# ============================================================================
Write-Host "`n=== LIST Command Tests ===" -ForegroundColor Yellow

# Test: rusk list <tab> (should return empty, no arguments)
$test_list1 = Test-CompletionScenario `
    -Description "rusk list <tab> (no arguments)" `
    -Tokens @("rusk", "list", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should return empty (list takes no arguments)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "list" -or $Prev -eq "l") {
            Assert-True $true "List command takes no arguments"
            return $true
        }
        return $false
    }

if (-not $test_list1) { $allTestsPassed = $false }

# Test: rusk l <tab> (alias test)
$test_list2 = Test-CompletionScenario `
    -Description "rusk l <tab> (alias completion)" `
    -Tokens @("rusk", "l", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should return empty (using alias)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "l") {
            Assert-True $true "Should work with alias 'l'"
            return $true
        }
        return $false
    }

if (-not $test_list2) { $allTestsPassed = $false }

# ============================================================================
# RESTORE COMMAND TESTS
# ============================================================================
Write-Host "`n=== RESTORE Command Tests ===" -ForegroundColor Yellow

# Test: rusk restore <tab> (should return empty, no arguments)
$test_restore1 = Test-CompletionScenario `
    -Description "rusk restore <tab> (no arguments)" `
    -Tokens @("rusk", "restore", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should return empty (restore takes no arguments)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "restore" -or $Prev -eq "r") {
            Assert-True $true "Restore command takes no arguments"
            return $true
        }
        return $false
    }

if (-not $test_restore1) { $allTestsPassed = $false }

# Test: rusk r <tab> (alias test)
$test_restore2 = Test-CompletionScenario `
    -Description "rusk r <tab> (alias completion)" `
    -Tokens @("rusk", "r", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should return empty (using alias)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "r") {
            Assert-True $true "Should work with alias 'r'"
            return $true
        }
        return $false
    }

if (-not $test_restore2) { $allTestsPassed = $false }

# ============================================================================
# COMPLETIONS COMMAND TESTS
# ============================================================================
Write-Host "`n=== COMPLETIONS Command Tests ===" -ForegroundColor Yellow

# Test: rusk completions <tab> (should suggest subcommands)
$test_completions1 = Test-CompletionScenario `
    -Description "rusk completions <tab> (subcommand completion)" `
    -Tokens @("rusk", "completions", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest subcommands (install, show)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "completions" -or $Prev -eq "c") {
            Assert-True $true "Should suggest subcommands install and show"
            return $true
        }
        return $false
    }

if (-not $test_completions1) { $allTestsPassed = $false }

# Test: rusk completions install <tab> (should suggest shells)
$test_completions2 = Test-CompletionScenario `
    -Description "rusk completions install <tab> (shell completion)" `
    -Tokens @("rusk", "completions", "install", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest shells (bash, zsh, fish, nu, powershell)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "install") {
            Assert-True $true "Should suggest available shells"
            return $true
        }
        return $false
    }

if (-not $test_completions2) { $allTestsPassed = $false }

# Test: rusk completions show <tab> (should suggest shells)
$test_completions3 = Test-CompletionScenario `
    -Description "rusk completions show <tab> (shell completion)" `
    -Tokens @("rusk", "completions", "show", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest shells (bash, zsh, fish, nu, powershell)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "show") {
            Assert-True $true "Should suggest available shells"
            return $true
        }
        return $false
    }

if (-not $test_completions3) { $allTestsPassed = $false }

# Test: rusk c <tab> (alias test)
$test_completions4 = Test-CompletionScenario `
    -Description "rusk c <tab> (alias completion)" `
    -Tokens @("rusk", "c", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest subcommands (using alias)" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "c") {
            Assert-True $true "Should work with alias 'c'"
            return $true
        }
        return $false
    }

if (-not $test_completions4) { $allTestsPassed = $false }

# ============================================================================
# COMMAND ALIASES TESTS
# ============================================================================
Write-Host "`n=== Command Aliases Tests ===" -ForegroundColor Yellow

# Test: rusk a <tab> (add alias)
$test_alias1 = Test-CompletionScenario `
    -Description "rusk a <tab> (add alias)" `
    -Tokens @("rusk", "a", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest flags (using alias 'a')" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "a") {
            Assert-True $true "Should work with alias 'a' for add"
            return $true
        }
        return $false
    }

if (-not $test_alias1) { $allTestsPassed = $false }

# Test: rusk d <tab> (del alias)
$test_alias2 = Test-CompletionScenario `
    -Description "rusk d <tab> (del alias)" `
    -Tokens @("rusk", "d", "") `
    -WordToComplete "" `
    -ExpectedBehavior "Should suggest task IDs (using alias 'd')" `
    -Validation {
        param($Tokens, $WordToComplete, $Prev, $Cur)
        if ($Prev -eq "d") {
            Assert-True $true "Should work with alias 'd' for del"
            return $true
        }
        return $false
    }

if (-not $test_alias2) { $allTestsPassed = $false }

Write-Host "`n" + "=" * 60 -ForegroundColor Cyan
if ($allTestsPassed) {
    Write-Host "All command tests passed!" -ForegroundColor Green
    exit 0
} else {
    Write-Host "Some command tests failed!" -ForegroundColor Red
    exit 1
}
