#!/bin/bash
# Bash completion script for rusk
#
# Installation:
#   1. Automatic (recommended):
#      rusk completions install bash
#
#   2. Manual:
#      Generate script using rusk command:
#      rusk completions show bash > ~/.bash_completion.d/rusk
#
#      Then add to your ~/.bashrc:
#      source ~/.bash_completion.d/rusk
#
#      System-wide (requires root):
#      rusk completions show bash | sudo tee /etc/bash_completion.d/rusk > /dev/null
#      System-wide completions are loaded automatically on bash startup

# Find rusk binary
_rusk_cmd() {
    command -v rusk 2>/dev/null || echo "rusk"
}

# True if the text would not come back as it is when put on the command line
# bare: a shell-special character, or any whitespace but single spaces between
# words (tabs, line breaks, runs of spaces and spaces at either end).
_rusk_needs_quotes() {
    local text="$1"
    # Special chars: | ; & > < ( ) [ ] { } $ " ' ` \ * ? ~ # @ ! % ^ = + - / : ,
    case "$text" in
        *[\|\;\&\>\<\(\)\[\]\{\}\$\"\'\`\\*\?\~\#\@\!\%\^\=\+\-\/\:\,]*)
            return 0
            ;;
        *[[:cntrl:]]* | *'  '* | ' '* | *' ')
            return 0
            ;;
    esac
    return 1
}

# Quote text if it contains special characters
# Always use single quotes: they are fully inert in bash (no $ ` \ ! expansion),
# and embedded single quotes are emitted via the POSIX '\'' idiom
_rusk_quote_text() {
    local text="$1"
    if ! _rusk_needs_quotes "$text"; then
        echo "$text"
        return
    fi

    # Replace each ' with '\'' (close quote, escaped quote, reopen quote)
    local escaped="${text//\'/\'\\\'\'}"
    echo "'$escaped'"
}

# The text of task $1 from `rusk list --for-completion-lines`, quoted for the command
# line. Each task is one line, `<id><TAB><text>`, with `\\`, `\n`, `\r` and
# `\t` escaped in the text: `printf %b` gives it back.
_rusk_get_task_text() {
    local task_id="$1"
    local rusk_cmd=$(_rusk_cmd)
    local rusk_db=""
    if [[ "$COMP_LINE" =~ RUSK_DB=([^\ ]+) ]]; then
        rusk_db="${BASH_REMATCH[1]}"
    fi
    
    local output
    if [ -n "$rusk_db" ]; then
        output=$( ( export RUSK_DB="$rusk_db"; "$rusk_cmd" list --for-completion-lines 2>/dev/null ) )
    else
        output=$("$rusk_cmd" list --for-completion-lines 2>/dev/null)
    fi
    
    local text="" line
    while IFS= read -r line; do
        if [[ "$line" == "$task_id"$'\t'* ]]; then
            printf -v text '%b' "${line#*$'\t'}"
            break
        fi
    done <<< "$output"
    
    if [ -n "$text" ]; then
        # A text that starts with `-` would be read as options: after `--`
        # rusk takes it as text
        [[ "$text" == -* ]] && printf -- '-- '
        _rusk_quote_text "$text"
    fi
}

# Count how many IDs have been entered
_rusk_count_ids() {
    local count=0
    local i
    # Find rusk command index
    local rusk_idx=-1
    for ((i=0; i<${#COMP_WORDS[@]}; i++)); do
        if [[ "${COMP_WORDS[i]}" == "rusk" ]]; then
            rusk_idx=$i
            break
        fi
    done
    # Start from word after command (rusk_idx + 2: skip "rusk" and command like "edit")
    local start_idx=$((rusk_idx + 2))
    for ((i=start_idx; i<COMP_CWORD; i++)); do
        if [[ "${COMP_WORDS[i]}" =~ ^[0-9,]+$ ]]; then
            ((count++))
        fi
    done
    echo $count
}

# True when add has at least one completed task-text token (not a flag; skips date value after -d/--date)
_rusk_add_has_task_text() {
    local rusk_idx=-1
    local i
    for ((i=0; i<${#COMP_WORDS[@]}; i++)); do
        if [[ "${COMP_WORDS[i]}" == "rusk" ]]; then
            rusk_idx=$i
            break
        fi
    done
    (( rusk_idx >= 0 )) || return 1
    local cmd="${COMP_WORDS[$((rusk_idx+1))]}"
    [[ "$cmd" == "add" || "$cmd" == "a" ]] || return 1
    local start=$((rusk_idx+2))
    local prev=""
    local w
    for ((i=start; i<COMP_CWORD; i++)); do
        w="${COMP_WORDS[i]}"
        [[ -n "$w" ]] || continue
        if [[ "$prev" == "-d" || "$prev" == "--date" || "$prev" == "-a" || "$prev" == "--after" ]]; then
            prev="$w"
            continue
        fi
        if [[ "$w" == "-d" || "$w" == "--date" || "$w" == "-a" || "$w" == "--after" ]]; then
            prev="$w"
            continue
        fi
        if [[ "$w" == -* ]]; then
            prev="$w"
            continue
        fi
        return 0
    done
    return 1
}

# After -d/--date: offer -h/--help only. Bash may leave the cursor on the -d token (prev=text) or on a new empty word (prev=-d).
_rusk_add_help_after_date_only() {
    [[ "$prev" == "-d" || "$prev" == "--date" || "$prev" == "-a" || "$prev" == "--after" ]] && return 0
    if [[ "$cur" == "-d" || "$cur" == "--date" || "$cur" == "-a" || "$cur" == "--after" ]] && _rusk_add_has_task_text; then
        return 0
    fi
    return 1
}

# True if edit has at least one task ID token (skips -d/--date value)
_rusk_edit_has_task_id() {
    local rusk_idx=-1
    local i
    for ((i=0; i<${#COMP_WORDS[@]}; i++)); do
        if [[ "${COMP_WORDS[i]}" == "rusk" ]]; then
            rusk_idx=$i
            break
        fi
    done
    (( rusk_idx >= 0 )) || return 1
    local cmd="${COMP_WORDS[$((rusk_idx+1))]}"
    [[ "$cmd" == "edit" || "$cmd" == "e" ]] || return 1
    local start=$((rusk_idx+2))
    local prev=""
    local w
    for ((i=start; i<COMP_CWORD; i++)); do
        w="${COMP_WORDS[i]}"
        [[ -n "$w" ]] || continue
        if [[ "$prev" == "-d" || "$prev" == "--date" || "$prev" == "-a" || "$prev" == "--after" ]]; then
            prev="$w"
            continue
        fi
        if [[ "$w" == "-d" || "$w" == "--date" || "$w" == "-a" || "$w" == "--after" ]]; then
            prev="$w"
            continue
        fi
        if [[ "$w" == -* ]]; then
            prev="$w"
            continue
        fi
        if [[ "$w" =~ ^[0-9,]+$ ]]; then
            return 0
        fi
        prev="$w"
    done
    return 1
}

_rusk_edit_help_after_date_only() {
    [[ "$prev" == "-d" || "$prev" == "--date" || "$prev" == "-a" || "$prev" == "--after" ]] && return 0
    if [[ "$cur" == "-d" || "$cur" == "--date" || "$cur" == "-a" || "$cur" == "--after" ]] && _rusk_edit_has_task_id; then
        return 0
    fi
    return 1
}

# The value flags of add/edit not on the line yet: -d/--date and -a/--after
# can each be given once, also as `--date=X` or `-dX`.
_rusk_unused_value_flags() {
    local have_d=0 have_a=0 p="" a j rusk_i=-1
    for ((j=0; j<${#COMP_WORDS[@]}; j++)); do
        if [[ "${COMP_WORDS[j]}" == "rusk" ]]; then
            rusk_i=$j
            break
        fi
    done
    if (( rusk_i >= 0 )); then
        for ((j=rusk_i+2; j<COMP_CWORD; j++)); do
            a="${COMP_WORDS[j]}"
            [[ -n "$a" ]] || continue
            if [[ "$p" == "-d" || "$p" == "--date" || "$p" == "-a" || "$p" == "--after" ]]; then
                p="$a"
                continue
            fi
            case "$a" in
                -d|--date|--date=*|-d?*) have_d=1 ;;
                -a|--after|--after=*|-a?*) have_a=1 ;;
            esac
            p="$a"
        done
    fi
    local flags=""
    (( have_d == 0 )) && flags="-d --date"
    (( have_a == 0 )) && flags="$flags -a --after"
    printf '%s' "$flags"
}

_rusk_complete_add_edit_flags() {
    local gcur="$cur"
    if _rusk_add_has_task_text; then
        COMPREPLY=($(compgen -W "$(_rusk_unused_value_flags) -h --help" -- "$gcur"))
    else
        COMPREPLY=($(compgen -W "-h --help" -- "$gcur"))
    fi
    return 0
}

# Complete flags for edit: -d/--date only after a task id (TUI: first line; CLI: -d <date>)
_rusk_complete_edit_flags() {
    local gcur="$cur"
    if _rusk_edit_has_task_id; then
        COMPREPLY=($(compgen -W "$(_rusk_unused_value_flags) -h --help" -- "$gcur"))
    else
        COMPREPLY=($(compgen -W "-h --help" -- "$gcur"))
    fi
    return 0
}

# Complete flags for del command (`--done` takes no ids: offered only before any)
_rusk_complete_del_flags() {
    local gcur="$cur"
    local flags="--yes -y --help -h"
    [ "$(_rusk_count_ids)" -eq 0 ] && flags="--done $flags"
    COMPREPLY=($(compgen -W "$flags" -- "$gcur"))
    return 0
}

# Complete flags for mark command
_rusk_complete_mark_flags() {
    local gcur="$cur"
    COMPREPLY=($(compgen -W "-p --priority -h --help" -- "$gcur"))
    return 0
}

# Flags for list (compact + help)
_rusk_complete_list_flags() {
    local gcur="$cur"
    COMPREPLY=($(compgen -W "-c --compact --no-compact -h --help" -- "$gcur"))
    return 0
}

# Flags for search (--id + help)
_rusk_complete_search_flags() {
    local gcur="$cur"
    COMPREPLY=($(compgen -W "--id -h --help" -- "$gcur"))
    return 0
}

# Help-only flags for restore
_rusk_complete_help_flags() {
    local gcur="$cur"
    COMPREPLY=($(compgen -W "-h --help" -- "$gcur"))
    return 0
}

# Flags for gen (-o/--output take a file path)
_rusk_complete_gen_flags() {
    local gcur="$cur"
    COMPREPLY=($(compgen -W "-o --output -h --help" -- "$gcur"))
    return 0
}

# Flags for serve (--host/--port take free-form values)
_rusk_complete_serve_flags() {
    local gcur="$cur"
    COMPREPLY=($(compgen -W "--host --port -h --help" -- "$gcur"))
    return 0
}

# Get available shells for completions install/show, excluding already selected ones
_rusk_get_available_shells() {
    local all_shells=("bash" "zsh" "fish" "nu" "powershell")
    local selected=()

    # Find index of install/show in COMP_WORDS
    local i install_idx=-1
    for ((i=1; i<${#COMP_WORDS[@]}; i++)); do
        if [[ "${COMP_WORDS[i]}" == "install" || "${COMP_WORDS[i]}" == "show" ]]; then
            install_idx=$i
            break
        fi
    done

    # Collect already specified shells after install/show
    if (( install_idx >= 0 )); then
        for ((i=install_idx+1; i<${#COMP_WORDS[@]}; i++)); do
            local w="${COMP_WORDS[i]}"
            case " ${all_shells[*]} " in
                *" $w "*)
                    selected+=("$w")
                    ;;
            esac
        done
    fi

    # Output shells that are not yet selected
    local result=()
    for sh in "${all_shells[@]}"; do
        if [[ ! " ${selected[*]} " =~ (^|[[:space:]])"$sh"([[:space:]]|$) ]]; then
            result+=("$sh")
        fi
    done

    echo "${result[*]}"
}

# Whether a `--` comes before the word under the cursor: every word after
# it is text, with nothing to offer.
_rusk_after_double_dash() {
    local j
    for ((j=rusk_idx+2; j<COMP_CWORD; j++)); do
        [[ "${COMP_WORDS[j]}" == "--" ]] && return 0
    done
    return 1
}

_rusk_completion() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local prev=""
    local cmd=""
    
    # Get previous word if available
    if [ $COMP_CWORD -gt 0 ]; then
        prev="${COMP_WORDS[COMP_CWORD-1]}"
    fi
    
    # Find rusk command in COMP_WORDS (skip environment variables like RUSK_DB=./)
    local rusk_idx=-1
    local i
    for ((i=0; i<${#COMP_WORDS[@]}; i++)); do
        if [[ "${COMP_WORDS[i]}" == "rusk" ]]; then
            rusk_idx=$i
            break
        fi
    done
    
    # Get command (word after rusk) if available
    if [ $rusk_idx -ge 0 ] && [ $((rusk_idx + 1)) -lt ${#COMP_WORDS[@]} ]; then
        cmd="${COMP_WORDS[$((rusk_idx + 1))]}"
    fi
    
    # The first word after "rusk" is a command (or a flag of rusk itself),
    # a command typed in full too: Tab ends the word, the next Tab offers
    # what follows it.
    if [ $rusk_idx -ge 0 ] && [ $COMP_CWORD -eq $((rusk_idx + 1)) ]; then
        case "$cur" in
            -*)
                COMPREPLY=($(compgen -W "-h --help -V --version" -- "$cur"))
                ;;
            *)
                COMPREPLY=($(compgen -W "add edit mark del list search restore gen serve sync completions help a e m d l s r g c" -- "$cur"))
                ;;
        esac
        return 0
    fi
    _rusk_after_double_dash && return 0
    
    # Complete subcommands
    case "$cmd" in
        add|a)
            if _rusk_add_help_after_date_only; then
                if [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                    COMPREPLY=($(compgen -W "-h --help" -- "$cur"))
                fi
            elif [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                _rusk_complete_add_edit_flags
            fi
            ;;
            
        edit|e)
            if _rusk_edit_help_after_date_only; then
                if [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                    COMPREPLY=($(compgen -W "-h --help" -- "$cur"))
                fi
            # `rusk edit <id><TAB>` with no space: append task text when available.
            elif [[ "$cur" =~ ^[0-9]+$ ]] && [[ "$prev" == "edit" || "$prev" == "e" ]]; then
                if [ $(_rusk_count_ids) -eq 0 ]; then
                    local task_text=$(_rusk_get_task_text "$cur")
                    if [ -n "$task_text" ]; then
                        COMPREPLY=("$cur $task_text")
                        return 0
                    fi
                fi
            elif [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                _rusk_complete_edit_flags
            fi
            ;;
            
        mark|m|del|d)
            if [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                if [[ "$cmd" == "del" || "$cmd" == "d" ]]; then
                    _rusk_complete_del_flags
                else
                    _rusk_complete_mark_flags
                fi
            fi
            ;;
            
        list|l)
            if [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                _rusk_complete_list_flags
            fi
            ;;
        search|s)
            if [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                _rusk_complete_search_flags
            fi
            ;;
        restore|r)
            if [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                _rusk_complete_help_flags
            fi
            ;;

        gen|g)
            if [[ "$prev" == "-o" || "$prev" == "--output" ]]; then
                # Output value is a file path: offer filesystem completion
                COMPREPLY=($(compgen -f -- "$cur"))
            elif [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                _rusk_complete_gen_flags
            fi
            ;;

        serve)
            if [[ "$prev" == "--host" || "$prev" == "--port" ]]; then
                # Free-form value: no candidates
                :
            elif [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                _rusk_complete_serve_flags
            fi
            ;;

        sync)
            local saw_dir=0
            for ((i=rusk_idx+2; i<${#COMP_WORDS[@]}; i++)); do
                if [[ "${COMP_WORDS[i]}" == "push" || "${COMP_WORDS[i]}" == "pull" ]]; then
                    saw_dir=1
                    break
                fi
            done
            if [[ $saw_dir -eq 1 ]]; then
                if [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                    COMPREPLY=($(compgen -W "--force -h --help" -- "$cur"))
                fi
            else
                COMPREPLY=($(compgen -W "push pull -h --help" -- "$cur"))
            fi
            ;;

        completions|c)
            local sub=""
            for ((i=rusk_idx+2; i<COMP_CWORD; i++)); do
                if [[ "${COMP_WORDS[i]}" == "install" || "${COMP_WORDS[i]}" == "show" ]]; then
                    sub="${COMP_WORDS[i]}"
                    break
                fi
            done
            if [[ -n "$sub" ]]; then
                local shells=$(_rusk_get_available_shells)
                if [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                    COMPREPLY=($(compgen -W "$shells -h --help" -- "$cur"))
                elif [[ -n "$shells" ]]; then
                    COMPREPLY=($(compgen -W "$shells" -- "$cur"))
                fi
            else
                COMPREPLY=($(compgen -W "install show -h --help" -- "$cur"))
            fi
            ;;

        help)
            # `rusk help <command> [<subcommand>]`, nothing more.
            case $((COMP_CWORD - rusk_idx)) in
                2)
                    COMPREPLY=($(compgen -W "add edit mark del list search restore gen serve sync completions" -- "$cur"))
                    ;;
                3)
                    case "${COMP_WORDS[rusk_idx + 2]}" in
                        sync) COMPREPLY=($(compgen -W "push pull" -- "$cur")) ;;
                        completions) COMPREPLY=($(compgen -W "install show" -- "$cur")) ;;
                    esac
                    ;;
            esac
            ;;
    esac
}

complete -F _rusk_completion rusk
