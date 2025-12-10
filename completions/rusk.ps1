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
#
# Note: If tab completion shows a list but doesn't insert, try:
#   Set-PSReadLineKeyHandler -Key Tab -Function Complete
#   Set-PSReadLineKeyHandler -Chord 'Ctrl+Spacebar' -Function MenuComplete

# Find rusk binary
function _rusk_get_cmd {
    $cmd = Get-Command rusk -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }
    return "rusk"
}

# Get list of task IDs from rusk list output
function _rusk_get_task_ids {
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

# Get task text by ID
function _rusk_get_task_text {
    param([string]$taskId)
    $rusk_cmd = _rusk_get_cmd
    try {
        $output = & $rusk_cmd list 2>$null
        if ($output) {
            $line = $output | Where-Object { $_ -match '[•✔]' } | Select-String -Pattern "^\s+[•✔]\s+$taskId\s+"
            if ($line) {
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

# Get date options (today, tomorrow, week ahead, two weeks ahead)
function _rusk_get_date_options {
    $today = (Get-Date).ToString("dd-MM-yyyy")
    $tomorrow = (Get-Date).AddDays(1).ToString("dd-MM-yyyy")
    $weekAhead = (Get-Date).AddDays(7).ToString("dd-MM-yyyy")
    $twoWeeksAhead = (Get-Date).AddDays(14).ToString("dd-MM-yyyy")
    return @($today, $tomorrow, $weekAhead, $twoWeeksAhead)
}

# Get entered task IDs from tokens
function _rusk_get_entered_ids {
    param($tokens)
    $enteredIds = @()
    for ($i = 2; $i -lt $tokens.Count; $i++) {
        if ($tokens[$i].Value -match '^\d+$') {
            $enteredIds += [int]$tokens[$i].Value
        }
    }
    return $enteredIds
}

# Filter out already entered IDs from task ID list
function _rusk_filter_ids {
    param($ids, $enteredIds)
    if ($enteredIds.Count -eq 0) {
        return $ids
    }
    return $ids | Where-Object { $enteredIds -notcontains [int]$_ }
}

# Check if previous token is a date flag
function _rusk_is_after_date_flag {
    param($prev, $tokens, $commandAst)
    if ($prev -eq '--date' -or $prev -eq '-d') {
        return $true
    }
    # Check all tokens for date flag
    foreach ($token in $tokens) {
        if ($token.Value -eq '--date' -or $token.Value -eq '-d') {
            return $true
        }
    }
    # Check command text
    if ($commandAst) {
        $commandText = $commandAst.Extent.Text
        if ($commandText -match '\s-d\s' -or $commandText -match '\s--date\s' -or $commandText -match '\s-d$' -or $commandText -match '\s--date$') {
            return $true
        }
    }
    return $false
}

# Complete date values
function _rusk_complete_date {
    param($wordToComplete)
    $dates = _rusk_get_date_options
    if ([string]::IsNullOrEmpty($wordToComplete)) {
        $filtered = $dates
    } else {
        $filtered = $dates | Where-Object { $_ -like "$wordToComplete*" }
    }
    if ($filtered) {
        return $filtered | ForEach-Object {
            $desc = switch ($_) {
                { $_ -eq $dates[0] } { "Today ($_)" }
                { $_ -eq $dates[1] } { "Tomorrow ($_)" }
                { $_ -eq $dates[2] } { "One week ahead ($_)" }
                { $_ -eq $dates[3] } { "Two weeks ahead ($_)" }
                default { $_ }
            }
            [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $desc)
        }
    }
    return @()
}

# Complete task IDs with filtering and descriptions
function _rusk_complete_task_ids {
    param($tokens, $wordToComplete)
    $ids = _rusk_get_task_ids
    if (-not $ids) {
        return @()
    }
    
    $enteredIds = _rusk_get_entered_ids $tokens
    $filteredIds = _rusk_filter_ids $ids $enteredIds
    
    if ([string]::IsNullOrEmpty($wordToComplete)) {
        $filtered = $filteredIds
    } else {
        $filtered = $filteredIds | Where-Object { $_ -like "$wordToComplete*" }
    }
    
    if ($filtered) {
        return $filtered | ForEach-Object {
            $taskText = _rusk_get_task_text $_
            $description = if ($taskText) {
                $text = if ($taskText.Length -gt 40) { $taskText.Substring(0, 40) + "..." } else { $taskText }
                "Task ID $_`: $text"
            } else {
                "Task ID $_"
            }
            [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $description)
        }
    }
    return @()
}

Register-ArgumentCompleter -Native -CommandName rusk -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $tokens = $commandAst.CommandElements
    $command = $null
    $prev = $null
    $cur = $wordToComplete

    # Parse command and previous token
    if ($tokens.Count -gt 1) {
        $command = $tokens[1].Value
    }
    if ($tokens.Count -gt 2) {
        if ([string]::IsNullOrEmpty($wordToComplete)) {
            $prev = $tokens[$tokens.Count - 1].Value
        } else {
            $prev = $tokens[$tokens.Count - 2].Value
        }
    }
    
    # Complete commands (when only "rusk" is typed)
    if ($tokens.Count -eq 1) {
        $commands = @('add', 'a', 'edit', 'e', 'mark', 'm', 'del', 'd', 'list', 'l', 'restore', 'r', 'completions')
        if ([string]::IsNullOrEmpty($wordToComplete)) {
            $filtered = $commands
        } else {
            $filtered = $commands | Where-Object { $_ -like "$wordToComplete*" }
        }
        if ($filtered) {
            return $filtered | ForEach-Object {
                [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
            }
        }
        return @()
    }

    # Handle subcommands
    switch ($command) {
        { $_ -in 'add', 'a' } {
            # Complete date values after date flag
            if (_rusk_is_after_date_flag $prev $tokens $commandAst) {
                if ($wordToComplete -ne '-d' -and $wordToComplete -ne '--date') {
                    return _rusk_complete_date $wordToComplete
                }
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
            # Suggest task text if current word is a number (ID) and it's the first ID
            if ($cur -match '^\d+$' -and ($prev -eq 'edit' -or $prev -eq 'e')) {
                $enteredIds = _rusk_get_entered_ids $tokens
                if ($enteredIds.Count -eq 0) {
                    $taskText = _rusk_get_task_text $cur
                    if ($taskText) {
                        return @([System.Management.Automation.CompletionResult]::new("$cur $taskText", "$cur $taskText", [System.Management.Automation.CompletionResultType]::ParameterValue, "Append task text"))
                    }
                }
            }

            # Suggest task text if previous word is a single ID and current is empty
            if ($prev -match '^\d+$' -and [string]::IsNullOrEmpty($cur)) {
                $enteredIds = _rusk_get_entered_ids $tokens
                if ($enteredIds.Count -eq 1) {
                    $taskText = _rusk_get_task_text $prev
                    if ($taskText) {
                        return @([System.Management.Automation.CompletionResult]::new($taskText, $taskText, [System.Management.Automation.CompletionResultType]::ParameterValue, "Current task text"))
                    }
                }
            }

            # Complete date flag
            if (_rusk_is_after_date_flag $prev $tokens $commandAst) {
                if ($wordToComplete -ne '-d' -and $wordToComplete -ne '--date') {
                    return _rusk_complete_date $wordToComplete
                }
            }

            # Complete task IDs
            if ($prev -in @('edit', 'e') -or $prev -match '^\d+$' -or $cur -match '^\d*$' -or [string]::IsNullOrEmpty($cur)) {
                return _rusk_complete_task_ids $tokens $wordToComplete
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
            # Complete task IDs
            if ($prev -eq $command -or $cur -match '^\d*$' -or [string]::IsNullOrEmpty($cur)) {
                return _rusk_complete_task_ids $tokens $wordToComplete
            }

            # For del, complete flags
            if ($command -in @('del', 'd') -and $cur -like '-*') {
                return @(
                    [System.Management.Automation.CompletionResult]::new('--done', '--done', [System.Management.Automation.CompletionResultType]::ParameterName, 'Delete all completed tasks'),
                    [System.Management.Automation.CompletionResult]::new('--help', '--help', [System.Management.Automation.CompletionResultType]::ParameterName, 'Show help'),
                    [System.Management.Automation.CompletionResult]::new('-h', '-h', [System.Management.Automation.CompletionResultType]::ParameterName, 'Show help')
                )
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
            if ($tokens.Count -eq 3 -and ($tokens[2].Value -eq 'install' -or $tokens[2].Value -eq 'show')) {
                return @('bash', 'zsh', 'fish', 'nu', 'powershell') | Where-Object { $_ -like "$wordToComplete*" } | ForEach-Object {
                    [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
                }
            }
            return @()
        }
    }

    return @()
}
