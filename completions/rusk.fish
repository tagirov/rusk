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

# Custom Tab handler that removes backslash escaping for rusk edit command
function __rusk_complete_and_unescape
    # Get command line before completion
    set -l cmd_before (commandline)
    
    # Perform standard completion
    commandline -f complete
    
    # Get command line after completion
    set -l cmd_after (commandline)
    
    # Check if command starts with "rusk e" or "rusk edit" and completion changed
    if test "$cmd_before" != "$cmd_after"
        if string match -qr '^rusk\s+(e|edit)\s+\d+\s+' -- "$cmd_after"
            # Remove backslash escaping using string unescape
            set -l unescaped (string unescape -- "$cmd_after")
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

# Get task text by ID
function __rusk_get_task_text
    set -l task_id $argv[1]
    set -l rusk_cmd (__rusk_cmd)
    $rusk_cmd list 2>/dev/null | grep -E "^\s*[•✔]\s+$task_id\s+" | head -1 | sed -E 's/^[[:space:]]*[•✔][[:space:]]+[0-9]+[[:space:]]+[0-9-]*[[:space:]]*//'
end

# Quote text if it contains special characters requiring escaping (excluding spaces)
# Spaces are handled by __rusk_complete_and_unescape which removes backslash escaping
function __rusk_quote_text
    set -l text $argv[1]
    # Check if text contains special characters that require escaping (excluding spaces)
    # Special characters: | ; & > < ( ) [ ] { } $ " ' ` \ * ? ~ # @ ! % ^ = + - / : , .
    # Note: - is placed at the end of character class to avoid range interpretation
    if string match -qr '[|;&><()\[\]{}$"'"'"'`\\*?~#@!%^=+/:,.-]' -- "$text"
        # Wrap in double quotes for special characters
        # Escape any existing double quotes in the text
        set -l escaped_text (string replace -a '"' '\\"' -- "$text")
        printf '"%s"\n' "$escaped_text"
    else
        # No special characters requiring escaping - return as-is
        # Spaces will be unescaped by __rusk_complete_and_unescape
        printf '%s\n' "$text"
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
    
    if __rusk_has_edit_id
        # Only suggest date flag after ID
        set -l date_flags -d --date
        __rusk_complete_flags $date_flags
    else
        # All available flags when no ID is present
        set -l all_flags -d --date -h --help
        __rusk_complete_flags $all_flags
    end
end

# Check if we should complete flags for edit command
function __rusk_should_complete_edit_flags
    __rusk_is_command edit e; or return 1
    __rusk_is_after_date_flag; and return 1
    # Don't suggest flags if ID is already entered
    __rusk_has_edit_id; and return 1
    set -l current_word (__rusk_get_current_word)
    __rusk_is_flag "$current_word"; or test -z "$current_word"
end

# ============================================================================
# Edit Command Text Completion
# ============================================================================

# Complete task text after ID
function __rusk_complete_edit_text
    set -l cmdline (__rusk_get_cmdline)
    if test (count $cmdline) -lt 3
        return
    end
    
    # Check if we're after edit/e command
    set -l cmd $cmdline[2]
    if test "$cmd" != "edit" -a "$cmd" != "e"
        return
    end
    
    # Get arguments after edit/e command
    set -l args $cmdline[3..-1]
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
                __rusk_quote_text "$task_text"
            end
        end
    end
end

# Check if we should complete task text after ID
function __rusk_should_complete_edit_text
    __rusk_is_command edit e; or return 1
    set -l cmdline (__rusk_get_cmdline)
    test (count $cmdline) -ge 3; or return 1
    
    set -l args $cmdline[3..-1]
    test (count $args) -ge 1; or return 1
    
    set -l last_arg $args[-1]
    __rusk_is_number "$last_arg"; or return 1
    
    set -l current_word (__rusk_get_current_word)
    test -z "$current_word"; or test "$current_word" = "$last_arg"
end

# Check if we should complete edit ID
function __rusk_should_complete_edit_id
    __rusk_is_command edit e; or return 1
    set -l cmdline (__rusk_get_cmdline)
    
    # Don't complete ID if we're after a flag
    set -l last_token $cmdline[-1]
    if __rusk_is_flag "$last_token"
        return 1
    end
    
    # Don't complete ID if there's text after an ID
    if test (count $cmdline) -ge 3
        set -l args $cmdline[3..-1]
        set -l found_id false
        for arg in $args
            if __rusk_is_flag "$arg"
                continue
            end
            if __rusk_is_number "$arg"
                set found_id true
            else if test "$found_id" = "true"
                # Found text after ID - don't suggest more IDs
                return 1
            end
        end
        # If last arg is an ID, don't suggest more IDs (user is typing text next)
        if test (count $args) -ge 1
            set -l last_arg $args[-1]
            if __rusk_is_number "$last_arg"
                return 1
            end
        end
    end
    return 0
end

# ============================================================================
# Mark/Del Command Functions
# ============================================================================

# Check if we should complete task IDs for mark/del commands
function __rusk_should_complete_mark_del_id
    __rusk_is_command mark m del d; or return 1
    set -l cmdline (__rusk_get_cmdline)
    
    # Don't complete ID if we're after a flag
    set -l last_token $cmdline[-1]
    if __rusk_is_flag "$last_token"
        return 1
    end
    
    # Check if there are any ID arguments after the command
    if test (count $cmdline) -ge 3
        set -l args $cmdline[3..-1]
        for arg in $args
            if not __rusk_is_flag "$arg"
                return 1
            end
        end
    end
    return 0
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
# Text is always wrapped in single quotes to prevent fish from escaping spaces with backslashes
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
