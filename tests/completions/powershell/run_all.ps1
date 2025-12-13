# Run all PowerShell completion tests

$ErrorActionPreference = "Stop"
$scriptPath = Split-Path -Parent $MyInvocation.MyCommand.Path
$testFiles = Get-ChildItem -Path $scriptPath -Filter "test_*.ps1" | Sort-Object Name

Write-Host "PowerShell Completion Tests" -ForegroundColor Cyan
Write-Host "=" * 60 -ForegroundColor Cyan
Write-Host "Running $($testFiles.Count) test file(s)...`n" -ForegroundColor Yellow

$allPassed = $true
$passedCount = 0
$failedCount = 0

foreach ($testFile in $testFiles) {
    Write-Host "Running: $($testFile.Name)" -ForegroundColor Yellow
    try {
        & $testFile.FullName
        if ($LASTEXITCODE -eq 0) {
            $passedCount++
            Write-Host "✓ $($testFile.Name) passed`n" -ForegroundColor Green
        } else {
            $failedCount++
            $allPassed = $false
            Write-Host "✗ $($testFile.Name) failed`n" -ForegroundColor Red
        }
    } catch {
        $failedCount++
        $allPassed = $false
        Write-Host "✗ $($testFile.Name) error: $_`n" -ForegroundColor Red
    }
}

Write-Host "=" * 60 -ForegroundColor Cyan
Write-Host "Summary:" -ForegroundColor Cyan
Write-Host "  Passed: $passedCount" -ForegroundColor Green
Write-Host "  Failed: $failedCount" -ForegroundColor $(if ($failedCount -eq 0) { "Green" } else { "Red" })
Write-Host "=" * 60 -ForegroundColor Cyan

if ($allPassed) {
    Write-Host "All tests passed!" -ForegroundColor Green
    exit 0
} else {
    Write-Host "Some tests failed!" -ForegroundColor Red
    exit 1
}
