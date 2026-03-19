# Fish completion script for rusk
#
# Installation:
#   1. Automatic (recommended):
#      rusk completions install fish
#
#   2. Manual:
#      Generate script using rusk command:
#      mkdir -p ~/.config/fish/completions
#      rusk completions show fish > ~/.config/fish/completions/rusk.fish
#
#      Completions will be automatically loaded by Fish shell

# ============================================================================
# Tab Completion Wrapper (removes backslash escaping for rusk edit)
# ============================================================================

# Custom Tab handler that removes backslash escaping and unnecessary quotes for rusk edit command
function __rusk_complete_and_unescape
    # Get command line before completion
    set -l cmd_before (commandline)
    
    # Perform standard completion
    commandline -f complete
    
    # Get command line after completion
    set -l cmd_after (commandline)
    
    # Check if command contains "rusk e" or "rusk edit" followed by ID and completion changed
    # This pattern works with or without environment variables (e.g., "RUSK_DB=./ rusk e 7")
    if test "$cmd_before" != "$cmd_after"
        # fish sometimes escapes the space after the id as `22\ <text>`
        # but matching the exact escape pattern is brittle, so just check
        # for "rusk (e|edit) <id>" and rely on later parsing after unescape.
        set -l __rusk_match1 0
        if string match -qr 'rusk\s+(e|edit)\s+\d+' -- "$cmd_after"
            set __rusk_match1 1
        end

        if test $__rusk_match1 -eq 1
            # Remove script escapes, then strip any remaining `\ ` (Fish escaped spaces) in the buffer.
            set -l unescaped (string replace -a '\ ' ' ' -- (string unescape -- "$cmd_after"))
            
            # Remove unnecessary quotes that fish may have added when using environment variables
            # Extract everything before the task text and the task text itself
            # Pattern: everything up to and including "rusk e <id> " or "rusk edit <id> "
            set -l match_result (string match -r '(.*\brusk\s+(e|edit)\s+\d+)\s+(.*)' -- "$unescaped")
            if test (count $match_result) -ge 4
                # match_result[1] is the full match
                # match_result[2] is everything up to and including the ID
                # match_result[3] is the command (e or edit) 
                # match_result[4] is the text after the ID
                set -l prefix "$match_result[2] "
                set -l text_after_id $match_result[4]
                set text_after_id (string replace -a '\ ' ' ' -- "$text_after_id")
                
                # Do not rewrite flag completion after ID (e.g. "-", "-d", "--date").
                # Rewriting here can produce "'-'" instead of flag suggestions.
                if not string match -qr '^-' -- "$text_after_id"
                    # First, remove escaped quotes (\" becomes ")
                    set text_after_id (string replace -a '\\"' '"' -- "$text_after_id")
                    set text_after_id (string replace -a "\\'" "'" -- "$text_after_id")
                    
                    # Remove surrounding quotes if present (both single and double quotes)
                    # Check for double quotes first
                    if string match -qr '^".*"$' -- "$text_after_id"
                        # Remove surrounding double quotes
                        set text_after_id (string replace -r '^"(.*)"$' '$1' -- "$text_after_id")
                    else
                        # Check for single quotes by checking first and last character
                        set -l first_char (string sub -s 1 -l 1 -- "$text_after_id")
                        set -l last_char (string sub -s -1 -l 1 -- "$text_after_id")
                        if test "$first_char" = "'" -a "$last_char" = "'"
                            # Remove surrounding single quotes
                            set text_after_id (string sub -s 2 -e -2 -- "$text_after_id")
                        end
                    end
                    
                    # Remove any trailing whitespace and extra characters that fish may have added
                    # This fixes issues where fish adds extra characters like "e" at the end
                    set text_after_id (string trim -- "$text_after_id")
                    
                    # Remove any trailing single characters that might be command aliases (e, a, m, d, etc.)
                    # Fish may add these when using environment variables
                    # Command aliases: e (edit), a (add), m (mark), d (del), l (list), r (restore), c (completions)
                    set -l command_aliases "e" "a" "m" "d" "l" "r" "c"
                    
                    # Only remove trailing command aliases if they are preceded by a space
                    # This prevents removing letters from words like "Editor" (r), "Code" (e), etc.
                    while test (string length -- "$text_after_id") -gt 1
                        set -l last_char (string sub -s -1 -l 1 -- "$text_after_id")
                        set -l second_last_char (string sub -s -2 -l 1 -- "$text_after_id")
                        
                        # Only consider it an alias if it's preceded by a space
                        if test "$second_last_char" != " "
                            # Not preceded by space, so it's part of a word - don't remove
                            break
                        end
                        
                        # Check if last char is a command alias
                        set -l is_alias false
                        for alias in $command_aliases
                            if test "$last_char" = "$alias"
                                set is_alias true
                                break
                            end
                        end
                        
                        if test "$is_alias" = "true"
                            # Remove the alias character and the space before it
                            set text_after_id (string sub -s 1 -e -2 -- "$text_after_id" | string trim)
                        else
                            # Not an alias, stop removing
                            break
                        end
                    end
                    
                    # Simply reconstruct the command without adding extra escaping
                    # Fish will handle quoting as needed
                    set -l quoted (__rusk_quote_text "$text_after_id" | string collect | string trim)
                    set unescaped "$prefix$quoted"
                end
            end
            
            commandline -r -- "$unescaped"
        end
    end
end

# Bind Tab to our custom function (only in rusk context, falls back to default otherwise)
bind \t __rusk_complete_and_unescape

# ============================================================================
# Utility Functions
# ============================================================================

# Find rusk binary
function __rusk_cmd
    command -v rusk 2>/dev/null; or echo rusk
end

# Get command line arguments
function __rusk_get_cmdline
    commandline -opc
end

# Get command line arguments including current word under cursor.
# Fish's `commandline -opc` excludes the current token, which breaks cases like:
#   rusk edit 22<tab>
function __rusk_get_cmdline_with_current
    set -l cmdline (__rusk_get_cmdline)
    set -l current_word (__rusk_get_current_word)
    if test -n "$current_word"
        if test (count $cmdline) -eq 0
            set -a cmdline "$current_word"
        else
            set -l last_token $cmdline[-1]
            if test "$last_token" != "$current_word"
                set -a cmdline "$current_word"
            end
        end
    end
    for arg in $cmdline
        echo $arg
    end
end

# Get current word being typed
function __rusk_get_current_word
    commandline -ct
end

# Check if word matches pattern
function __rusk_is_flag
    string match -qr '^-' -- "$argv[1]"
end

# Check if word is a number (task ID)
function __rusk_is_number
    string match -qr '^[0-9]+$' -- "$argv[1]"
end

# ============================================================================
# Task Management Functions
# ============================================================================

# Get all task IDs from rusk list output
function __rusk_get_all_task_ids
    set -l rusk_cmd (__rusk_cmd)
    $rusk_cmd list 2>/dev/null | grep -E '[•✔]' | grep -oE '^\s+[•✔]\s+[0-9]+\s+' | grep -oE '[0-9]+' | sort -n
end

# Get entered task IDs from command line
function __rusk_get_entered_ids
    set -l cmdline (__rusk_get_cmdline)
    set -l entered_ids
    # Skip first two words: "rusk" and command
    for i in (seq 3 (count $cmdline))
        set -l arg $cmdline[$i]
        if test -n "$arg"; and __rusk_is_number "$arg"
            set -a entered_ids "$arg"
        end
    end
    echo $entered_ids
end

# Get task IDs excluding already entered ones
function __rusk_get_task_ids
    set -l all_ids (__rusk_get_all_task_ids)
    set -l entered_ids (__rusk_get_entered_ids)
    
    if test (count $entered_ids) -gt 0
        for id in $all_ids
            if not contains -- $id $entered_ids
                echo $id
            end
        end
    else
        for id in $all_ids
            echo $id
        end
    end
end

# Get task text by ID (supports multi-line tasks via rusk list --for-completion)
function __rusk_get_task_text
    set -l task_id $argv[1]
    set -l rusk_cmd (__rusk_cmd)
    set -l output ($rusk_cmd list --for-completion 2>/dev/null)
    set -l text ""
    set -l collecting false
    for line in $output
        if string match -qr "^\d+\t" -- "$line"
            set -l id (echo $line | cut -f1)
            if test "$id" = "$task_id"
                set text (echo $line | cut -f2-)
                set collecting true
            else
                set collecting false
            end
        else if test "$collecting" = true
            set text "$text\n$line"
        end
    end
    if test -n "$text"
        printf '%s\n' "$text"
    end
end

# True (status 0) if text contains shell metacharacters that require quoting.
# Space alone does not count; see __rusk_quote_text.
function __rusk_has_shell_metachar -a text
    string match -qr '[|;\&><\(\)\[\]\{\}\$"\'`\\\*\?\~\#\@\!\%\^\=\+\-\/\:\,]' -- "$text"
end

# Check if text contains single quote
function __rusk_contains_single_quote
    set -l text $argv[1]
    if string match -q "*'*" -- "$text"
        return 0
    end
    return 1
end

# Quote text only for shell metacharacters (not spaces alone). Otherwise raw text.
# Use single quotes if no single quote in text, otherwise use double quotes with escaping
function __rusk_quote_text
    set -l text $argv[1]
    set -l will_quote 0
    if __rusk_has_shell_metachar "$text"
        set will_quote 1
    end
    if test $will_quote -eq 0
        printf '%s\n' "$text"
        return
    end
    
    # If no single quote in text, use single quotes (no escaping needed)
    if not __rusk_contains_single_quote "$text"
        printf "'%s'\n" "$text"
    else
        # Escape double quotes
        set text (string replace -a '"' '\\"' -- "$text")
        # Escape backticks to prevent command substitution
        set text (string replace -a '`' '\\`' -- "$text")
        # Escape dollar signs to prevent variable expansion
        set text (string replace -a '$' '\\$' -- "$text")
        # Escape backslashes
        set text (string replace -a '\\' '\\\\' -- "$text")
        # Wrap in double quotes
        printf '"%s"\n' "$text"
    end
end

# ============================================================================
# Date Functions
# ============================================================================

function __rusk_get_today_date
    date +%d-%m-%Y 2>/dev/null
end

function __rusk_get_tomorrow_date
    date -d '+1 day' +%d-%m-%Y 2>/dev/null; or date -v+1d +%d-%m-%Y 2>/dev/null; or date +%d-%m-%Y
end

function __rusk_get_week_ahead_date
    date -d '+1 week' +%d-%m-%Y 2>/dev/null; or date -v+1w +%d-%m-%Y 2>/dev/null; or date +%d-%m-%Y
end

function __rusk_get_two_weeks_ahead_date
    date -d '+2 weeks' +%d-%m-%Y 2>/dev/null; or date -v+2w +%d-%m-%Y 2>/dev/null; or date +%d-%m-%Y
end

# ============================================================================
# Command Detection Functions
# ============================================================================

# Check if we're in a specific command (with aliases)
function __rusk_is_command
    set -l cmd $argv[1]
    set -l aliases $argv[2..-1]
    __fish_seen_subcommand_from $cmd $aliases
end

# Check if we're in completions/c command
function __rusk_is_completions_command
    set -l cmdline (__rusk_get_cmdline)
    if test (count $cmdline) -ge 2
        set -l cmd $cmdline[2]
        test "$cmd" = "completions"; or test "$cmd" = "c"
    else
        return 1
    end
end

# Check if install/show is already in command line
function __rusk_has_install_or_show
    set -l cmdline (__rusk_get_cmdline)
    for i in (seq 2 (count $cmdline))
        set -l arg $cmdline[$i]
        if test "$arg" = "install" -o "$arg" = "show"
            return 0
        end
    end
    return 1
end

# ============================================================================
# Flag Completion Functions
# ============================================================================

# Complete flags with narrowing support
function __rusk_complete_flags
    set -l flags $argv
    set -l current_word (__rusk_get_current_word)
    
    if __rusk_is_flag "$current_word"
        # Filter flags that match current word
        for flag in $flags
            if string match -qr "^$current_word" -- "$flag"
                echo $flag
            end
        end
    else
        # Show all flags
        for flag in $flags
            echo $flag
        end
    end
end

# Check if previous word is a date flag
function __rusk_is_after_date_flag
    set -l cmdline (__rusk_get_cmdline)
    if test (count $cmdline) -ge 2
        set -l prev_word $cmdline[-1]
        test "$prev_word" = "-d"; or test "$prev_word" = "--date"
    else
        return 1
    end
end

# Check if we should complete date value
function __rusk_should_complete_date
    set -l commands $argv
    __rusk_is_command $commands[1] $commands[2..-1]; or return 1
    __rusk_is_after_date_flag
end

# Check if we should complete flags (generic)
function __rusk_should_complete_flags
    set -l commands $argv[1..-2]
    set -l flags $argv[-1]
    __rusk_is_command $commands[1] $commands[2..-1]; or return 1
    __rusk_is_after_date_flag; and return 1
    set -l current_word (__rusk_get_current_word)
    __rusk_is_flag "$current_word"; or test -z "$current_word"
end

# Complete flags for add command
function __rusk_complete_add_flags
    set -l all_flags -d --date -h --help -V --version
    __rusk_complete_flags $all_flags
end

# Check if we should complete flags for add command
function __rusk_should_complete_add_flags
    __rusk_is_command add a; or return 1
    __rusk_is_after_date_flag; and return 1
    
    # Don't complete flags if we're completing task text (after flags)
    set -l cmdline (__rusk_get_cmdline)
    if test (count $cmdline) -ge 3
        set -l prev_word $cmdline[-1]
        if not __rusk_is_flag "$prev_word"
            # Check if it's not a date format
            if not string match -qr '^\d{2}-\d{2}-\d{4}$' -- "$prev_word"
                return 1
            end
        end
    end
    
    set -l current_word (__rusk_get_current_word)
    __rusk_is_flag "$current_word"; or test -z "$current_word"
end

# Check if there are ID arguments after edit command
function __rusk_has_edit_id
    set -l cmdline (__rusk_get_cmdline)
    if test (count $cmdline) -ge 3
        set -l args $cmdline[3..-1]
        for arg in $args
            if not __rusk_is_flag "$arg"
                if __rusk_is_number "$arg"
                    return 0
                end
            end
        end
    end
    return 1
end

# Complete flags for edit command
function __rusk_complete_edit_flags
    set -l current_word (__rusk_get_current_word)

    # After task ID completion point, we always suggest only flags.
    set -l all_flags -d --date -h --help
    __rusk_complete_flags $all_flags
end

# Check if we should complete flags for edit command
function __rusk_should_complete_edit_flags
    __rusk_is_command edit e; or return 1
    __rusk_is_after_date_flag; and return 1
    set -l current_word (__rusk_get_current_word)
    __rusk_is_flag "$current_word"; or test -z "$current_word"
end

# ============================================================================
# Edit Command Text Completion
# ============================================================================

# Complete task text after ID
function __rusk_complete_edit_text
    set -l cmdline (__rusk_get_cmdline_with_current)
    if test (count $cmdline) -lt 3
        return
    end
    
    # Find rusk command index (skip environment variables)
    set -l rusk_idx -1
    for i in (seq 1 (count $cmdline))
        if test "$cmdline[$i]" = "rusk"
            set rusk_idx $i
            break
        end
    end
    
    if test $rusk_idx -lt 1
        return
    end
    
    # Get command after rusk (should be "edit" or "e")
    set -l cmd_idx (math $rusk_idx + 1)
    if test $cmd_idx -gt (count $cmdline)
        return
    end
    
    set -l cmd $cmdline[$cmd_idx]
    if test "$cmd" != "edit" -a "$cmd" != "e"
        return
    end
    
    # Get arguments after edit/e command
    set -l args_start_idx (math $cmd_idx + 1)
    if test $args_start_idx -gt (count $cmdline)
        return
    end
    
    set -l args $cmdline[$args_start_idx..-1]
    if test (count $args) -lt 1
        return
    end
    
    # Count how many IDs have been entered
    set -l id_count 0
    for arg in $args
        if __rusk_is_number "$arg"
            set id_count (math $id_count + 1)
        end
    end
    
    # Only suggest task text if there's exactly one ID
    if test $id_count -eq 1
        set -l last_arg $args[-1]
        if __rusk_is_number "$last_arg"
            set -l task_text (__rusk_get_task_text $last_arg)
            if test -n "$task_text"
                # When completing without a space after ID (e.g. "rusk edit 22<tab>"),
                # fish filters candidates by current token ("22").
                # Emit "<id> <text>" so candidate matches and expands in place.
                set -l current_word (__rusk_get_current_word)
                if test "$current_word" = "$last_arg"
                    printf '%s %s\n' "$last_arg" (string join \n $task_text)
                else
                    # Output raw text; wrapper will add proper quoting after insert
                    printf '%s\n' (string join \n $task_text)
                end
            end
        end
    end
end

# Check if we should complete task text after ID
function __rusk_should_complete_edit_text
    __rusk_is_command edit e; or return 1
    set -l cmdline (__rusk_get_cmdline_with_current)
    test (count $cmdline) -ge 3; or return 1
    
    set -l args $cmdline[3..-1]
    test (count $args) -ge 1; or return 1
    
    set -l last_arg $args[-1]
    __rusk_is_number "$last_arg"; or return 1
    
    set -l current_word (__rusk_get_current_word)
    # Only when ID is immediately followed by <tab> (no space).
    test "$current_word" = "$last_arg"
end

# Check if we should complete edit ID
function __rusk_should_complete_edit_id
    __rusk_is_command edit e; or return 1
    # IDs should never be suggested; task text completion is handled separately.
    return 1
end

# ============================================================================
# Mark/Del Command Functions
# ============================================================================

# Check if we should complete task IDs for mark/del commands
function __rusk_should_complete_mark_del_id
    __rusk_is_command mark m del d; or return 1
    # IDs should never be suggested; mark/del keep only flag completion.
    return 1
end

# ============================================================================
# Completions Command Functions
# ============================================================================

# Get available shells, excluding already selected ones
function __rusk_get_available_shells
    set -l cmdline (__rusk_get_cmdline)
    set -l all_shells bash zsh fish nu powershell
    set -l selected_shells
    
    # Find install or show in command line
    set -l install_show_index 0
    for i in (seq 2 (count $cmdline))
        set -l arg $cmdline[$i]
        if test "$arg" = "install" -o "$arg" = "show"
            set install_show_index $i
            break
        end
    end
    
    # If we found install/show, collect all shell arguments after it
    if test $install_show_index -gt 0
        for i in (seq (math $install_show_index + 1) (count $cmdline))
            set -l arg $cmdline[$i]
            if contains -- $arg $all_shells
                set -a selected_shells $arg
            end
        end
    end
    
    # Return shells that are not selected
    for shell in $all_shells
        if not contains -- $shell $selected_shells
            echo $shell
        end
    end
end

# Check if we should complete shells (after install/show)
function __rusk_should_complete_shells
    __rusk_is_completions_command; or return 1
    __rusk_has_install_or_show; or return 1
    
    # Don't suggest flags after shell is selected
    set -l current_word (__rusk_get_current_word)
    if __rusk_is_flag "$current_word"
        return 1
    end
    return 0
end

# ============================================================================
# Main Command Completions
# ============================================================================

# Root commands and aliases
complete -c rusk -f -n '__fish_use_subcommand' -a 'add' -d 'Add a new task'
complete -c rusk -f -n '__fish_use_subcommand' -a 'edit' -d 'Edit tasks by id(s)'
complete -c rusk -f -n '__fish_use_subcommand' -a 'mark' -d 'Mark tasks as done/undone'
complete -c rusk -f -n '__fish_use_subcommand' -a 'del' -d 'Delete tasks by id(s)'
complete -c rusk -f -n '__fish_use_subcommand' -a 'list' -d 'List all tasks'
complete -c rusk -f -n '__fish_use_subcommand' -a 'restore' -d 'Restore from backup'
complete -c rusk -f -n '__fish_use_subcommand' -a 'completions' -d 'Install shell completions'

# Aliases
complete -c rusk -f -n '__fish_use_subcommand' -a 'a' -d 'Alias for add'
complete -c rusk -f -n '__fish_use_subcommand' -a 'e' -d 'Alias for edit'
complete -c rusk -f -n '__fish_use_subcommand' -a 'm' -d 'Alias for mark'
complete -c rusk -f -n '__fish_use_subcommand' -a 'd' -d 'Alias for del'
complete -c rusk -f -n '__fish_use_subcommand' -a 'l' -d 'Alias for list'
complete -c rusk -f -n '__fish_use_subcommand' -a 'r' -d 'Alias for restore'
complete -c rusk -f -n '__fish_use_subcommand' -a 'c' -d 'Alias for completions'

# Global flags
complete -c rusk -f -n '__fish_use_subcommand' -s h -l help -d 'Show help'
complete -c rusk -f -n '__fish_use_subcommand' -s V -l version -d 'Show version'

# ============================================================================
# Add Command Completions
# ============================================================================

# Date completions
complete -c rusk -f -n '__rusk_should_complete_date add a' -a '(__rusk_get_today_date)' -d 'Today'
complete -c rusk -f -n '__rusk_should_complete_date add a' -a '(__rusk_get_tomorrow_date)' -d 'Tomorrow'
complete -c rusk -f -n '__rusk_should_complete_date add a' -a '(__rusk_get_week_ahead_date)' -d 'One week ahead'
complete -c rusk -f -n '__rusk_should_complete_date add a' -a '(__rusk_get_two_weeks_ahead_date)' -d 'Two weeks ahead'

# Flag completions
complete -c rusk -f -n '__rusk_should_complete_add_flags' -a '(__rusk_complete_add_flags)'

# ============================================================================
# Edit Command Completions
# ============================================================================

# Date completions
complete -c rusk -f -n '__rusk_should_complete_date edit e' -a '(__rusk_get_today_date)' -d 'Today'
complete -c rusk -f -n '__rusk_should_complete_date edit e' -a '(__rusk_get_tomorrow_date)' -d 'Tomorrow'
complete -c rusk -f -n '__rusk_should_complete_date edit e' -a '(__rusk_get_week_ahead_date)' -d 'One week ahead'
complete -c rusk -f -n '__rusk_should_complete_date edit e' -a '(__rusk_get_two_weeks_ahead_date)' -d 'Two weeks ahead'

# Flag completions
complete -c rusk -f -n '__rusk_should_complete_edit_flags' -a '(__rusk_complete_edit_flags)'

# Task text completion (before ID completion for priority)
# Task text after ID: wrapper unescapes Fish backslashes; quotes only if __rusk_has_shell_metachar (not for spaces alone).
complete -c rusk -f \
    -n '__rusk_should_complete_edit_text' \
    -a '(__rusk_complete_edit_text)' \
    -d 'Task text'

# Task ID completion
complete -c rusk -f -n '__rusk_should_complete_edit_id' -a '(__rusk_get_task_ids)' -d 'Task ID'

# ============================================================================
# Mark/Del Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_should_complete_mark_del_id' -a '(__rusk_get_task_ids)' -d 'Task ID'
complete -c rusk -f -n '__rusk_should_complete_mark_del_id' -l done -d 'Delete all completed tasks'

# ============================================================================
# List/Restore Command Completions
# ============================================================================

complete -c rusk -f -n '__fish_seen_subcommand_from list l restore r'

# ============================================================================
# Completions Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_is_completions_command; and not __rusk_has_install_or_show' -a 'install' -d 'Install completions for a shell'
complete -c rusk -f -n '__rusk_is_completions_command; and not __rusk_has_install_or_show' -a 'show' -d 'Show completion script'
complete -c rusk -f -n '__rusk_should_complete_shells' -a '(__rusk_get_available_shells)'
