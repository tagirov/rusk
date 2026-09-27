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

# The text of task $argv[1] from `rusk list --for-completion-lines`, printed as it
# is (a trailing newline included). Each task is one line, `<id><TAB><text>`,
# with `\\`, `\n`, `\r` and `\t` escaped in the text: `printf %b` gives it back.
function __rusk_get_task_text -a task_id
    set -l rusk_cmd (__rusk_cmd)
    for line in ($rusk_cmd list --for-completion-lines 2>/dev/null)
        set -l fields (string split -m 1 \t -- $line)
        if test "$fields[1]" = "$task_id"
            printf '%b' "$fields[2]"
            return 0
        end
    end
    return 1
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

# Check if previous word is a value-taking flag (-d/--date, -a/--after)
function __rusk_is_after_date_flag
    set -l cmdline (__rusk_get_cmdline)
    if test (count $cmdline) -ge 2
        set -l prev_word $cmdline[-1]
        contains -- "$prev_word" -d --date -a --after
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
        if contains -- "$prev" -d --date -a --after
            set prev "$w"
            continue
        end
        if contains -- "$w" -d --date -a --after
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

# Complete flags for add command (-d/--date, -a/--after only after task text)
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
        set all_flags -d --date -a --after -h --help
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
        if contains -- "$prev" -d --date -a --after
            set prev "$w"
            continue
        end
        if contains -- "$w" -d --date -a --after
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

# Complete flags for edit: -d/--date, -a/--after after a task id (unless already present)
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
        set -l has_a 0
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
                if contains -- "$p" -d --date -a --after
                    set p "$a"
                    continue
                end
                if test "$a" = -d; or test "$a" = --date
                    set has_d 1
                end
                if test "$a" = -a; or test "$a" = --after
                    set has_a 1
                end
                set p "$a"
            end
        end
        set all_flags
        if test $has_d -eq 0
            set -a all_flags -d --date
        end
        if test $has_a -eq 0
            set -a all_flags -a --after
        end
        set -a all_flags -h --help
    end
    __rusk_complete_flags $all_flags
end

# Check if we should complete flags for edit command
function __rusk_should_complete_edit_flags
    __rusk_is_command edit e; or return 1
    # After `--` all is text. Next to the id the text comes first:
    # __rusk_complete_edit_text offers the flags when there is none.
    contains -- -- (__rusk_get_cmdline); and return 1
    set -l slot (__rusk_edit_text_slot)
    and contains -- $slot[1] text dashes typed-dashes
    and return 1
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

# `rusk edit <id><TAB>` puts the text of the task on the command line. fish
# escapes whatever it inserts and inserts one token per Tab, so the text is a
# candidate of its own, as it is, and fish quotes it: `rusk edit 3<TAB>`
# completes the id, the next Tab the text. Nothing rebinds Tab for this.

# Where the cursor is in `rusk edit` next to its one id: prints `id <n>` while
# the id itself is typed, `text <n>` on the token after it, `dashes <n>` on
# the token after `<n> --`, and `typed-dashes <n>` while that `--` is still
# the token under the cursor (fish adds no space after a word ending in `-`);
# fails anywhere else. The values of -d/--date/-a/--after are not ids, and
# after `--` there are none.
function __rusk_edit_text_slot
    __rusk_is_command edit e; or return 1
    set -l words (__rusk_get_cmdline)
    set -l current (__rusk_get_current_word)
    # Skip env assignments (RUSK_DB=... rusk edit 7), `rusk` and the command
    set -l rusk_idx (contains -i -- rusk $words); or return 1
    set -l cmd_idx (math $rusk_idx + 1)
    contains -- "$words[$cmd_idx]" edit e; or return 1
    set -l args $words[(math $cmd_idx + 1)..-1]
    set -l ids
    set -l value_next 0
    set -l dashes_at 0
    set -l n 0
    for w in $args
        set n (math $n + 1)
        if test $dashes_at -gt 0
            continue
        else if test $value_next -eq 1
            set value_next 0
        else if contains -- $w -d --date -a --after
            set value_next 1
        else if test "$w" = --
            set dashes_at $n
        else if __rusk_is_number "$w"
            set -a ids $w
        end
    end
    test $value_next -eq 0; or return 1
    if test $dashes_at -eq 0
        if __rusk_is_number "$current"
            test (count $ids) -eq 0; or return 1
            printf '%s\n' id $current
        else if test (count $ids) -eq 1; and test "$args[-1]" = "$ids[1]"
            if test -z "$current"
                printf '%s\n' text $ids[1]
            else if test "$current" = --
                printf '%s\n' typed-dashes $ids[1]
            else
                return 1
            end
        else
            return 1
        end
    else if test -z "$current"; and test (count $ids) -eq 1; and test $dashes_at -eq $n
        and test "$args[-2]" = "$ids[1]"
        printf '%s\n' dashes $ids[1]
    else
        return 1
    end
end

# The candidates for the slot __rusk_edit_text_slot names, each ended by a NUL
# (a text may hold line breaks): on the id, the id with the start of its text
# to tell it by; next to it the text — or `--` first, when the text starts
# with `-` and would be read as options (fish adds no space after a word
# that ends in `-`: offered `--` once more, it does). Two texts fish cannot
# insert as they are, and so are not offered: one with a tab (fish takes the
# first tab of a candidate for the start of its description) and one that
# starts with `~` (fish leaves that unescaped, to be expanded to a home
# directory). Where there is no text to offer, next to the id, the flags are.
function __rusk_complete_edit_text
    set -l slot (__rusk_edit_text_slot); or return
    set -l text (__rusk_get_task_text $slot[2] | string collect -N)
    set -l offered 1
    if test -z "$text"; or string match -qr '\t|^~' -- $text
        set offered 0
    end
    switch $slot[1]
        case id
            test -n "$text"; or return
            set -l first (string split -m 1 \n -- $text)[1]
            printf '%s\t%s\0' $slot[2] (string replace -ra '[[:cntrl:]]' ' ' -- $first)
        case text typed-dashes
            if test $offered -eq 1; and string match -q -- '-*' $text
                printf '%s\t%s\0' -- 'Task text follows (it starts with -)'
            else if test $offered -eq 1; and test $slot[1] = text
                printf '%s\t%s\0' $text 'Task text'
            else
                set -l flags (__rusk_complete_edit_flags)
                test (count $flags) -gt 0; and printf '%s\0' $flags
            end
        case dashes
            test $offered -eq 1; and printf '%s\t%s\0' $text 'Task text'
    end
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
        # `--done` takes no ids: offered only before any
        if test (count $cmdline) -ge 3; and string match -qr '^[0-9,]+$' -- $cmdline[3..-1]
            __rusk_complete_flags -y --yes -h --help
        else
            __rusk_complete_flags --done -y --yes -h --help
        end
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
        __rusk_complete_flags -c --compact --no-compact -h --help
    else
        __rusk_complete_flags -h --help
    end
end

# search: --id + help flags (query is free text)
function __rusk_should_complete_search_flags
    __rusk_is_command search s; or return 1
    set -l cw (__rusk_get_current_word)
    if __rusk_is_flag "$cw"
        return 0
    end
    if test -z "$cw"
        return 0
    end
    if contains -- "$cw" search s
        if test (count (__rusk_get_cmdline)) -eq 1
            return 0
        end
    end
    return 1
end

function __rusk_complete_search_flags
    __rusk_complete_flags --id -h --help
end

# ============================================================================
# Gen/Serve Command Functions
# ============================================================================

# Check if previous word is the gen output flag (-o/--output takes a file path)
function __rusk_is_after_output_flag
    set -l cmdline (__rusk_get_cmdline)
    if test (count $cmdline) -ge 2
        set -l prev_word $cmdline[-1]
        test "$prev_word" = "-o"; or test "$prev_word" = "--output"
    else
        return 1
    end
end

# Check if previous word is a serve value flag (--host/--port take free-form values)
function __rusk_is_after_serve_value_flag
    set -l cmdline (__rusk_get_cmdline)
    if test (count $cmdline) -ge 2
        set -l prev_word $cmdline[-1]
        test "$prev_word" = "--host"; or test "$prev_word" = "--port"
    else
        return 1
    end
end

# gen: option flags (-o/--output value gets file completion via a separate -F rule)
function __rusk_should_complete_gen_flags
    __rusk_is_command gen g; or return 1
    if __rusk_is_after_output_flag
        return 1
    end
    set -l cw (__rusk_get_current_word)
    if __rusk_is_flag "$cw"
        return 0
    end
    if test -z "$cw"
        return 0
    end
    if contains -- "$cw" gen g
        if test (count (__rusk_get_cmdline)) -eq 1
            return 0
        end
    end
    return 1
end

function __rusk_complete_gen_flags
    __rusk_complete_flags -o --output -h --help
end

# serve: option flags (no completion for --host/--port values)
function __rusk_should_complete_serve_flags
    __rusk_is_command serve; or return 1
    if __rusk_is_after_serve_value_flag
        return 1
    end
    set -l cw (__rusk_get_current_word)
    if __rusk_is_flag "$cw"
        return 0
    end
    if test -z "$cw"
        return 0
    end
    if contains -- "$cw" serve
        if test (count (__rusk_get_cmdline)) -eq 1
            return 0
        end
    end
    return 1
end

function __rusk_complete_serve_flags
    __rusk_complete_flags --host --port -h --help
end

# ============================================================================
# Sync Command Functions
# ============================================================================

# Check if we're in sync command (no alias)
function __rusk_is_sync_command
    set -l cmdline (__rusk_get_cmdline)
    if test (count $cmdline) -ge 2
        test "$cmdline[2]" = "sync"
    else
        return 1
    end
end

# Check if push/pull is already in command line
function __rusk_has_push_or_pull
    set -l cmdline (__rusk_get_cmdline)
    for i in (seq 2 (count $cmdline))
        set -l arg $cmdline[$i]
        if test "$arg" = "push" -o "$arg" = "pull"
            return 0
        end
    end
    return 1
end

# push/pull not entered yet: offer subcommands (plus -h/--help via flag rule)
function __rusk_should_complete_sync_subcommands
    __rusk_is_sync_command; or return 1
    __rusk_has_push_or_pull; and return 1
    set -l cw (__rusk_get_current_word)
    if __rusk_is_flag "$cw"
        return 1
    end
    return 0
end

# Help flags at the `rusk sync` level (empty word or flag token)
function __rusk_should_complete_sync_help
    __rusk_is_sync_command; or return 1
    __rusk_has_push_or_pull; and return 1
    set -l cw (__rusk_get_current_word)
    if __rusk_is_flag "$cw"
        return 0
    end
    test -z "$cw"
end

# --force/-h/--help after push/pull
function __rusk_should_complete_sync_direction_flags
    __rusk_is_sync_command; or return 1
    __rusk_has_push_or_pull; or return 1
    set -l cw (__rusk_get_current_word)
    if __rusk_is_flag "$cw"
        return 0
    end
    test -z "$cw"
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
complete -c rusk -f -n '__fish_use_subcommand' -a 'search' -d 'Search tasks by text'
complete -c rusk -f -n '__fish_use_subcommand' -a 'restore' -d 'Restore from backup'
complete -c rusk -f -n '__fish_use_subcommand' -a 'gen' -d 'Generate a read-only HTML page'
complete -c rusk -f -n '__fish_use_subcommand' -a 'serve' -d 'Serve the web UI'
complete -c rusk -f -n '__fish_use_subcommand' -a 'sync' -d 'Synchronize with a remote'
complete -c rusk -f -n '__fish_use_subcommand' -a 'completions' -d 'Install shell completions'

# Aliases share the command description so the fish pager groups each
# alias with its command on one aligned row (e.g. "a  add    Add a new task").
complete -c rusk -f -n '__fish_use_subcommand' -a 'a' -d 'Add a new task'
complete -c rusk -f -n '__fish_use_subcommand' -a 'e' -d 'Edit tasks by id(s)'
complete -c rusk -f -n '__fish_use_subcommand' -a 'm' -d 'Mark tasks as done/undone'
complete -c rusk -f -n '__fish_use_subcommand' -a 'd' -d 'Delete tasks by id(s)'
complete -c rusk -f -n '__fish_use_subcommand' -a 'l' -d 'List all tasks'
complete -c rusk -f -n '__fish_use_subcommand' -a 's' -d 'Search tasks by text'
complete -c rusk -f -n '__fish_use_subcommand' -a 'r' -d 'Restore from backup'
complete -c rusk -f -n '__fish_use_subcommand' -a 'g' -d 'Generate a read-only HTML page'
complete -c rusk -f -n '__fish_use_subcommand' -a 'c' -d 'Install shell completions'

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

# The id, then the task text (see __rusk_complete_edit_text)
complete -c rusk -f \
    -n '__rusk_edit_text_slot >/dev/null' \
    -a '(__rusk_complete_edit_text | string split0)'

# ============================================================================
# Mark/Del Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_should_complete_mark_del_flags' -a '(__rusk_complete_mark_del_flags)'

# ============================================================================
# List/Restore Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_should_complete_list_restore_flags' -a '(__rusk_complete_list_restore_flags)'

# ============================================================================
# Search Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_should_complete_search_flags' -a '(__rusk_complete_search_flags)'

# ============================================================================
# Gen/Serve Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_should_complete_gen_flags' -a '(__rusk_complete_gen_flags)'
# -o/--output value is a file path: enable file completion (-F)
complete -c rusk -F -n '__rusk_is_command gen g; and __rusk_is_after_output_flag'
complete -c rusk -f -n '__rusk_should_complete_serve_flags' -a '(__rusk_complete_serve_flags)'
# --host/--port values are free-form: suppress the file fallback, offer nothing
complete -c rusk -f -n '__rusk_is_command serve; and __rusk_is_after_serve_value_flag'

# ============================================================================
# Sync Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_should_complete_sync_subcommands' -a 'push' -d 'Upload local tasks to the remote'
complete -c rusk -f -n '__rusk_should_complete_sync_subcommands' -a 'pull' -d 'Replace the local database with the remote tasks'
complete -c rusk -f -n '__rusk_should_complete_sync_help' -a '(__rusk_complete_flags -h --help)'
complete -c rusk -f -n '__rusk_should_complete_sync_direction_flags' -a '(__rusk_complete_flags --force -h --help)'

# ============================================================================
# Completions Command Completions
# ============================================================================

complete -c rusk -f -n '__rusk_is_completions_command; and not __rusk_has_install_or_show' -a 'install' -d 'Install completions for a shell'
complete -c rusk -f -n '__rusk_is_completions_command; and not __rusk_has_install_or_show' -a 'show' -d 'Show completion script'
complete -c rusk -f -n '__rusk_should_complete_completions_help_empty' -a '(__rusk_complete_flags -h --help)'
complete -c rusk -f -n '__rusk_should_complete_completions_help' -a '(__rusk_complete_flags -h --help)'
complete -c rusk -f -n '__rusk_should_complete_shells' -a '(__rusk_get_available_shells)'
