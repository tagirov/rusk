# Helper functions for PowerShell completion tests

function Test-CompletionScenario {
    param(
        [string]$Description,
        [array]$Tokens,
        [string]$WordToComplete,
        [string]$ExpectedBehavior,
        [scriptblock]$Validation
    )
    
    Write-Host "`n" + "=" * 60 -ForegroundColor Cyan
    Write-Host "Test: $Description" -ForegroundColor Yellow
    Write-Host "Tokens: $($Tokens -join ' ')" -ForegroundColor Gray
    Write-Host "wordToComplete: '$WordToComplete'" -ForegroundColor Gray
    Write-Host "Expected: $ExpectedBehavior" -ForegroundColor Cyan
    
    # Calculate prev and cur (matching the logic in rusk.ps1)
    $prev = $null
    $cur = $WordToComplete
    
    # Helper to get token value (handles both strings and objects)
    function Get-TokenValue($token) {
        if ($token -is [string]) {
            return $token
        } else {
            return $token.Value
        }
    }
    
    if ($Tokens.Count -gt 2) {
        if ([string]::IsNullOrEmpty($WordToComplete)) {
            # If wordToComplete is empty, prev is the last non-empty token
            # Skip empty tokens at the end
            for ($i = $Tokens.Count - 1; $i -ge 2; $i--) {
                $tokenValue = Get-TokenValue $Tokens[$i]
                if (-not [string]::IsNullOrEmpty($tokenValue)) {
                    $prev = $tokenValue
                    break
                }
            }
            # If no non-empty token found after index 2, prev is the command (tokens[1])
            if ([string]::IsNullOrEmpty($prev) -and $Tokens.Count -gt 1) {
                $prev = Get-TokenValue $Tokens[1]
            }
        } else {
            if ($Tokens.Count -gt 2) {
                $prev = Get-TokenValue $Tokens[$Tokens.Count - 2]
            }
        }
    } elseif ($Tokens.Count -eq 2) {
        # Only command and empty token - prev is the command
        $prev = Get-TokenValue $Tokens[1]
    }
    
    Write-Host "  prev = '$prev', cur = '$cur'" -ForegroundColor White
    
    # Create mock tokens (tokens are already strings, wrap them in objects)
    $mockTokens = @()
    foreach ($t in $Tokens) {
        if ($t -is [string]) {
            $mockTokens += [PSCustomObject]@{ Value = $t }
        } else {
            $mockTokens += $t
        }
    }
    
    # Run validation
    try {
        $result = & $Validation -Tokens $mockTokens -WordToComplete $WordToComplete -Prev $prev -Cur $cur
        if ($result) {
            Write-Host "  ✓ Test passed" -ForegroundColor Green
            return $true
        } else {
            Write-Host "  ✗ Test failed" -ForegroundColor Red
            return $false
        }
    } catch {
        Write-Host "  ✗ Test error: $_" -ForegroundColor Red
        return $false
    }
}

function Assert-Equals {
    param($Actual, $Expected, $Message)
    
    if ($Actual -eq $Expected) {
        Write-Host "  ✓ $Message" -ForegroundColor Green
        return $true
    } else {
        Write-Host "  ✗ $Message (expected: $Expected, actual: $Actual)" -ForegroundColor Red
        return $false
    }
}

function Assert-NotEquals {
    param($Actual, $NotExpected, $Message)
    
    if ($Actual -ne $NotExpected) {
        Write-Host "  ✓ $Message" -ForegroundColor Green
        return $true
    } else {
        Write-Host "  ✗ $Message (unexpected value: $Actual)" -ForegroundColor Red
        return $false
    }
}

function Assert-True {
    param($Condition, $Message)
    
    if ($Condition) {
        Write-Host "  ✓ $Message" -ForegroundColor Green
        return $true
    } else {
        Write-Host "  ✗ $Message" -ForegroundColor Red
        return $false
    }
}

function Assert-False {
    param($Condition, $Message)
    
    if (-not $Condition) {
        Write-Host "  ✓ $Message" -ForegroundColor Green
        return $true
    } else {
        Write-Host "  ✗ $Message" -ForegroundColor Red
        return $false
    }
}
