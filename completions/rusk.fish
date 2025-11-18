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

# Find rusk binary
function __rusk_cmd
    command -v rusk 2>/dev/null; or echo rusk
end

function __rusk_get_task_ids
    set -l rusk_cmd (__rusk_cmd)
    set -l all_ids ($rusk_cmd list 2>/dev/null | grep -E '[•✔]' | grep -oE '^\s+[•✔]\s+[0-9]+\s+' | grep -oE '[0-9]+' | sort -n)
    # Exclude already entered IDs from suggestions
    set -l cmdline (commandline -opc)
    set -l entered_ids
    # Skip first two words: "rusk" and command
    for i in (seq 3 (count $cmdline))
        if test -n "$cmdline[$i]"; and string match -qr '^[0-9]+$' -- "$cmdline[$i]"
            set -a entered_ids "$cmdline[$i]"
        end
    end
    # Filter out entered IDs
    if test (count $entered_ids) -gt 0
        for id in $all_ids
            set -l found 0
            for entered in $entered_ids
                if test "$id" = "$entered"
                    set found 1
                    break
                end
            end
            if test $found -eq 0
                echo $id
            end
        end
    else
        for id in $all_ids
            echo $id
        end
    end
end

function __rusk_get_task_text
    set -l task_id $argv[1]
    set -l rusk_cmd (__rusk_cmd)
    $rusk_cmd list 2>/dev/null | grep -E "^\s*[•✔]\s+$task_id\s+" | head -1 | sed -E 's/^[[:space:]]*[•✔][[:space:]]+[0-9]+[[:space:]]+[0-9-]*[[:space:]]*//'
end

# Main commands
complete -c rusk -f -n '__fish_use_subcommand' -a 'add' -d 'Add a new task'
complete -c rusk -f -n '__fish_use_subcommand' -a 'edit' -d 'Edit tasks by id(s)'
complete -c rusk -f -n '__fish_use_subcommand' -a 'mark' -d 'Mark tasks as done/undone'
complete -c rusk -f -n '__fish_use_subcommand' -a 'del' -d 'Delete tasks by id(s)'
complete -c rusk -f -n '__fish_use_subcommand' -a 'list' -d 'List all tasks'
complete -c rusk -f -n '__fish_use_subcommand' -a 'restore' -d 'Restore from backup'
complete -c rusk -f -n '__fish_use_subcommand' -a 'completions' -d 'Install shell completions'
complete -c rusk -f -n '__fish_use_subcommand' -a 'a' -d 'Alias for add'
complete -c rusk -f -n '__fish_use_subcommand' -a 'e' -d 'Alias for edit'
complete -c rusk -f -n '__fish_use_subcommand' -a 'm' -d 'Alias for mark'
complete -c rusk -f -n '__fish_use_subcommand' -a 'd' -d 'Alias for del'
complete -c rusk -f -n '__fish_use_subcommand' -a 'l' -d 'Alias for list'
complete -c rusk -f -n '__fish_use_subcommand' -a 'r' -d 'Alias for restore'
# Global flags
complete -c rusk -f -n '__fish_use_subcommand' -s h -l help -d 'Show help'
complete -c rusk -f -n '__fish_use_subcommand' -s V -l version -d 'Show version'

# Functions to get individual date options
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

# Function to check if we're completing date value
function __rusk_should_complete_date
    __fish_seen_subcommand_from add a; or return 1
    set -l cmdline (commandline -opc)
    # Check if previous word is -d or --date
    if test (count $cmdline) -ge 2
        set -l prev_word $cmdline[-1]
        test "$prev_word" = "-d"; or test "$prev_word" = "--date"
    else
        return 1
    end
end

# Function to complete flags for add command with narrowing
function __rusk_complete_add_flags
    set -l cmdline (commandline -opc)
    set -l current_word (commandline -ct)
    
    # All available flags
    set -l all_flags -d --date -h --help -V --version
    
    # If current word starts with -, filter flags that match
    if string match -qr '^-' -- "$current_word"
        for flag in $all_flags
            if string match -qr "^$current_word" -- "$flag"
                echo $flag
            end
        end
    else
        # If current word doesn't start with -, show all flags
        for flag in $all_flags
            echo $flag
        end
    end
end

# Function to check if we're completing flags for add command
function __rusk_should_complete_add_flags
    __fish_seen_subcommand_from add a; or return 1
    set -l cmdline (commandline -opc)
    # Don't complete flags if we're completing date value
    if test (count $cmdline) -ge 2
        set -l prev_word $cmdline[-1]
        if test "$prev_word" = "-d"; or test "$prev_word" = "--date"
            return 1
        end
    end
    # Don't complete flags if we're completing task text (after flags)
    # Check if there's already text entered
    if test (count $cmdline) -ge 3
        # If previous word is not a flag and not a date, we're probably entering text
        set -l prev_word $cmdline[-1]
        if not string match -qr '^-' -- "$prev_word"
            # Check if it's not a date format
            if not string match -qr '^\d{2}-\d{2}-\d{4}$' -- "$prev_word"
                return 1
            end
        end
    end
    # Complete flags if current word starts with - or is empty
    set -l current_word (commandline -ct)
    string match -qr '^-' -- "$current_word"; or test -z "$current_word"
end

# Add command flags
# Complete date flag with individual date options
complete -c rusk -f -n '__rusk_should_complete_date' -a '(__rusk_get_today_date)' -d 'Today'
complete -c rusk -f -n '__rusk_should_complete_date' -a '(__rusk_get_tomorrow_date)' -d 'Tomorrow'
complete -c rusk -f -n '__rusk_should_complete_date' -a '(__rusk_get_week_ahead_date)' -d 'One week ahead'
complete -c rusk -f -n '__rusk_should_complete_date' -a '(__rusk_get_two_weeks_ahead_date)' -d 'Two weeks ahead'

# Complete flags with narrowing support
complete -c rusk -f -n '__rusk_should_complete_add_flags' -a '(__rusk_complete_add_flags)'

# Function to check if we're completing date value for edit command
function __rusk_should_complete_edit_date
    __fish_seen_subcommand_from edit e; or return 1
    set -l cmdline (commandline -opc)
    # Check if previous word is -d or --date
    if test (count $cmdline) -ge 2
        set -l prev_word $cmdline[-1]
        test "$prev_word" = "-d"; or test "$prev_word" = "--date"
    else
        return 1
    end
end

# Function to complete flags for edit command with narrowing
function __rusk_complete_edit_flags
    set -l cmdline (commandline -opc)
    set -l current_word (commandline -ct)
    
    # All available flags
    set -l all_flags -d --date -h --help
    
    # If current word starts with -, filter flags that match
    if string match -qr '^-' -- "$current_word"
        for flag in $all_flags
            if string match -qr "^$current_word" -- "$flag"
                echo $flag
            end
        end
    else
        # If current word doesn't start with -, show all flags
        for flag in $all_flags
            echo $flag
        end
    end
end

# Function to check if we're completing flags for edit command
function __rusk_should_complete_edit_flags
    __fish_seen_subcommand_from edit e; or return 1
    set -l cmdline (commandline -opc)
    # Don't complete flags if we're completing date value
    if test (count $cmdline) -ge 2
        set -l prev_word $cmdline[-1]
        if test "$prev_word" = "-d"; or test "$prev_word" = "--date"
            return 1
        end
    end
    # Complete flags if current word starts with - or is empty
    set -l current_word (commandline -ct)
    string match -qr '^-' -- "$current_word"; or test -z "$current_word"
end

# Edit command - flags first
# Complete date flag with individual date options
complete -c rusk -f -n '__rusk_should_complete_edit_date' -a '(__rusk_get_today_date)' -d 'Today'
complete -c rusk -f -n '__rusk_should_complete_edit_date' -a '(__rusk_get_tomorrow_date)' -d 'Tomorrow'
complete -c rusk -f -n '__rusk_should_complete_edit_date' -a '(__rusk_get_week_ahead_date)' -d 'One week ahead'
complete -c rusk -f -n '__rusk_should_complete_edit_date' -a '(__rusk_get_two_weeks_ahead_date)' -d 'Two weeks ahead'

# Complete flags with narrowing support
complete -c rusk -f -n '__rusk_should_complete_edit_flags' -a '(__rusk_complete_edit_flags)'

# Function to complete task text after ID
function __rusk_complete_edit_text
    set -l cmdline (commandline -opc)
    # We need at least: rusk edit <ID>
    if test (count $cmdline) -ge 3
        # Check if we're after edit/e command
        if test "$cmdline[2]" = "edit" -o "$cmdline[2]" = "e"
            # Get arguments after edit/e command
            set -l args $cmdline[3..-1]
            if test (count $args) -ge 1
                # Count how many IDs have been entered
                set -l id_count 0
                for arg in $args
                    if string match -qr '^[0-9]+$' -- $arg
                        set id_count (math $id_count + 1)
                    end
                end
                # Only suggest task text if there's exactly one ID
                if test $id_count -eq 1
                    # Get the last argument (should be task ID)
                    set -l last_arg $args[-1]
                    # Check if last argument is a number (task ID)
                    if string match -qr '^[0-9]+$' -- $last_arg
                        set -l task_text (__rusk_get_task_text $last_arg)
                        if test -n "$task_text"
                            # Return text in quotes if it contains spaces
                            # This helps fish handle it better
                            if string match -qr ' ' -- "$task_text"
                                printf '"%s"\n' "$task_text"
                            else
                                printf '%s\n' "$task_text"
                            end
                        end
                    end
                end
            end
        end
    end
end

# Completion for task text after ID in edit command
# This should be checked BEFORE ID completion to have higher priority
function __rusk_should_complete_edit_text
    __fish_seen_subcommand_from edit e; or return 1
    set -l cmdline (commandline -opc)
    test (count $cmdline) -ge 3; or return 1
    # Get arguments after edit/e command
    set -l args $cmdline[3..-1]
    test (count $args) -ge 1; or return 1
    set -l last_arg $args[-1]
    # Check if last argument is a number (task ID)
    string match -qr '^[0-9]+$' -- $last_arg; or return 1
    # Check if we're at the position right after the ID
    # Current word should be empty or equal to the ID
    set -l current_word (commandline -ct)
    test -z "$current_word"; or test "$current_word" = "$last_arg"
end

# Complete task text after ID - this rule should come BEFORE ID completion
# Note: Fish requires 2 tabs for single match (safety feature), but this rule
# has higher priority and will trigger first
complete -c rusk -f \
    -n '__rusk_should_complete_edit_text' \
    -a '(__rusk_complete_edit_text)' \
    -d 'Task text'

# Edit command - task IDs
# Complete task IDs when we're in edit/e command and haven't completed text yet
function __rusk_should_complete_edit_id
    __fish_seen_subcommand_from edit e; or return 1
    set -l cmdline (commandline -opc)
    # Don't complete ID if we're after a flag
    set -l last_token $cmdline[-1]
    if string match -qr '^-' -- $last_token
        return 1
    end
    # Don't complete ID if we're completing text (after an ID)
    # Get arguments after edit/e command
    if test (count $cmdline) -ge 3
        set -l args $cmdline[3..-1]
        if test (count $args) -ge 1
            set -l last_arg $args[-1]
            if string match -qr '^[0-9]+$' -- $last_arg
                # We're after an ID, don't complete another ID (let text completion handle it)
                return 1
            end
        end
    end
    return 0
end

complete -c rusk -f -n '__rusk_should_complete_edit_id' -a '(__rusk_get_task_ids)' -d 'Task ID'

# Mark command - task IDs
complete -c rusk -f -n '__fish_seen_subcommand_from mark m' -a '(__rusk_get_task_ids)' -d 'Task ID'

# Del command - task IDs and --done flag
complete -c rusk -f -n '__fish_seen_subcommand_from del d' -a '(__rusk_get_task_ids)' -d 'Task ID'
complete -c rusk -f -n '__fish_seen_subcommand_from del d' -l done -d 'Delete all completed tasks'

# List and restore don't take arguments
complete -c rusk -f -n '__fish_seen_subcommand_from list l restore r'

# Completions subcommand
complete -c rusk -f -n '__fish_seen_subcommand_from completions' -a 'install' -d 'Install completions for a shell'
complete -c rusk -f -n '__fish_seen_subcommand_from completions' -a 'show' -d 'Show completion script'
complete -c rusk -f -n '__fish_seen_subcommand_from completions; and __fish_seen_subcommand_from install show' -a 'bash' -d 'Bash shell'
complete -c rusk -f -n '__fish_seen_subcommand_from completions; and __fish_seen_subcommand_from install show' -a 'zsh' -d 'Zsh shell'
complete -c rusk -f -n '__fish_seen_subcommand_from completions; and __fish_seen_subcommand_from install show' -a 'fish' -d 'Fish shell'
complete -c rusk -f -n '__fish_seen_subcommand_from completions; and __fish_seen_subcommand_from install show' -a 'nu' -d 'Nu shell'
complete -c rusk -f -n '__fish_seen_subcommand_from completions; and __fish_seen_subcommand_from install show' -a 'powershell' -d 'PowerShell'

