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

# Get list of task IDs from rusk list output
_rusk_get_task_ids() {
    local rusk_cmd=$(_rusk_cmd)
    # Check if RUSK_DB is set in command line (use full command line)
    local rusk_db=""
    if [[ "$COMP_LINE" =~ RUSK_DB=([^\ ]+) ]]; then
        rusk_db="${BASH_REMATCH[1]}"
    fi
    
    if [ -n "$rusk_db" ]; then
        ( export RUSK_DB="$rusk_db"; "$rusk_cmd" list 2>/dev/null ) | grep -E '[•✔]' | grep -oE '^\s+[•✔]\s+[0-9]+\s+' | grep -oE '[0-9]+' | sort -n | tr '\n' ' '
    else
        "$rusk_cmd" list 2>/dev/null | grep -E '[•✔]' | grep -oE '^\s+[•✔]\s+[0-9]+\s+' | grep -oE '[0-9]+' | sort -n | tr '\n' ' '
    fi
}

# Check if text contains special characters that require quoting
_rusk_needs_quotes() {
    local text="$1"
    # Special chars: | ; & > < ( ) [ ] { } $ " ' ` \ * ? ~ # @ ! % ^ = + - / : ,
    # Using case statement for portability and reliability
    case "$text" in
        *[\|\;\&\>\<\(\)\[\]\{\}\$\"\'\`\\*\?\~\#\@\!\%\^\=\+\-\/\:\,]*)
            return 0
            ;;
    esac
    return 1
}

# Check if text contains single quote
_rusk_contains_single_quote() {
    case "$1" in
        *"'"*)
            return 0
            ;;
    esac
    return 1
}

# Quote text if it contains special characters
# Use single quotes if no single quote in text, otherwise use double quotes with escaping
_rusk_quote_text() {
    local text="$1"
    if ! _rusk_needs_quotes "$text"; then
        echo "$text"
        return
    fi
    
    # If no single quote in text, use single quotes (no escaping needed)
    if ! _rusk_contains_single_quote "$text"; then
        echo "'$text'"
    else
        # Use double quotes with escaping
        local escaped="${text//\"/\\\"}"
        # Escape backticks to prevent command substitution
        escaped="${escaped//\`/\\\`}"
        # Escape dollar signs to prevent variable expansion
        escaped="${escaped//\$/\\$}"
        # Escape backslashes
        escaped="${escaped//\\/\\\\}"
        echo "\"$escaped\""
    fi
}

# Get task text by ID (supports multi-line tasks via rusk list --for-completion)
_rusk_get_task_text() {
    local task_id="$1"
    local rusk_cmd=$(_rusk_cmd)
    local rusk_db=""
    if [[ "$COMP_LINE" =~ RUSK_DB=([^\ ]+) ]]; then
        rusk_db="${BASH_REMATCH[1]}"
    fi
    
    local output
    if [ -n "$rusk_db" ]; then
        output=$( ( export RUSK_DB="$rusk_db"; "$rusk_cmd" list --for-completion 2>/dev/null ) )
    else
        output=$("$rusk_cmd" list --for-completion 2>/dev/null)
    fi
    
    local text="" collecting=0 id rest
    while IFS= read -r line; do
        if [[ "$line" =~ ^([0-9]+)$'\t'(.*)$ ]]; then
            id="${BASH_REMATCH[1]}"
            rest="${BASH_REMATCH[2]}"
            if [[ "$id" == "$task_id" ]]; then
                text="$rest"
                collecting=1
            else
                collecting=0
            fi
        elif [[ $collecting -eq 1 ]]; then
            text="${text}"$'\n'"${line}"
        fi
    done <<< "$output"
    
    if [ -n "$text" ]; then
        _rusk_quote_text "$text"
    fi
}

# Get entered task IDs from command line
_rusk_get_entered_ids() {
    local entered_ids=""
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
        if [[ "${COMP_WORDS[i]}" =~ ^[0-9]+$ ]]; then
            entered_ids="$entered_ids ${COMP_WORDS[i]}"
        fi
    done
    echo "$entered_ids"
}

# Filter out already entered IDs from task ID list
_rusk_filter_ids() {
    local ids="$1"
    local entered_ids="$2"
    
    if [ -z "$entered_ids" ]; then
        echo "$ids"
        return
    fi
    
    local filtered_ids=""
    for id in $ids; do
        if [[ ! "$entered_ids" =~ (^|[[:space:]])"$id"([[:space:]]|$) ]]; then
            filtered_ids="$filtered_ids $id"
        fi
    done
    echo "$filtered_ids" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//'
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
        if [[ "${COMP_WORDS[i]}" =~ ^[0-9]+$ ]]; then
            ((count++))
        fi
    done
    echo $count
}

# Complete task IDs with filtering
_rusk_complete_task_ids() {
    local ids=$(_rusk_get_task_ids)
    if [ -z "$ids" ]; then
        return 1
    fi
    
    local entered_ids=$(_rusk_get_entered_ids)
    local filtered_ids=$(_rusk_filter_ids "$ids" "$entered_ids")
    
    if [ -n "$filtered_ids" ]; then
        COMPREPLY=($(compgen -W "$filtered_ids" -- "$cur"))
        return 0
    fi
    return 1
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
        if [[ "$prev" == "-d" || "$prev" == "--date" ]]; then
            prev="$w"
            continue
        fi
        if [[ "$w" == "-d" || "$w" == "--date" ]]; then
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

# Complete flags for add command (-d/--date only after task text)
_rusk_complete_add_edit_flags() {
    if _rusk_add_has_task_text; then
        COMPREPLY=($(compgen -W "-d --date -h --help" -- "$cur"))
    else
        COMPREPLY=($(compgen -W "-h --help" -- "$cur"))
    fi
    return 0
}

# Complete flags for edit command (no -d/--date in tab suggestions; flags still work when typed)
_rusk_complete_edit_flags() {
    COMPREPLY=($(compgen -W "-h --help" -- "$cur"))
    return 0
}

# Complete flags for del command
_rusk_complete_del_flags() {
    COMPREPLY=($(compgen -W "--done --help -h" -- "$cur"))
    return 0
}

# Complete flags for mark command
_rusk_complete_mark_flags() {
    COMPREPLY=($(compgen -W "-h --help" -- "$cur"))
    return 0
}

# Help-only flags for list/restore
_rusk_complete_help_flags() {
    COMPREPLY=($(compgen -W "-h --help" -- "$cur"))
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
    
    # Complete commands (if we're right after rusk command)
    if [ $rusk_idx -ge 0 ] && [ $COMP_CWORD -eq $((rusk_idx + 1)) ]; then
        COMPREPLY=($(compgen -W "add edit mark del list restore completions a e m d l r" -- "$cur"))
        return 0
    fi
    
    # Complete subcommands
    case "$cmd" in
        add|a)
            if [[ "$prev" == "--date" ]] || [[ "$prev" == "-d" ]]; then
                if [[ -z "$cur" ]]; then
                    COMPREPLY=($(compgen -W "-h --help" -- "$cur"))
                fi
            elif [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                _rusk_complete_add_edit_flags
            fi
            ;;
            
        edit|e)
            # After single ID with a space: suggest ONLY flags
            if [[ "$prev" =~ ^[0-9]+$ ]] && [[ -z "$cur" ]]; then
                if [ $(_rusk_count_ids) -eq 1 ]; then
                    _rusk_complete_edit_flags
                    return 0
                fi
            fi

            # Support completion without a space after ID: `rusk edit <id><TAB>`
            # Here `$cur` is the numeric ID being edited; we append task text after it.
            if [[ "$cur" =~ ^[0-9]+$ ]] && ([[ "$prev" == "edit" ]] || [[ "$prev" == "e" ]]); then
                if [ $(_rusk_count_ids) -eq 0 ]; then
                    local task_text=$(_rusk_get_task_text "$cur")
                    if [ -n "$task_text" ]; then
                        COMPREPLY=("$cur $task_text")
                        return 0
                    fi
                fi
            fi
            
            if [[ "$prev" == "--date" ]] || [[ "$prev" == "-d" ]]; then
                if [[ -z "$cur" ]]; then
                    COMPREPLY=($(compgen -W "-h --help" -- "$cur"))
                fi
            elif [[ "$cur" == -* ]]; then
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
            
        list|l|restore|r)
            if [[ -z "$cur" ]] || [[ "$cur" == -* ]]; then
                _rusk_complete_help_flags
            fi
            ;;
            
        completions)
            if [[ "$prev" == "completions" ]]; then
                COMPREPLY=($(compgen -W "install show" -- "$cur"))
            else
                # After install/show, suggest only shells that haven't been used yet
                local shells=$(_rusk_get_available_shells)
                if [[ -n "$shells" ]]; then
                    COMPREPLY=($(compgen -W "$shells" -- "$cur"))
                fi
            fi
            ;;
    esac
}

complete -F _rusk_completion rusk
