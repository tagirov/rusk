# PowerShell completion script for rusk
# 
# Installation:
#   1. Automatic (recommended):
#      rusk completions install powershell
#   
#   2. Manual:
#      Generate script using rusk command:
#      
#      On Windows:
#      New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\Documents\PowerShell"
#      rusk completions show powershell | Out-File -FilePath "$env:USERPROFILE\Documents\PowerShell\rusk-completions.ps1" -Encoding utf8
#
#      On Linux/macOS:
#      New-Item -ItemType Directory -Force -Path "$HOME/.config/powershell"
#      rusk completions show powershell | Out-File -FilePath "$HOME/.config/powershell/rusk-completions.ps1" -Encoding utf8
#
#      Then add to your PowerShell profile ($PROFILE):
#      On Windows: . "$env:USERPROFILE\Documents\PowerShell\rusk-completions.ps1"
#      On Linux/macOS: . "$HOME/.config/powershell/rusk-completions.ps1"
#
# To find your PowerShell profile path, run: $PROFILE
# On Linux/macOS, PowerShell profile is usually at: ~/.config/powershell/Microsoft.PowerShell_profile.ps1
#
# Note: If tab completion shows a list but doesn't insert, try:
#   1. Set-PSReadLineKeyHandler -Key Tab -Function Complete
#   2. Or use Ctrl+Space for menu completion
#   3. Make sure PSReadLine is installed: Install-Module PSReadLine -Force
#   4. For PowerShell 5.1, you may need to use TabExpansion2 instead
#
# IMPORTANT: If completions show but don't insert, the issue is likely with PSReadLine.
# 
# This is a known issue with PowerShell's -Native completer and PSReadLine.
# The completions are working (they show the list), but PSReadLine isn't inserting them.
#
# Solution 1 (most common - try this first):
#   Set-PSReadLineKeyHandler -Key Tab -Function Complete
#   Set-PSReadLineKeyHandler -Chord 'Ctrl+Spacebar' -Function MenuComplete
#
# Solution 2 (if Solution 1 doesn't work):
#   Set-PSReadLineOption -PredictionSource None
#   Set-PSReadLineKeyHandler -Key Tab -Function Complete
#
# Solution 3 (for PowerShell 7+):
#   Install-Module PSReadLine -Force -SkipPublisherCheck
#   Import-Module PSReadLine
#   Set-PSReadLineKeyHandler -Key Tab -Function Complete
#
# Solution 4 (if nothing else works):
#   Try using the alternative registration method below (comment out -Native completer)
#   Or check PowerShell version - some versions have issues with -Native completer
#
# After making changes, restart PowerShell or reload completions:
#   . "$env:USERPROFILE\Documents\PowerShell\rusk-completions.ps1"
#
# NOTE: The completer code is correct. This is a known PowerShell/PSReadLine limitation.
# Some PowerShell versions/configurations have issues with -Native completer insertion.
#
# KNOWN ISSUE: -Native completer shows the list but doesn't insert values on Tab press.
# This is a PowerShell limitation, not a bug in the completion script.
#
# WORKAROUNDS:
# 1. Use arrow keys to navigate the list and press Enter to select
# 2. Type the full command manually (completions still help by showing available options)
# 3. Use PowerShell 7.4+ which has better -Native completer support
# 4. Consider using a different shell (Bash, Zsh, Fish, Nu) which have better completion support

function _rusk_get_cmd {
    # Find rusk binary
    $cmd = Get-Command rusk -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }
    return "rusk"
}

function _rusk_get_task_ids {
    # Get list of task IDs from rusk list output
    # Filter lines with • or ✔ to skip table headers
    $rusk_cmd = _rusk_get_cmd
    try {
        $output = & $rusk_cmd list 2>$null
        if ($output) {
            $output | Where-Object { $_ -match '[•✔]' } | Select-String -Pattern '^\s+[•✔]\s+(\d+)\s+' | ForEach-Object {
                if ($_.Matches.Success) {
                    [int]$_.Matches.Groups[1].Value
                }
            } | Sort-Object | ForEach-Object { $_.ToString() }
        }
    } catch {
        return @()
    }
}

function _rusk_get_task_text {
    param([string]$taskId)
    # Get task text by parsing rusk list output
    # Filter lines with • or ✔ to skip table headers
    $rusk_cmd = _rusk_get_cmd
    try {
        $output = & $rusk_cmd list 2>$null
        if ($output) {
            $line = $output | Where-Object { $_ -match '[•✔]' } | Select-String -Pattern "^\s+[•✔]\s+$taskId\s+"
            if ($line) {
                # Extract text after date (skip status, ID, date)
                $parts = $line.Line -split '\s+', 4
                if ($parts.Length -ge 4) {
                    return $parts[3]
                }
            }
        }
    } catch {
        return $null
    }
    return $null
}

Register-ArgumentCompleter -Native -CommandName rusk -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $tokens = $commandAst.CommandElements
    $command = $null
    $prev = $null
    $cur = $wordToComplete

    # Parse command and arguments
    if ($tokens.Count -gt 1) {
        $command = $tokens[1].Value
    }
    # prev is the token before the current word being completed
    # For -Native completer, wordToComplete is the current word being completed
    # If wordToComplete is empty, prev is the last token
    # If wordToComplete is not empty, prev is the second-to-last token
    if ($tokens.Count -gt 2) {
        if ([string]::IsNullOrEmpty($wordToComplete)) {
            # If wordToComplete is empty, we're completing after the last token
            # So prev is the last token
            $prev = $tokens[$tokens.Count - 1].Value
        } else {
            # If wordToComplete is not empty, we're completing the current token
            # So prev is the second-to-last token
            $prev = $tokens[$tokens.Count - 2].Value
        }
    }
    
    # Debug: write to file to see what tokens are being passed
    # This is more reliable than Write-Host in completers
    # Use cross-platform temp directory
    $tempDir = if ($env:TMPDIR) { $env:TMPDIR } elseif ($env:TEMP) { $env:TEMP } else { "/tmp" }
    $debugFile = Join-Path $tempDir "rusk-completions-debug.txt"
    # Get full command text from AST
    $commandText = if ($commandAst) { $commandAst.Extent.Text } else { "" }
    $debugInfo = @"
Command text: '$commandText'
Tokens: $($tokens | ForEach-Object { $_.Value })
Tokens.Count: $($tokens.Count)
wordToComplete: '$wordToComplete'
prev: '$prev'
cur: '$cur'
prev eq -d: $($prev -eq '-d')
prev eq --date: $($prev -eq '--date')
"@
    if ($tokens.Count -gt 0) {
        $debugInfo += "`nLast token: '$($tokens[$tokens.Count - 1].Value)'"
        $debugInfo += "`nAll tokens:"
        for ($i = 0; $i -lt $tokens.Count; $i++) {
            $debugInfo += "`n  [$i] = '$($tokens[$i].Value)'"
        }
    }
    # Check if command text contains -d or --date
    $debugInfo += "`nCommand contains -d: $($commandText -match '\s-d\s')"
    $debugInfo += "`nCommand contains --date: $($commandText -match '\s--date\s')"
    $debugInfo += "`n---`n"
    $debugInfo | Out-File -FilePath $debugFile -Append

    # Complete commands (when only "rusk" is typed)
    if ($tokens.Count -eq 1) {
        $commands = @('add', 'a', 'edit', 'e', 'mark', 'm', 'del', 'd', 'list', 'l', 'restore', 'r', 'completions')
        # Filter commands that match wordToComplete (if empty, return all)
        if ([string]::IsNullOrEmpty($wordToComplete)) {
            $filtered = $commands
        } else {
            $filtered = $commands | Where-Object { $_ -like "$wordToComplete*" }
        }
        if ($filtered) {
            # For -Native completer, PowerShell requires CompletionResult objects
            # The first parameter (completionText) is what gets inserted
            # Make sure completionText matches what should be inserted
            # Try returning as array to ensure proper handling
            $results = @()
            foreach ($cmd in $filtered) {
                $results += [System.Management.Automation.CompletionResult]::new($cmd, $cmd, [System.Management.Automation.CompletionResultType]::ParameterValue, $cmd)
            }
            return $results
        }
        return @()
    }

    # Handle subcommands
    switch ($command) {
        { $_ -in 'add', 'a' } {
            # Complete date values after --date or -d flag
            # Check if previous token is --date or -d
            # For "rusk a text -d <tab>", prev should be "-d" when wordToComplete is empty
            # For "rusk a text --date <tab>", prev should be "--date" when wordToComplete is empty
            # Also check all tokens and command text for --date or -d flag
            $shouldCompleteDate = $false
            if ($prev -eq '--date' -or $prev -eq '-d') {
                $shouldCompleteDate = $true
            } else {
                # Check all tokens for --date or -d flag
                foreach ($token in $tokens) {
                    if ($token.Value -eq '--date' -or $token.Value -eq '-d') {
                        $shouldCompleteDate = $true
                        break
                    }
                }
                # Also check command text if tokens don't have the flag
                if (-not $shouldCompleteDate -and $commandAst) {
                    $commandText = $commandAst.Extent.Text
                    if ($commandText -match '\s-d\s' -or $commandText -match '\s--date\s' -or $commandText -match '\s-d$' -or $commandText -match '\s--date$') {
                        $shouldCompleteDate = $true
                    }
                }
            }
            
            # Don't complete dates if user is typing -d or --date
            if ($wordToComplete -eq '-d' -or $wordToComplete -eq '--date') {
                $shouldCompleteDate = $false
            }
            
            if ($shouldCompleteDate) {
                $today = (Get-Date).ToString("dd-MM-yyyy")
                $tomorrow = (Get-Date).AddDays(1).ToString("dd-MM-yyyy")
                $weekAhead = (Get-Date).AddDays(7).ToString("dd-MM-yyyy")
                $twoWeeksAhead = (Get-Date).AddDays(14).ToString("dd-MM-yyyy")
                # Filter dates that match wordToComplete (if empty, return all)
                $dates = @($today, $tomorrow, $weekAhead, $twoWeeksAhead)
                if ([string]::IsNullOrEmpty($wordToComplete)) {
                    $filtered = $dates
                } else {
                    $filtered = $dates | Where-Object { $_ -like "$wordToComplete*" }
                }
                if ($filtered) {
                    $results = @()
                    foreach ($date in $filtered) {
                        $desc = switch ($date) {
                            $today { "Today ($today)" }
                            $tomorrow { "Tomorrow ($tomorrow)" }
                            $weekAhead { "One week ahead ($weekAhead)" }
                            $twoWeeksAhead { "Two weeks ahead ($twoWeeksAhead)" }
                            default { $date }
                        }
                        $results += [System.Management.Automation.CompletionResult]::new($date, $date, [System.Management.Automation.CompletionResultType]::ParameterValue, $desc)
                    }
                    return $results
                }
                return @()
            }
            # Complete flags
            if ($cur -like '-*' -or [string]::IsNullOrEmpty($cur)) {
                $flags = @('--date', '-d', '--help', '-h')
                if ([string]::IsNullOrEmpty($wordToComplete)) {
                    $filtered = $flags
                } else {
                    $filtered = $flags | Where-Object { $_ -like "$wordToComplete*" }
                }
                if ($filtered) {
                    return $filtered | ForEach-Object {
                        $desc = switch ($_) {
                            '--date' { 'Set task date' }
                            '-d' { 'Set task date' }
                            '--help' { 'Show help' }
                            '-h' { 'Show help' }
                            default { $_ }
                        }
                        [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterName, $desc)
                    }
                }
            }
            return @()
        }

        { $_ -in 'edit', 'e' } {
            # If current word is a number (ID), suggest task text to append after ID
            # This handles "rusk e 1<Tab>" case (without space after ID)
            # But only if this is the first ID (not multiple IDs)
            if ($cur -match '^\d+$' -and ($prev -eq 'edit' -or $prev -eq 'e')) {
                # Check if there are any other IDs already entered (should be none at this point)
                $enteredIds = @()
                for ($i = 2; $i -lt $tokens.Count; $i++) {
                    if ($tokens[$i].Value -match '^\d+$') {
                        $enteredIds += [int]$tokens[$i].Value
                    }
                }
                # Only suggest task text if this is the first ID
                if ($enteredIds.Count -eq 0) {
                    $taskText = _rusk_get_task_text $cur
                    if ($taskText) {
                        return @(
                            [System.Management.Automation.CompletionResult]::new("$cur $taskText", "$cur $taskText", [System.Management.Automation.CompletionResultType]::ParameterValue, "Append task text")
                        )
                    }
                }
            }

            # If previous word is a number (ID) and cur is empty, suggest task text
            # This handles "rusk e 1 <Tab>" case (with space after ID)
            # But only if there's a single ID (not multiple IDs)
            if ($prev -match '^\d+$' -and [string]::IsNullOrEmpty($cur)) {
                # Count how many IDs have been entered (skip first two: "rusk" and command)
                $enteredIds = @()
                for ($i = 2; $i -lt $tokens.Count; $i++) {
                    if ($tokens[$i].Value -match '^\d+$') {
                        $enteredIds += [int]$tokens[$i].Value
                    }
                }
                # Only suggest task text if there's exactly one ID
                if ($enteredIds.Count -eq 1) {
                    $taskText = _rusk_get_task_text $prev
                    if ($taskText) {
                        return @(
                            [System.Management.Automation.CompletionResult]::new($taskText, $taskText, [System.Management.Automation.CompletionResultType]::ParameterValue, "Current task text")
                        )
                    }
                }
            }

            # Complete task IDs with task text in description
            if ($prev -in @('edit', 'e') -or $prev -match '^\d+$' -or $cur -match '^\d*$' -or [string]::IsNullOrEmpty($cur)) {
                $ids = _rusk_get_task_ids
                if ($ids) {
                    # Exclude already entered IDs from suggestions
                    # Get all tokens after the command (skip first two: "rusk" and command)
                    $enteredIds = @()
                    for ($i = 2; $i -lt $tokens.Count; $i++) {
                        if ($tokens[$i].Value -match '^\d+$') {
                            $enteredIds += [int]$tokens[$i].Value
                        }
                    }
                    # Filter out entered IDs
                    if ($enteredIds.Count -gt 0) {
                        $ids = $ids | Where-Object { $enteredIds -notcontains [int]$_ }
                    }
                    $filtered = $ids | Where-Object { $_ -like "$cur*" }
                    if ($filtered) {
                        return $filtered | ForEach-Object {
                            $taskText = _rusk_get_task_text $_
                            $description = if ($taskText) {
                                $text = if ($taskText.Length -gt 40) { $taskText.Substring(0, 40) + "..." } else { $taskText }
                                "Task ID $_`: $text"
                            } else {
                                "Task ID $_"
                            }
                            # For -Native completer, use CompletionResultType enum
                            [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $description)
                        }
                    }
                }
            }

            # Complete --date flag with current date
            if ($prev -eq '--date' -or $prev -eq '-d') {
                $today = (Get-Date).ToString("dd-MM-yyyy")
                $tomorrow = (Get-Date).AddDays(1).ToString("dd-MM-yyyy")
                $weekAhead = (Get-Date).AddDays(7).ToString("dd-MM-yyyy")
                $twoWeeksAhead = (Get-Date).AddDays(14).ToString("dd-MM-yyyy")
                return @(
                    [System.Management.Automation.CompletionResult]::new($today, $today, [System.Management.Automation.CompletionResultType]::ParameterValue, "Today ($today)"),
                    [System.Management.Automation.CompletionResult]::new($tomorrow, $tomorrow, [System.Management.Automation.CompletionResultType]::ParameterValue, "Tomorrow ($tomorrow)"),
                    [System.Management.Automation.CompletionResult]::new($weekAhead, $weekAhead, [System.Management.Automation.CompletionResultType]::ParameterValue, "One week ahead ($weekAhead)"),
                    [System.Management.Automation.CompletionResult]::new($twoWeeksAhead, $twoWeeksAhead, [System.Management.Automation.CompletionResultType]::ParameterValue, "Two weeks ahead ($twoWeeksAhead)")
                )
            }

            # Complete flags
            if ($cur -like '-*') {
                return @(
                    [System.Management.Automation.CompletionResult]::new('--date', '--date', [System.Management.Automation.CompletionResultType]::ParameterName, 'Set task date'),
                    [System.Management.Automation.CompletionResult]::new('-d', '-d', [System.Management.Automation.CompletionResultType]::ParameterName, 'Set task date'),
                    [System.Management.Automation.CompletionResult]::new('--help', '--help', [System.Management.Automation.CompletionResultType]::ParameterName, 'Show help'),
                    [System.Management.Automation.CompletionResult]::new('-h', '-h', [System.Management.Automation.CompletionResultType]::ParameterName, 'Show help')
                )
            }

            return @()
        }

        { $_ -in 'mark', 'm', 'del', 'd' } {
            # Complete task IDs with task text in description
            if ($prev -eq $command -or $cur -match '^\d*$' -or [string]::IsNullOrEmpty($cur)) {
                $ids = _rusk_get_task_ids
                if ($ids) {
                    # Exclude already entered IDs from suggestions
                    # Get all tokens after the command (skip first two: "rusk" and command)
                    $enteredIds = @()
                    for ($i = 2; $i -lt $tokens.Count; $i++) {
                        if ($tokens[$i].Value -match '^\d+$') {
                            $enteredIds += [int]$tokens[$i].Value
                        }
                    }
                    # Filter out entered IDs
                    if ($enteredIds.Count -gt 0) {
                        $ids = $ids | Where-Object { $enteredIds -notcontains [int]$_ }
                    }
                    $filtered = $ids | Where-Object { $_ -like "$cur*" }
                    if ($filtered) {
                        return $filtered | ForEach-Object {
                            $taskText = _rusk_get_task_text $_
                            $description = if ($taskText) {
                                $text = if ($taskText.Length -gt 40) { $taskText.Substring(0, 40) + "..." } else { $taskText }
                                "Task ID $_`: $text"
                            } else {
                                "Task ID $_"
                            }
                            # For -Native completer, use CompletionResultType enum
                            [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $description)
                        }
                    }
                }
            }

            # For del, complete --done flag
            if ($command -in @('del', 'd')) {
                if ($cur -like '-*') {
                return @(
                    [System.Management.Automation.CompletionResult]::new('--done', '--done', [System.Management.Automation.CompletionResultType]::ParameterName, 'Delete all completed tasks'),
                    [System.Management.Automation.CompletionResult]::new('--help', '--help', [System.Management.Automation.CompletionResultType]::ParameterName, 'Show help'),
                    [System.Management.Automation.CompletionResult]::new('-h', '-h', [System.Management.Automation.CompletionResultType]::ParameterName, 'Show help')
                )
                }
            }

            return @()
        }

        { $_ -in 'list', 'l', 'restore', 'r' } {
            # These commands don't take arguments
            return @()
        }

        'completions' {
            if ($tokens.Count -eq 2) {
                return @('install', 'show') | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
                    [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
                }
            }
            if ($tokens.Count -eq 3 -and $tokens[2].Value -eq 'install') {
                return @('bash', 'zsh', 'fish', 'nu', 'powershell') | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
                    [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
                }
            }
            if ($tokens.Count -eq 3 -and $tokens[2].Value -eq 'show') {
                return @('bash', 'zsh', 'fish', 'nu', 'powershell') | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
                    [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
                }
            }
            return @()
        }
    }

    return @()
}

# Alternative registration method (if -Native doesn't work)
# If the -Native completer above doesn't insert values, try this approach:
# Comment out the Register-ArgumentCompleter -Native above and uncomment this:
#
# $ruskCompleter = {
#     param($wordToComplete, $commandAst, $cursorPosition)
#     # Copy the logic from the -Native completer above
#     # This uses TabExpansion2 which may work better
# }
# 
# if (Get-Command TabExpansion2 -ErrorAction SilentlyContinue) {
#     $function:TabExpansion2 = {
#         param($line, $lastWord)
#         if ($line -match '^rusk\s') {
#             # Call the completer logic
#             return $null
#         }
#         & (Get-Command TabExpansion2 -ErrorAction SilentlyContinue) $line $lastWord
#     }
# }

