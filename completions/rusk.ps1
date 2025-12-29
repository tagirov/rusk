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
        # Check if RUSK_DB is set in environment
        if ($env:RUSK_DB) {
            $output = & $rusk_cmd list 2>$null
        } else {
            $output = & $rusk_cmd list 2>$null
        }
        if ($output) {
            $ids = @()
            foreach ($line in $output) {
                if ($line -match '[•✔]' -and $line -match '^\s+[•✔]\s+(\d+)\s+') {
                    $id = [int]$matches[1]
                    $ids += $id
                }
            }
            $ids | Sort-Object | ForEach-Object { $_.ToString() }
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
        # Check if RUSK_DB is set in environment
        if ($env:RUSK_DB) {
            $output = & $rusk_cmd list 2>$null
        } else {
            $output = & $rusk_cmd list 2>$null
        }
        if ($output) {
            foreach ($line in $output) {
                # Convert to string if needed
                $lineStr = [string]$line
                # Match lines with status symbol and our task ID
                if ($lineStr -match "^\s+[•✔]\s+$taskId\s+") {
                    # Remove leading whitespace
                    $lineStr = $lineStr.TrimStart()
                    # Remove status symbol (• or ✔) and following spaces
                    $lineStr = $lineStr -replace '^[•✔]\s+', ''
                    # Remove task ID and following spaces
                    $lineStr = $lineStr -replace "^$taskId\s+", ''
                    
                    # Now line should contain: [date] text or just text
                    # Check if it starts with a date (dd-mm-yyyy format)
                    if ($lineStr -match '^(\d{2}-\d{2}-\d{4})\s+(.+)$') {
                        # Has date, return text after date (only if text exists)
                        $text = $matches[2].Trim()
                        if ($text -and $text.Length -gt 0) {
                            return $text
                        }
                        # Date exists but no text after it
                        return $null
                    } elseif ($lineStr -match '^(\d{2}-\d{2}-\d{4})\s*$') {
                        # Only date, no text
                        return $null
                    } elseif ($lineStr -and $lineStr.Trim().Length -gt 0) {
                        # No date, return remaining text
                        return $lineStr.Trim()
                    }
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
# Excludes the current word being completed (last token if wordToComplete is empty)
function _rusk_get_entered_ids {
    param($tokens, $wordToComplete)
    $enteredIds = @()
    # Start from index 2 to skip "rusk" and command name
    # Determine end index: exclude last token only if wordToComplete is empty AND last token is empty
    $endIndex = $tokens.Count
    if ([string]::IsNullOrEmpty($wordToComplete) -and $tokens.Count -gt 2) {
        $lastTokenValue = $tokens[$tokens.Count - 1].Value
        # Only exclude last token if it's empty (represents current word being completed)
        if ([string]::IsNullOrEmpty($lastTokenValue)) {
            $endIndex = $tokens.Count - 1
        }
    }
    for ($i = 2; $i -lt $endIndex; $i++) {
        $tokenValue = $tokens[$i].Value
        # Only count non-empty tokens that are numeric IDs
        if (-not [string]::IsNullOrEmpty($tokenValue) -and $tokenValue -match '^\d+$') {
            $enteredIds += [int]$tokenValue
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

# Check if previous token is a date flag (only check immediate previous token)
function _rusk_is_after_date_flag {
    param($prev, $tokens, $commandAst)
    # Only return true if the immediate previous token is a date flag
    if ($prev -eq '--date' -or $prev -eq '-d') {
        return $true
    }
    return $false
}

# Complete date values
# NOTE: This function should ONLY be called when we're explicitly after a date flag
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
    
    $enteredIds = _rusk_get_entered_ids $tokens $wordToComplete
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
            # When wordToComplete is empty, cursor is after the last token
            # prev is the last token (which is the actual last argument)
            $prev = $tokens[$tokens.Count - 1].Value
        } else {
            # When wordToComplete is not empty, we're typing the current word
            # prev is the token before the current word
            $prev = $tokens[$tokens.Count - 2].Value
        }
    } elseif ($tokens.Count -eq 2) {
        # Only command and current word - prev is the command
        $prev = $tokens[1].Value
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
            # Get all entered IDs from tokens to check context (excluding current word)
            $enteredIds = _rusk_get_entered_ids $tokens $wordToComplete
            
            # CRITICAL CHECK FIRST: If previous token is an ID and current word is empty,
            # we MUST suggest task text ONLY, NOT dates or anything else
            # This handles "rusk e 1 <tab>" case (with space after ID)
            # This check MUST come FIRST, before ANY other logic, to prevent dates from being suggested
            if ($prev -match '^\d+$' -and [string]::IsNullOrEmpty($cur)) {
                # Only proceed if we have exactly one ID and it matches prev
                if ($enteredIds.Count -eq 1 -and $prev -eq $enteredIds[0].ToString()) {
                    $taskText = _rusk_get_task_text $prev
                    if ($taskText -and -not [string]::IsNullOrWhiteSpace($taskText)) {
                        # Return ONLY task text, nothing else - this prevents dates from being suggested
                        # Use ListItemText and CompletionText explicitly to avoid path interpretation
                        $completionResult = [System.Management.Automation.CompletionResult]::new(
                            $taskText,           # completionText - what gets inserted
                            $taskText,           # listItemText - what shows in list
                            [System.Management.Automation.CompletionResultType]::ParameterValue,
                            "Current task text"  # toolTip
                        )
                        return @($completionResult)
                    }
                    # If no task text found, return empty - do NOT suggest dates or anything else
                    return @()
                }
                # If prev is an ID but condition above didn't match, still return empty to prevent dates
                return @()
            }
            
            # Check if we're immediately after a date flag - ONLY then suggest dates
            # This check comes AFTER the ID check to ensure dates are never suggested after an ID
            if ($prev -eq '--date' -or $prev -eq '-d') {
                if ($wordToComplete -ne '-d' -and $wordToComplete -ne '--date') {
                    return _rusk_complete_date $wordToComplete
                }
            }

            # Suggest task text if current word is a number (ID) and it's the first ID
            # This handles "rusk e 1<tab>" case (without space after ID)
            if ($cur -match '^\d+$' -and ($prev -eq 'edit' -or $prev -eq 'e')) {
                if ($enteredIds.Count -eq 0) {
                    $taskText = _rusk_get_task_text $cur
                    if ($taskText) {
                        return @([System.Management.Automation.CompletionResult]::new("$cur $taskText", "$cur $taskText", [System.Management.Automation.CompletionResultType]::ParameterValue, "Append task text"))
                    }
                }
            }

            # Complete task IDs (when at command or when typing ID)
            # Only if we're NOT after a date flag and NOT after an ID
            if ($prev -ne '--date' -and $prev -ne '-d' -and $enteredIds.Count -eq 0) {
                if ($prev -in @('edit', 'e') -or $cur -match '^\d*$' -or [string]::IsNullOrEmpty($cur)) {
                    $result = _rusk_complete_task_ids $tokens $wordToComplete
                    if ($result) {
                        return $result
                    }
                }
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
