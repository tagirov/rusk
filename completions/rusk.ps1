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

# Check if text contains special characters that require quoting
# Special chars: | ; & > < ( ) [ ] { } $ " ' ` \ * ? ~ # @ ! % ^ = + - / : ,
function _rusk_needs_quotes {
    param([string]$text)
    return $text -match '[|;\&><\(\)\[\]\{\}\$"\''`\\\*\?\~\#\@\!\%\^\=\+\-\/\:\,]'
}

# Check if text contains single quote
function _rusk_contains_single_quote {
    param([string]$text)
    return $text.Contains("'")
}

# Quote text if it contains special characters
# Use single quotes if no single quote in text, otherwise use double quotes with escaping
function _rusk_quote_text {
    param([string]$text)
    if (-not (_rusk_needs_quotes $text)) {
        return $text
    }
    
    # If no single quote in text, use single quotes (no escaping needed)
    if (-not (_rusk_contains_single_quote $text)) {
        return "'" + $text + "'"
    } else {
        # Use double quotes with escaping
        $escaped = $text -replace '"', '\"'
        # Escape backticks to prevent command substitution
        $escaped = $escaped -replace '`', '\`'
        # Escape dollar signs to prevent variable expansion
        $escaped = $escaped -replace '\$', '\$'
        # Escape backslashes at the end
        $escaped = $escaped -replace '\\', '\\\\'
        return '"' + $escaped + '"'
    }
}

# Shell names for `rusk completions install` / `show` (exclude already-typed full names)
function _rusk_get_remaining_shells_after_install_show {
    param($tokens)
    $all = @('bash', 'zsh', 'fish', 'nu', 'powershell')
    $idx = -1
    for ($i = 2; $i -lt $tokens.Count; $i++) {
        $v = $tokens[$i].Value
        if ($v -eq 'install' -or $v -eq 'show') {
            $idx = $i
            break
        }
    }
    if ($idx -lt 0) {
        return @()
    }
    $picked = @()
    for ($j = $idx + 1; $j -lt $tokens.Count; $j++) {
        $w = $tokens[$j].Value
        if ($all -contains $w -and $picked -notcontains $w) {
            $picked += $w
        }
    }
    $all | Where-Object { $picked -notcontains $_ }
}

# Prefix for flag completion when the current token is the subcommand alias (e.g. `rusk e|`)
function _rusk_flag_completion_prefix {
    param([string]$wordToComplete, $tokens, [string]$command, [string]$cur)
    if (($tokens.Count -eq 2) -and ($cur -eq $command)) {
        return ''
    }
    return $wordToComplete
}

function _rusk_emit_flag_completions {
    param([string[]]$flags, [string]$wordToComplete, $tokens, [string]$command, [string]$cur)
    $pref = _rusk_flag_completion_prefix $wordToComplete $tokens $command $cur
    $filtered = if ([string]::IsNullOrEmpty($pref)) {
        $flags
    } else {
        $flags | Where-Object { $_ -like "$pref*" }
    }
    if (-not $filtered) {
        return @()
    }
    return $filtered | ForEach-Object {
        $t = $_
        $desc = switch ($t) {
            '--date' { 'Set task date' }
            '-d' { 'Set task date' }
            '--done' { 'Delete all completed tasks' }
            '--help' { 'Show help' }
            '-h' { 'Show help' }
            default { $t }
        }
        [System.Management.Automation.CompletionResult]::new($t, $t, [System.Management.Automation.CompletionResultType]::ParameterName, $desc)
    }
}

# Get list of task IDs from rusk list output
function _rusk_get_task_ids {
    $rusk_cmd = _rusk_get_cmd
    try {
        $output = & $rusk_cmd list 2>$null
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

# Get task text by ID (supports multi-line tasks via rusk list --for-completion)
function _rusk_get_task_text {
    param([string]$taskId)
    $rusk_cmd = _rusk_get_cmd
    try {
        $output = & $rusk_cmd list --for-completion 2>$null
        if ($output) {
            $text = ""
            $collecting = $false
            foreach ($line in $output) {
                $lineStr = [string]$line
                if ($lineStr -match "^(\d+)`t(.*)") {
                    $id = $matches[1]
                    $rest = $matches[2]
                    if ($id -eq $taskId) {
                        $text = $rest
                        $collecting = $true
                    } else {
                        $collecting = $false
                    }
                } elseif ($collecting) {
                    $text = $text + "`n" + $lineStr
                }
            }
            if ($text.Trim()) {
                return $text.Trim()
            }
        }
    } catch {
        return $null
    }
    return $null
}

# Get entered task IDs from tokens
# Excludes the current word being completed (last token if wordToComplete is empty)
function _rusk_get_entered_ids {
    param($tokens, $wordToComplete)
    $enteredIds = @()
    # Start from index 2 to skip "rusk" and command name
    # Determine end index: exclude last token only if wordToComplete is empty AND last token is empty
    $endIndex = $tokens.Count
    if ($tokens.Count -gt 2) {
        # If we are completing a non-empty current word, exclude the last token
        # because it represents the token under the cursor.
        if (-not [string]::IsNullOrEmpty($wordToComplete)) {
            $endIndex = $tokens.Count - 1
        } elseif ([string]::IsNullOrEmpty($wordToComplete)) {
            $lastTokenValue = $tokens[$tokens.Count - 1].Value
            # Only exclude last token if it's empty (represents current word being completed)
            if ([string]::IsNullOrEmpty($lastTokenValue)) {
                $endIndex = $tokens.Count - 1
            }
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

# add: at least one completed task-text token before cursor (skip value after -d/--date)
function _rusk_add_has_prior_task_text {
    param($tokens, $wordToComplete)
    $endIndex = $tokens.Count
    if ($tokens.Count -gt 2) {
        if (-not [string]::IsNullOrEmpty($wordToComplete)) {
            $endIndex = $tokens.Count - 1
        } elseif ([string]::IsNullOrEmpty($wordToComplete)) {
            $lastTokenValue = $tokens[$tokens.Count - 1].Value
            if ([string]::IsNullOrEmpty($lastTokenValue)) {
                $endIndex = $tokens.Count - 1
            }
        }
    }
    $prev = ""
    for ($i = 2; $i -lt $endIndex; $i++) {
        $w = $tokens[$i].Value
        if ([string]::IsNullOrEmpty($w)) { continue }
        if ($prev -eq '-d' -or $prev -eq '--date') {
            $prev = $w
            continue
        }
        if ($w -eq '-d' -or $w -eq '--date') {
            $prev = $w
            continue
        }
        if ($w.StartsWith('-')) {
            $prev = $w
            continue
        }
        return $true
    }
    return $false
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
            if (_rusk_is_after_date_flag $prev $tokens $commandAst) {
                if ([string]::IsNullOrEmpty($cur)) {
                    return _rusk_emit_flag_completions @('--help', '-h') $wordToComplete $tokens $command $cur
                }
                return @()
            }
            if ($cur -like '-*' -or [string]::IsNullOrEmpty($cur) -or (($cur -eq $command) -and ($tokens.Count -eq 2))) {
                $hasText = _rusk_add_has_prior_task_text $tokens $wordToComplete
                $flags = if ($hasText) {
                    @('--date', '-d', '--help', '-h')
                } else {
                    @('--help', '-h')
                }
                return _rusk_emit_flag_completions $flags $wordToComplete $tokens $command $cur
            }
            return @()
        }

        { $_ -in 'edit', 'e' } {
            $enteredIds = _rusk_get_entered_ids $tokens $wordToComplete
            
            if ($prev -eq '--date' -or $prev -eq '-d') {
                if ([string]::IsNullOrEmpty($cur)) {
                    return _rusk_emit_flag_completions @('--help', '-h') $wordToComplete $tokens $command $cur
                }
                return @()
            }

            # Suggest task text if current word is a number (ID) and it's the first ID
            # This handles "rusk e 1<tab>" case (without space after ID)
            if ($cur -match '^\d+$' -and ($prev -eq 'edit' -or $prev -eq 'e')) {
                if ($enteredIds.Count -eq 0) {
                    $taskText = _rusk_get_task_text $cur
                    if ($taskText) {
                        $quotedText = _rusk_quote_text $taskText
                        return @([System.Management.Automation.CompletionResult]::new("$cur $quotedText", "$cur $taskText", [System.Management.Automation.CompletionResultType]::ParameterValue, "Append task text"))
                    }
                }
            }

            # Default: flags when empty, flag prefix, or still on the subcommand token
            if ([string]::IsNullOrEmpty($cur) -or $cur -like '-*' -or (($cur -eq $command) -and ($tokens.Count -eq 2))) {
                return _rusk_emit_flag_completions @('--help', '-h') $wordToComplete $tokens $command $cur
            }

            return @()
        }

        { $_ -in 'mark', 'm', 'del', 'd' } {
            if ($cur -like '-*' -or [string]::IsNullOrEmpty($cur) -or (($cur -eq $command) -and ($tokens.Count -eq 2))) {
                $df = if ($command -in @('del', 'd')) {
                    @('--done', '--help', '-h')
                } else {
                    @('--help', '-h')
                }
                return _rusk_emit_flag_completions $df $wordToComplete $tokens $command $cur
            }
            return @()
        }

        { $_ -in 'list', 'l', 'restore', 'r' } {
            if ($cur -like '-*' -or [string]::IsNullOrEmpty($cur) -or (($cur -eq $command) -and ($tokens.Count -eq 2))) {
                return _rusk_emit_flag_completions @('--help', '-h') $wordToComplete $tokens $command $cur
            }
            return @()
        }

        { $_ -in 'completions', 'c' } {
            $hasInstShow = $false
            for ($i = 2; $i -lt $tokens.Count; $i++) {
                $v = $tokens[$i].Value
                if ($v -eq 'install' -or $v -eq 'show') {
                    $hasInstShow = $true
                    break
                }
            }
            if (-not $hasInstShow) {
                $subPrefix = $wordToComplete
                if ($tokens.Count -eq 2 -and $tokens[1].Value -eq $wordToComplete) {
                    $subPrefix = ''
                }
                $subcmds = if ([string]::IsNullOrEmpty($subPrefix)) {
                    @('install', 'show')
                } else {
                    @('install', 'show') | Where-Object { $_ -like "$subPrefix*" }
                }
                return $subcmds | ForEach-Object {
                    [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
                }
            }
            $available = _rusk_get_remaining_shells_after_install_show $tokens
            if ($available.Count -eq 0) {
                return @()
            }
            $shellPref = $wordToComplete
            $sf = if ([string]::IsNullOrEmpty($shellPref)) {
                $available
            } else {
                $available | Where-Object { $_ -like "$shellPref*" }
            }
            return $sf | ForEach-Object {
                [System.Management.Automation.CompletionResult]::new($_, $_, [System.Management.Automation.CompletionResultType]::ParameterValue, $_)
            }
        }
    }

    return @()
}
