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

# Custom Tab handler: normalize fish escapes after completion, then quote task text
# with single quotes when it contains shell metacharacters (spaces alone do not trigger quoting).
function __rusk_complete_and_unescape
    set -l cmd_before (commandline)
    commandline -f complete
    set -l cmd_after (commandline)

    if test "$cmd_before" = "$cmd_after"
        return
    end

    # This also matches commands prefixed by env vars (e.g. RUSK_DB=... rusk edit 7)
    if not string match -qr 'rusk\s+(e|edit)\s+\d+' -- "$cmd_after"
        return
    end

    # Best-effort normalization: unescape fish escapes and always collapse `\ ` to plain space.
    set -l normalized (string unescape -- "$cmd_after")
    if test $status -ne 0 -o -z "$normalized"
        set normalized "$cmd_after"
    end
    set normalized (string replace -a '\ ' ' ' -- "$normalized")

    # Split prefix (through task id) and trailing text; quote trailing part if needed (not for flags).
    set -l match_result (string match -r '(.*\brusk\s+(e|edit)\s+\d+)\s+(.*)' -- "$normalized")
    if test (count $match_result) -ge 4
        set -l prefix "$match_result[2] "
        set -l text_after_id $match_result[4]
        set text_after_id (string replace -a '\ ' ' ' -- "$text_after_id")

        if not string match -qr '^-' -- "$text_after_id"
            set text_after_id (string replace -a '\\"' '"' -- "$text_after_id")
            set text_after_id (string replace -a "\\'" "'" -- "$text_after_id")
            if string match -qr '^".*"$' -- "$text_after_id"
                set text_after_id (string replace -r '^"(.*)"$' '$1' -- "$text_after_id")
            else
                set -l fc (string sub -s 1 -l 1 -- "$text_after_id")
                set -l lc (string sub -s -1 -l 1 -- "$text_after_id")
                if test "$fc" = "'" -a "$lc" = "'"
                    set text_after_id (string sub -s 2 -e -2 -- "$text_after_id")
                end
            end
            set text_after_id (string trim -- "$text_after_id")
            set -l quoted (__rusk_quote_text "$text_after_id" | string collect | string trim)
            set normalized "$prefix$quoted"
        end
    end

    commandline -r -- "$normalized"
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

function __rusk_contains_single_quote
    set -l text $argv[1]
    if string match -q "*'*" -- "$text"
        return 0
    end
    return 1
end

# Quote text only for shell metacharacters (not spaces alone). Otherwise raw text.
function __rusk_quote_text
    set -l text $argv[1]
    if not __rusk_has_shell_metachar "$text"
        printf '%s\n' "$text"
        return
    end
    if not __rusk_contains_single_quote "$text"
        printf "'%s'\n" "$text"
    else
        set text (string replace -a '"' '\\"' -- "$text")
        set text (string replace -a '`' '\\`' -- "$text")
        set text (string replace -a '$' '\\$' -- "$text")
        set text (string replace -a '\\' '\\\\' -- "$text")
        printf '"%s"\n' "$text"
    end
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

# True if add has at least one completed task-text token (skips value after -d/--date)
function __rusk_add_has_prior_task_text
    set -l cmdline (__rusk_get_cmdline)
    set -l n (count $cmdline)
    test $n -ge 3; or return 1
    set -l rusk_idx -1
    for i in (seq 1 $n)
        if test "$cmdline[$i]" = rusk
            set rusk_idx $i
            break
        end
    end
    test $rusk_idx -ge 1; or return 1
    set -l cmd_idx (math $rusk_idx + 1)
    test $cmd_idx -le $n; or return 1
    if not contains -- "$cmdline[$cmd_idx]" add a
        return 1
    end
    set -l start (math $cmd_idx + 1)
    test $start -le $n; or return 1
    set -l prev ""
    for j in (seq $start $n)
        set -l w "$cmdline[$j]"
        test -n "$w"; or continue
        if test "$prev" = -d; or test "$prev" = --date
            set prev "$w"
            continue
        end
        if test "$w" = -d; or test "$w" = --date
            set prev "$w"
            continue
        end
        if __rusk_is_flag "$w"
            set prev "$w"
            continue
        end
        return 0
    end
    return 1
end

# Complete flags for add command (-d/--date only after task text)
function __rusk_complete_add_flags
    if __rusk_is_after_date_flag
        set -l cw (__rusk_get_current_word)
        if test -z "$cw"
            __rusk_complete_flags -h --help
            return
        end
    end
    set -l all_flags -h --help
    if __rusk_add_has_prior_task_text
        set all_flags -d --date -h --help
    end
    __rusk_complete_flags $all_flags
end

# Check if we should complete flags for add command
function __rusk_should_complete_add_flags
    __rusk_is_command add a; or return 1
    if __rusk_is_after_date_flag
        set -l current_word (__rusk_get_current_word)
        if __rusk_is_flag "$current_word"
            return 1
        end
        if test -z "$current_word"
            return 0
        end
        return 1
    end
    set -l current_word (__rusk_get_current_word)
    if __rusk_is_flag "$current_word"
        return 0
    end
    if test -z "$current_word"
        return 0
    end
    # Subcommand token still under cursor (commandline -opc often only has "rusk" here)
    if contains -- "$current_word" add a
        if test (count (__rusk_get_cmdline)) -eq 1
            return 0
        end
    end
    return 1
end

# True if edit has a task id token in completed words
function __rusk_edit_has_task_id
    set -l cmdline (__rusk_get_cmdline)
    set -l n (count $cmdline)
    test $n -ge 3; or return 1
    set -l rusk_idx -1
    for i in (seq 1 $n)
        if test "$cmdline[$i]" = rusk
            set rusk_idx $i
            break
        end
    end
    test $rusk_idx -ge 1; or return 1
    set -l cmd_idx (math $rusk_idx + 1)
    test $cmd_idx -le $n; or return 1
    if not contains -- "$cmdline[$cmd_idx]" edit e
        return 1
    end
    set -l start (math $cmd_idx + 1)
    test $start -le $n; or return 1
    set -l prev ""
    for j in (seq $start $n)
        set -l w "$cmdline[$j]"
        test -n "$w"; or continue
        if test "$prev" = -d; or test "$prev" = --date
            set prev "$w"
            continue
        end
        if test "$w" = -d; or test "$w" = --date
            set prev "$w"
            continue
        end
        if __rusk_is_flag "$w"
            set prev "$w"
            continue
        end
        if string match -qr '^[0-9,]+$' -- "$w"
            return 0
        end
        set prev "$w"
    end
    return 1
end

# Complete flags for edit: -d/--date after a task id (unless -d already present)
function __rusk_complete_edit_flags
    if __rusk_is_after_date_flag
        set -l cw (__rusk_get_current_word)
        if test -z "$cw"
            __rusk_complete_flags -h --help
            return
        end
    end
    set -l all_flags -h --help
    if __rusk_edit_has_task_id
        set -l cmdline (__rusk_get_cmdline)
        set -l n (count $cmdline)
        set -l has_d 0
        set -l rusk_i -1
        for i in (seq 1 $n)
            if test "$cmdline[$i]" = rusk
                set rusk_i $i
                break
            end
        end
        if test $rusk_i -ge 1
            set -l p ""
            for j in (seq (math $rusk_i + 2) (math $n - 1))
                set -l a "$cmdline[$j]"
                test -n "$a"; or continue
                if test "$p" = -d; or test "$p" = --date
                    set p "$a"
                    continue
                end
                if test "$a" = -d; or test "$a" = --date
                    set has_d 1
                end
                set p "$a"
            end
        end
        if test $has_d -eq 0
            set all_flags -d --date -h --help
        end
    end
    __rusk_complete_flags $all_flags
end

# Check if we should complete flags for edit command
function __rusk_should_complete_edit_flags
    __rusk_is_command edit e; or return 1
    if __rusk_is_after_date_flag
        set -l current_word (__rusk_get_current_word)
        if __rusk_is_flag "$current_word"
            return 1
        end
        if test -z "$current_word"
            return 0
        end
        return 1
    end
    set -l current_word (__rusk_get_current_word)
    if __rusk_is_flag "$current_word"
        return 0
    end
    if test -z "$current_word"
        return 0
    end
    if contains -- "$current_word" edit e
        if test (count (__rusk_get_cmdline)) -eq 1
            return 0
        end
    end
    return 1
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
                    printf '%s %s\n' "$last_arg" (string join -- \n $task_text)
                else
                    # Output raw text; wrapper will add proper quoting after insert
                    printf '%s\n' (string join -- \n $task_text)
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

# Flags for mark/del (no task ID completion)
function __rusk_should_complete_mark_del_flags
    __rusk_is_command mark m del d; or return 1
    set -l cw (__rusk_get_current_word)
    if __rusk_is_flag "$cw"
        return 0
    end
    if test -z "$cw"
        return 0
    end
    if contains -- "$cw" mark m del d
        if test (count (__rusk_get_cmdline)) -eq 1
            return 0
        end
    end
    return 1
end

function __rusk_complete_mark_del_flags
    set -l cmdline (__rusk_get_cmdline)
    test (count $cmdline) -ge 2; or return
    set -l sub "$cmdline[2]"
    if contains -- $sub del d
        __rusk_complete_flags --done -h --help
    else
        __rusk_complete_flags -p --priority -h --help
    end
end

# list / restore: help flags only
function __rusk_should_complete_list_restore_flags
    __rusk_is_command list l restore r; or return 1
    set -l cw (__rusk_get_current_word)
    if __rusk_is_flag "$cw"
        return 0
    end
    if test -z "$cw"
        return 0
    end
    if contains -- "$cw" list l restore r
        if test (count (__rusk_get_cmdline)) -eq 1
            return 0
        end
    end
    return 1
end

function __rusk_complete_list_restore_flags
    set -l cmdline (__rusk_get_cmdline)
    test (count $cmdline) -ge 2; or return
    set -l sub "$cmdline[2]"
    if contains -- $sub list l
        __rusk_complete_flags -f --first-line -h --help
    else
        __rusk_complete_flags -h --help
    end
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

# -h/--help while typing a flag token under `rusk completions ...`
function __rusk_should_complete_completions_help
    __rusk_is_completions_command; or return 1
    set -l cw (__rusk_get_current_word)
    __rusk_is_flag "$cw"; or return 1
    return 0
end

# Empty current word: offer -h/--help next to install/show or shell names
function __rusk_should_complete_completions_help_empty
    __rusk_is_completions_command; or return 1
    set -l cw (__rusk_get_current_word)
    test -z "$cw"; or return 1
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

# Global flags (-s/-l only appear after "-" on the token; -a lists them with subcommands on bare <tab>)
complete -c rusk -f -n '__fish_use_subcommand' -a '-h' -d 'Show help'
complete -c rusk -f -n '__fish_use_subcommand' -a '--help' -d 'Show help'
complete -c rusk -f -n '__fish_use_subcommand' -a '-V' -d 'Show version'
complete -c rusk -f -n '__fish_use_subcommand' -a '--version' -d 'Show version'
complete -c rusk -f -n '__fish_use_subcommand' -s h -l help -d 'Show help'
complete -c rusk -f -n '__fish_use_subcommand' -s V -l version -d 'Show version'

# ============================================================================
# Add Command Completions
# ============================================================================

# Flag completions
complete -c rusk -f -n '__rusk_should_complete_add_flags' -a '(__rusk_complete_add_flags)'

# ============================================================================
# Edit Command Completions
# ============================================================================

# Flag completions
complete -c rusk -f -n '__rusk_should_complete_edit_flags' -a '(__rusk_complete_edit_flags)'

# Task text completion (before ID completion for priority)
# Task text after ID: wrapper normalizes escapes and wraps metacharacters in quotes (single if possible).
complete -c rusk -f \
    -n '__rusk_should_complete_edit_text' \
    -a '(__rusk_complete_edit_text)' \
    -d 'Task text'

# Task ID completion
complete -c rusk -f -n '__rusk_should_complete_edit_id' -a '(__rusk_get_task_ids)' -d 'Task ID'

# ============================================================================
# Mark/Del Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_should_complete_mark_del_flags' -a '(__rusk_complete_mark_del_flags)'

# ============================================================================
# List/Restore Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_should_complete_list_restore_flags' -a '(__rusk_complete_list_restore_flags)'

# ============================================================================
# Completions Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_is_completions_command; and not __rusk_has_install_or_show' -a 'install' -d 'Install completions for a shell'
complete -c rusk -f -n '__rusk_is_completions_command; and not __rusk_has_install_or_show' -a 'show' -d 'Show completion script'
complete -c rusk -f -n '__rusk_should_complete_completions_help_empty' -a '(__rusk_complete_flags -h --help)'
complete -c rusk -f -n '__rusk_should_complete_completions_help' -a '(__rusk_complete_flags -h --help)'
complete -c rusk -f -n '__rusk_should_complete_shells' -a '(__rusk_get_available_shells)'
